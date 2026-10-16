//! Process Watcher Engine
//!
//! Continuously scans Windows running processes every 500 ms using Toolhelp32 API.
//! Identifies target game executable PIDs (e.g., Aion 2) and enumerates active TCP/UDP
//! endpoints via GetExtendedTcpTable / GetExtendedUdpTable to dynamically populate
//! the interceptor's socket map and discover the live remote game server IP:Port.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info};

use crate::config::GameProfile;

#[derive(Debug, Clone, Default)]
pub struct ProcessState {
    pub is_running: bool,
    pub pids: Vec<u32>,
    pub tracked_ports: HashSet<u16>,
    pub active_remote_endpoint: Option<SocketAddr>,
}

pub struct ProcessWatcher {
    profile: GameProfile,
    state_tx: watch::Sender<ProcessState>,
    state_rx: watch::Receiver<ProcessState>,
}

impl ProcessWatcher {
    pub fn new(profile: GameProfile) -> Self {
        let (state_tx, state_rx) = watch::channel(ProcessState::default());
        Self {
            profile,
            state_tx,
            state_rx,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<ProcessState> {
        self.state_rx.clone()
    }

    pub async fn start(&self) {
        let profile = self.profile.clone();
        let tx = self.state_tx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            let mut previously_detected = false;
            let mut boosted_pids = HashSet::new();

            loop {
                interval.tick().await;

                let state = scan_game_state(&profile, &mut boosted_pids);

                if state.is_running != previously_detected {
                    if state.is_running {
                        info!(
                            "[ProcessWatcher] Target game detected! Matching PIDs: {:?} | Server: {:?}",
                            state.pids, state.active_remote_endpoint
                        );
                    } else {
                        info!("[ProcessWatcher] Target game closed.");
                        boosted_pids.clear();
                    }
                    previously_detected = state.is_running;
                }

                let _ = tx.send(state);
            }
        });
    }
}

/// Scans running processes for the given GameProfile, elevates priority for newly found game PIDs,
/// and discovers all local ports plus the primary remote game server endpoint.
pub fn scan_game_state(profile: &GameProfile, boosted_pids: &mut HashSet<u32>) -> ProcessState {
    let pids = scan_target_processes(profile);
    let is_running = !pids.is_empty();

    let mut tracked_ports = HashSet::new();
    let mut best_remote: Option<(u8, SocketAddr)> = None;

    if is_running {
        for &pid in &pids {
            if boosted_pids.insert(pid) {
                boost_process_priority(pid);
            }
            let (ports, remote_candidate) = get_process_sockets(pid, profile);
            tracked_ports.extend(ports);
            if let Some((prio, addr)) = remote_candidate {
                if best_remote.map_or(true, |(cur_prio, _)| prio > cur_prio) {
                    best_remote = Some((prio, addr));
                }
            }
        }
        debug!(
            "[ProcessWatcher] Active game socket ports: {:?}, Remote: {:?}",
            tracked_ports,
            best_remote.map(|(_, a)| a)
        );
    }

    ProcessState {
        is_running,
        pids,
        tracked_ports,
        active_remote_endpoint: best_remote.map(|(_, addr)| addr),
    }
}

#[cfg(windows)]
fn boost_process_priority(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn SetPriorityClass(hProcess: isize, dwPriorityClass: u32) -> i32;
    }
    const PROCESS_SET_INFORMATION: u32 = 0x0200;
    const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x00008000;

    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle != 0 {
            if SetPriorityClass(handle, ABOVE_NORMAL_PRIORITY_CLASS) != 0 {
                info!("[FPSBoost] Elevated CPU scheduling priority (ABOVE_NORMAL) for game PID {}", pid);
            }
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn boost_process_priority(_pid: u32) {}

#[cfg(windows)]
fn scan_target_processes(profile: &GameProfile) -> Vec<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut matched_pids = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return matched_pids;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let null_pos = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe_name = String::from_utf16_lossy(&entry.szExeFile[..null_pos]);

                if profile.matches_executable(&exe_name) {
                    matched_pids.push(entry.th32ProcessID);
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }

    matched_pids
}

#[cfg(not(windows))]
fn scan_target_processes(_profile: &GameProfile) -> Vec<u32> {
    vec![1337]
}

#[cfg(windows)]
fn get_process_sockets(
    target_pid: u32,
    profile: &GameProfile,
) -> (HashSet<u16>, Option<(u8, SocketAddr)>) {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    let mut ports = HashSet::new();
    let mut best_remote: Option<(u8, SocketAddr)> = None;

    unsafe {
        // Query TCP Table
        let mut size = 0u32;
        let _ = GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );

        if size > 0 {
            size += 512;
            let mut buffer = vec![0u8; size as usize];
            if GetExtendedTcpTable(
                buffer.as_mut_ptr() as *mut _,
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            ) == 0
            {
                let num_entries = *(buffer.as_ptr() as *const u32);
                let rows_ptr = buffer.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID;
                for i in 0..num_entries as usize {
                    let row = &*rows_ptr.add(i);
                    if row.dwOwningPid == target_pid {
                        let local_port = u16::from_be(row.dwLocalPort as u16);
                        ports.insert(local_port);

                        // MIB_TCP_STATE_ESTAB == 5
                        if row.dwState == 5 {
                            let remote_ip = row.dwRemoteAddr.to_ne_bytes();
                            let remote_port = u16::from_be(row.dwRemotePort as u16);

                            let is_private_or_loopback = remote_ip[0] == 0
                                || remote_ip[0] == 127
                                || remote_ip[0] == 10
                                || (remote_ip[0] == 192 && remote_ip[1] == 168);

                            if !is_private_or_loopback && remote_port > 0 {
                                let prio = if remote_port != 80
                                    && remote_port != 443
                                    && profile.matches_port(remote_port)
                                {
                                    3
                                } else if remote_port != 80 && remote_port != 443 {
                                    2
                                } else if remote_port == 443 {
                                    1
                                } else {
                                    0
                                };

                                if prio > 0 && best_remote.map_or(true, |(cur, _)| prio > cur) {
                                    let addr = SocketAddr::V4(SocketAddrV4::new(
                                        Ipv4Addr::from(remote_ip),
                                        remote_port,
                                    ));
                                    best_remote = Some((prio, addr));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Query UDP Table
        let mut udp_size = 0u32;
        let _ = GetExtendedUdpTable(
            std::ptr::null_mut(),
            &mut udp_size,
            0,
            AF_INET as u32,
            UDP_TABLE_OWNER_PID,
            0,
        );

        if udp_size > 0 {
            udp_size += 512;
            let mut buffer = vec![0u8; udp_size as usize];
            if GetExtendedUdpTable(
                buffer.as_mut_ptr() as *mut _,
                &mut udp_size,
                0,
                AF_INET as u32,
                UDP_TABLE_OWNER_PID,
                0,
            ) == 0
            {
                let num_entries = *(buffer.as_ptr() as *const u32);
                let rows_ptr = buffer.as_ptr().add(4) as *const MIB_UDPROW_OWNER_PID;
                for i in 0..num_entries as usize {
                    let row = &*rows_ptr.add(i);
                    if row.dwOwningPid == target_pid {
                        let local_port = u16::from_be(row.dwLocalPort as u16);
                        ports.insert(local_port);
                    }
                }
            }
        }
    }

    (ports, best_remote)
}

/// Fast synchronous check called on outbound TCP SYN packets to determine if a newly
/// created local TCP port belongs to one of the target game PIDs at t=0ms (before the
/// 500ms polling loop has updated `tracked_ports`).
#[cfg(windows)]
pub fn is_tcp_port_owned_by_pids(local_port: u16, target_pids: &[u32]) -> bool {
    if target_pids.is_empty() {
        return false;
    }
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    unsafe {
        let mut size = 0u32;
        let _ = GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
        if size == 0 {
            return false;
        }
        size += 512;
        let mut buffer = vec![0u8; size as usize];
        if GetExtendedTcpTable(
            buffer.as_mut_ptr() as *mut _,
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        ) == 0
        {
            let num_entries = *(buffer.as_ptr() as *const u32);
            let rows_ptr = buffer.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID;
            for i in 0..num_entries as usize {
                let row = &*rows_ptr.add(i);
                let port = u16::from_be(row.dwLocalPort as u16);
                if port == local_port && target_pids.contains(&row.dwOwningPid) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(not(windows))]
pub fn is_tcp_port_owned_by_pids(_local_port: u16, _target_pids: &[u32]) -> bool {
    false
}

#[cfg(windows)]
#[allow(non_snake_case)]
#[repr(C)]
struct MIB_TCPROW_OWNER_PID {
    dwState: u32,
    dwLocalAddr: u32,
    dwLocalPort: u32,
    dwRemoteAddr: u32,
    dwRemotePort: u32,
    dwOwningPid: u32,
}

#[cfg(windows)]
#[allow(non_snake_case)]
#[repr(C)]
struct MIB_UDPROW_OWNER_PID {
    dwLocalAddr: u32,
    dwLocalPort: u32,
    dwOwningPid: u32,
}

#[cfg(not(windows))]
fn get_process_sockets(
    _target_pid: u32,
    _profile: &GameProfile,
) -> (HashSet<u16>, Option<(u8, SocketAddr)>) {
    let mut s = HashSet::new();
    s.insert(3724);
    s.insert(7777);
    (s, None)
}
