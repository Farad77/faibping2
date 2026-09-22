//! Network Interception Engine
//!
//! Captures outbound non-loopback TCP and UDP packets using WinDivert.
//! Performs Strict Split-Tunneling: filters packets by comparing local socket ports
//! against the game PID's active ports table. System traffic (Discord, browser, etc.)
//! passes through untouched.

use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tracing::{info, warn, error};

use crate::config::GameProfile;
use crate::watcher::ProcessState;

pub struct InterceptedPacket {
    pub raw: Vec<u8>,
    pub is_tcp: bool,
    pub src_port: u16,
    pub dst_port: u16,
}

pub struct InterceptionEngine {
    profile: GameProfile,
    process_state_rx: watch::Receiver<ProcessState>,
    outbound_tx: mpsc::Sender<InterceptedPacket>,
    inbound_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

impl InterceptionEngine {
    pub fn new(
        profile: GameProfile,
        process_state_rx: watch::Receiver<ProcessState>,
        outbound_tx: mpsc::Sender<InterceptedPacket>,
        inbound_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            profile,
            process_state_rx,
            outbound_tx,
            inbound_rx: Arc::new(Mutex::new(inbound_rx)),
        }
    }

    pub async fn start(&self) {
        let profile = self.profile.clone();
        let mut state_rx = self.process_state_rx.clone();
        let tx = self.outbound_tx.clone();
        let inbound_rx = Arc::clone(&self.inbound_rx);

        info!("[InterceptionEngine] Starting WinDivert filter: 'outbound and !loopback and (tcp or udp)'");

        // Background worker loop
        tokio::task::spawn_blocking(move || {
            run_divert_loop(profile, &mut state_rx, tx, inbound_rx);
        });
    }
}

fn run_divert_loop(
    profile: GameProfile,
    state_rx: &mut watch::Receiver<ProcessState>,
    outbound_tx: mpsc::Sender<InterceptedPacket>,
    _inbound_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
) {
    #[cfg(windows)]
    {
        use std::ffi::CString;

        // Try dynamically loading WinDivert.dll
        let dll_name = CString::new("WinDivert.dll").unwrap();
        let h_module = unsafe {
            windows_sys::Win32::System::LibraryLoader::LoadLibraryA(dll_name.as_ptr() as *const u8)
        };

        if h_module == 0 {
            warn!("[WinDivert] WinDivert.dll not found in application directory or PATH.");
            warn!("[WinDivert] Running in synthetic emulation mode for local development.");
            run_emulation_loop(profile, state_rx, outbound_tx);
            return;
        }

        info!("[WinDivert] WinDivert.dll loaded successfully. Initializing kernel divert handle...");

        type WinDivertOpenFn = unsafe extern "system" fn(
            filter: *const i8,
            layer: i32,
            priority: i16,
            flags: u64,
        ) -> *mut std::ffi::c_void;

        type WinDivertRecvFn = unsafe extern "system" fn(
            handle: *mut std::ffi::c_void,
            p_packet: *mut u8,
            packet_len: u32,
            p_addr: *mut u8,
            p_read_len: *mut u32,
        ) -> i32;

        type WinDivertSendFn = unsafe extern "system" fn(
            handle: *mut std::ffi::c_void,
            p_packet: *const u8,
            packet_len: u32,
            p_addr: *const u8,
            p_write_len: *mut u32,
        ) -> i32;

        type WinDivertCloseFn = unsafe extern "system" fn(handle: *mut std::ffi::c_void) -> i32;

        let open_sym = CString::new("WinDivertOpen").unwrap();
        let recv_sym = CString::new("WinDivertRecv").unwrap();
        let send_sym = CString::new("WinDivertSend").unwrap();
        let close_sym = CString::new("WinDivertClose").unwrap();

        unsafe {
            let open_fn: WinDivertOpenFn = std::mem::transmute(
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(h_module, open_sym.as_ptr() as *const u8),
            );
            let recv_fn: WinDivertRecvFn = std::mem::transmute(
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(h_module, recv_sym.as_ptr() as *const u8),
            );
            let send_fn: WinDivertSendFn = std::mem::transmute(
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(h_module, send_sym.as_ptr() as *const u8),
            );
            let close_fn: WinDivertCloseFn = std::mem::transmute(
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(h_module, close_sym.as_ptr() as *const u8),
            );

            let filter = CString::new("outbound and !loopback and (tcp or udp)").unwrap();
            let handle = open_fn(filter.as_ptr(), 0 /* NETWORK layer */, 0, 0);

            if handle.is_null() {
                error!("[WinDivert] Failed to open WinDivert handle (requires Administrator privileges).");
                run_emulation_loop(profile, state_rx, outbound_tx);
                return;
            }

            info!("[WinDivert] Kernel packet capture active.");

            let mut packet_buf = vec![0u8; 65535];
            let mut addr_buf = [0u8; 64];
            let mut read_len = 0u32;

            loop {
                let ok = recv_fn(
                    handle,
                    packet_buf.as_mut_ptr(),
                    packet_buf.len() as u32,
                    addr_buf.as_mut_ptr(),
                    &mut read_len,
                );

                if ok == 0 {
                    break;
                }

                let packet = &packet_buf[..read_len as usize];
                if let Some((is_tcp, src_port, dst_port)) = parse_l4_ports(packet) {
                    let state = state_rx.borrow().clone();
                    let matches_game = state.tracked_ports.contains(&src_port)
                        || (state.is_running && profile.matches_port(dst_port));

                    if matches_game {
                        // Game packet: forward to GameTunnel overlay
                        let intercepted = InterceptedPacket {
                            raw: packet.to_vec(),
                            is_tcp,
                            src_port,
                            dst_port,
                        };
                        let _ = outbound_tx.blocking_send(intercepted);
                        // Do not re-inject outbound game packet into local NIC
                    } else {
                        // Strict Split-Tunneling: Pass system traffic back to Windows network stack immediately!
                        let mut write_len = 0u32;
                        send_fn(handle, packet.as_ptr(), read_len, addr_buf.as_ptr(), &mut write_len);
                    }
                } else {
                    // Non-TCP/UDP or unrecognized: pass through
                    let mut write_len = 0u32;
                    send_fn(handle, packet.as_ptr(), read_len, addr_buf.as_ptr(), &mut write_len);
                }
            }

            close_fn(handle);
        }
    }

    #[cfg(not(windows))]
    {
        run_emulation_loop(profile, state_rx, outbound_tx);
    }
}

fn run_emulation_loop(
    _profile: GameProfile,
    _state_rx: &mut watch::Receiver<ProcessState>,
    _outbound_tx: mpsc::Sender<InterceptedPacket>,
) {
    info!("[InterceptionEngine] Emulation loop standby.");
    std::thread::sleep(std::time::Duration::from_secs(u64::MAX));
}

/// Parse IPv4 header to extract Protocol, Source Port and Destination Port
pub fn parse_l4_ports(packet: &[u8]) -> Option<(bool, u16, u16)> {
    if packet.len() < 20 {
        return None;
    }
    let version = (packet[0] >> 4) & 0x0F;
    if version != 4 {
        return None; // IPv4 only for primary game routing
    }

    let ihl = (packet[0] & 0x0F) as usize * 4;
    if packet.len() < ihl + 4 {
        return None;
    }

    let protocol = packet[9];
    let is_tcp = protocol == 6;
    let is_udp = protocol == 17;

    if !is_tcp && !is_udp {
        return None;
    }

    let src_port = u16::from_be_bytes([packet[ihl], packet[ihl + 1]]);
    let dst_port = u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]);

    Some((is_tcp, src_port, dst_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_l4_ports() {
        // Construct minimal IPv4 UDP packet
        let mut packet = vec![0u8; 28];
        packet[0] = 0x45; // IPv4, IHL = 5 (20 bytes)
        packet[9] = 17;   // UDP
        packet[20..22].copy_from_slice(&50000u16.to_be_bytes()); // Src port 50000
        packet[22..24].copy_from_slice(&7777u16.to_be_bytes());  // Dst port 7777

        let parsed = parse_l4_ports(&packet);
        assert_eq!(parsed, Some((false, 50000, 7777)));
    }
}
