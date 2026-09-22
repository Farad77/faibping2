//! Process Watcher Engine
//!
//! Continuously scans Windows running processes every 500 ms using Toolhelp32 API.
//! Identifies target game executable PIDs (e.g., Aion 2) and enumerates active TCP/UDP
//! endpoints via GetExtendedTcpTable / GetExtendedUdpTable to dynamically populate
//! the interceptor's socket map.

use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, debug};

use crate::config::GameProfile;

#[derive(Debug, Clone, Default)]
pub struct ProcessState {
    pub is_running: bool,
    pub pids: Vec<u32>,
    pub tracked_ports: HashSet<u16>,
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

            loop {
                interval.tick().await;

                let pids = scan_target_processes(&profile);
                let is_running = !pids.is_empty();

                if is_running != previously_detected {
                    if is_running {
                        info!("[ProcessWatcher] Target game detected! Matching PIDs: {:?}", pids);
                    } else {
                        info!("[ProcessWatcher] Target game closed.");
                    }
                    previously_detected = is_running;
                }

                let mut tracked_ports = HashSet::new();
                if is_running {
                    for pid in &pids {
                        let ports = get_process_ports(*pid);
                        tracked_ports.extend(ports);
                    }
                    debug!("[ProcessWatcher] Active game socket ports: {:?}", tracked_ports);
                }

                let state = ProcessState {
                    is_running,
                    pids,
                    tracked_ports,
                };

                let _ = tx.send(state);
            }
        });
    }
}

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
                let null_pos = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
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
    // Cross-platform mock for CI or test environments
    vec![1337]
}

#[cfg(windows)]
fn get_process_ports(target_pid: u32) -> HashSet<u16> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    let mut ports = HashSet::new();

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

    ports
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
fn get_process_ports(_target_pid: u32) -> HashSet<u16> {
    let mut s = HashSet::new();
    s.insert(3724);
    s.insert(7777);
    s
}
