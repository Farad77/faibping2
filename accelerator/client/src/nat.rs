//! Virtual Tunnel NAT (1:1 Endpoint Translation)
//!
//! Translates client LAN IP (e.g. 192.168.1.x) to the virtual tunnel subnet (10.8.0.2)
//! so the Linux VPS gateway (tun-game: 10.8.0.1/24) can properly route return packets.
//! Automatically restores client destination IP and recalculates IPv4/TCP/UDP checksums
//! on return packets before injecting into the Windows TCP/IP stack.

use std::collections::HashMap;
use std::sync::Mutex;
use accelerator_common::checksum::recalculate_checksums;

pub struct TunnelNat {
    virtual_ip: [u8; 4],
    mappings: Mutex<HashMap<(u16, [u8; 4], u16), [u8; 4]>>,
    last_src_ip: Mutex<[u8; 4]>,
}

impl TunnelNat {
    pub fn new(virtual_ip: [u8; 4]) -> Self {
        Self {
            virtual_ip,
            mappings: Mutex::new(HashMap::new()),
            last_src_ip: Mutex::new([192, 168, 1, 1]),
        }
    }

    /// Translates an outbound packet: rewrites source IP to virtual_ip (10.8.0.2)
    /// and records mapping to restore it upon return.
    pub fn translate_outbound(&self, packet: &mut [u8]) -> bool {
        if packet.len() < 20 || (packet[0] >> 4) != 4 {
            return false;
        }

        let ihl = (packet[0] & 0x0F) as usize * 4;
        let proto = packet[9];

        let orig_src_ip: [u8; 4] = [packet[12], packet[13], packet[14], packet[15]];
        let dst_ip: [u8; 4] = [packet[16], packet[17], packet[18], packet[19]];

        *self.last_src_ip.lock().unwrap() = orig_src_ip;

        if (proto == 6 /* TCP */ || proto == 17 /* UDP */) && packet.len() >= ihl + 4 {
            let src_port = u16::from_be_bytes([packet[ihl], packet[ihl + 1]]);
            let dst_port = u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]);

            let mut map = self.mappings.lock().unwrap();
            map.insert((src_port, dst_ip, dst_port), orig_src_ip);
        }

        // Rewrite source IP to virtual tunnel IP
        packet[12..16].copy_from_slice(&self.virtual_ip);

        // Recalculate IP & L4 checksums
        recalculate_checksums(packet)
    }

    /// Translates an inbound return packet: rewrites destination IP back to client's local IP.
    pub fn translate_inbound(&self, packet: &mut [u8]) -> bool {
        if packet.len() < 20 || (packet[0] >> 4) != 4 {
            return false;
        }

        let ihl = (packet[0] & 0x0F) as usize * 4;
        let proto = packet[9];

        let src_ip: [u8; 4] = [packet[12], packet[13], packet[14], packet[15]];

        let target_ip = if (proto == 6 /* TCP */ || proto == 17 /* UDP */) && packet.len() >= ihl + 4 {
            let src_port = u16::from_be_bytes([packet[ihl], packet[ihl + 1]]);
            let dst_port = u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]);

            let map = self.mappings.lock().unwrap();
            map.get(&(dst_port, src_ip, src_port)).copied()
        } else {
            None
        };

        let client_ip = target_ip.unwrap_or_else(|| *self.last_src_ip.lock().unwrap());

        // Rewrite destination IP to original client IP
        packet[16..20].copy_from_slice(&client_ip);

        // Recalculate IP & L4 checksums
        recalculate_checksums(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_nat_roundtrip() {
        let nat = TunnelNat::new([10, 8, 0, 2]);

        // 1. Client creates outbound TCP packet
        let mut pkt = vec![0u8; 40];
        pkt[0] = 0x45;
        pkt[9] = 6; // TCP
        pkt[12..16].copy_from_slice(&[192, 168, 1, 229]); // Client local IP
        pkt[16..20].copy_from_slice(&[188, 42, 43, 98]);  // Game server IP
        pkt[20..22].copy_from_slice(&55000u16.to_be_bytes()); // Client port
        pkt[22..24].copy_from_slice(&6112u16.to_be_bytes());  // Server port
        pkt[32] = 0x50;

        assert!(nat.translate_outbound(&mut pkt));
        // Source IP should now be 10.8.0.2
        assert_eq!(&pkt[12..16], &[10, 8, 0, 2]);

        // 2. Server replies: src=188.42.43.98:6112, dst=10.8.0.2:55000
        let mut reply = vec![0u8; 40];
        reply[0] = 0x45;
        reply[9] = 6;
        reply[12..16].copy_from_slice(&[188, 42, 43, 98]);
        reply[16..20].copy_from_slice(&[10, 8, 0, 2]); // Virtual IP
        reply[20..22].copy_from_slice(&6112u16.to_be_bytes()); // Server port
        reply[22..24].copy_from_slice(&55000u16.to_be_bytes()); // Client port
        reply[32] = 0x50;

        assert!(nat.translate_inbound(&mut reply));
        // Destination IP should be restored to 192.168.1.229
        assert_eq!(&reply[16..20], &[192, 168, 1, 229]);
    }
}
