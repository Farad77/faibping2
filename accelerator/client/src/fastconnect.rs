//! FastConnect Engine (Anti Animation-Lock for Aion 2)
//!
//! MMORPG game engines like Aion 2 tie client skill animations to server-side TCP ACKs.
//! When `enable_fastconnect` is true, this module intercepts outbound TCP data segments,
//! synthesizes an immediate local TCP ACK (< 1ms latency) directly back to the game process,
//! and simultaneously relays the payload over the low-latency GameTunnel to the VPS.

use tracing::{debug, info};

pub struct FastConnectEngine {
    enabled: bool,
}

impl FastConnectEngine {
    pub fn new(enabled: bool) -> Self {
        if enabled {
            info!("[FastConnect] Engine ACTIVE: Local immediate TCP ACK enabled.");
        } else {
            info!("[FastConnect] Engine disabled.");
        }
        Self { enabled }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Generates an immediate local TCP ACK for an intercepted outbound TCP data packet.
    /// Returns the synthesized raw IPv4+TCP ACK packet if the original packet contained TCP payload.
    pub fn synthesize_local_ack(&self, ip_packet: &[u8]) -> Option<Vec<u8>> {
        if !self.enabled || ip_packet.len() < 40 {
            return None;
        }

        let ihl = (ip_packet[0] & 0x0F) as usize * 4;
        if ip_packet[9] != 6 || ip_packet.len() < ihl + 20 {
            return None; // Not TCP
        }

        let tcp_offset = ihl;
        let tcp_header_len = ((ip_packet[tcp_offset + 12] >> 4) & 0x0F) as usize * 4;
        let total_headers_len = tcp_offset + tcp_header_len;

        if ip_packet.len() <= total_headers_len {
            // Pure ACK or SYN without payload data, no need to synthesize extra ACK
            return None;
        }

        let payload_len = (ip_packet.len() - total_headers_len) as u32;

        let src_ip = &ip_packet[12..16];
        let dst_ip = &ip_packet[16..20];
        let src_port = &ip_packet[tcp_offset..tcp_offset + 2];
        let dst_port = &ip_packet[tcp_offset + 2..tcp_offset + 4];

        let seq_num = u32::from_be_bytes([
            ip_packet[tcp_offset + 4],
            ip_packet[tcp_offset + 5],
            ip_packet[tcp_offset + 6],
            ip_packet[tcp_offset + 7],
        ]);

        let ack_num = seq_num.wrapping_add(payload_len);

        // Build 40-byte IPv4 + TCP ACK packet (20 bytes IP, 20 bytes TCP)
        let mut ack_packet = vec![0u8; 40];

        // 1. IPv4 Header
        ack_packet[0] = 0x45; // Version 4, IHL 5 (20 bytes)
        ack_packet[1] = 0x00; // DSCP
        ack_packet[2..4].copy_from_slice(&40u16.to_be_bytes()); // Total length
        ack_packet[4..6].copy_from_slice(&0u16.to_be_bytes()); // Identification
        ack_packet[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF flag
        ack_packet[8] = 64;   // TTL
        ack_packet[9] = 6;    // TCP
        ack_packet[10..12].copy_from_slice(&[0, 0]); // Checksum (computed below)
        ack_packet[12..16].copy_from_slice(dst_ip); // Source is server
        ack_packet[16..20].copy_from_slice(src_ip); // Dest is client

        // Compute IP checksum
        let ip_csum = compute_ip_checksum(&ack_packet[..20]);
        ack_packet[10..12].copy_from_slice(&ip_csum.to_be_bytes());

        let server_seq = &ip_packet[tcp_offset + 8..tcp_offset + 12];

        // 2. TCP Header
        ack_packet[20..22].copy_from_slice(dst_port); // Source port
        ack_packet[22..24].copy_from_slice(src_port); // Dest port
        ack_packet[24..28].copy_from_slice(server_seq); // Sequence num matches server stream
        ack_packet[28..32].copy_from_slice(&ack_num.to_be_bytes()); // Acknowledgment num
        ack_packet[32] = 0x50; // Data offset: 5 (20 bytes)
        ack_packet[33] = 0x10; // Flags: ACK (bit 4)
        ack_packet[34..36].copy_from_slice(&65535u16.to_be_bytes()); // Window size
        ack_packet[36..38].copy_from_slice(&[0, 0]); // Checksum (computed below)
        ack_packet[38..40].copy_from_slice(&[0, 0]); // Urgent pointer

        // Compute TCP checksum over pseudo-header + TCP segment
        let tcp_csum = compute_tcp_checksum(&ack_packet[12..16], &ack_packet[16..20], &ack_packet[20..40]);
        ack_packet[36..38].copy_from_slice(&tcp_csum.to_be_bytes());

        debug!(
            "[FastConnect] Synthesized immediate ACK for seq {} + len {} -> ack {}",
            seq_num, payload_len, ack_num
        );

        Some(ack_packet)
    }
}

fn compute_ip_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;
    for i in (0..header.len()).step_by(2) {
        let word = u16::from_be_bytes([header[i], header[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn compute_tcp_checksum(src_ip: &[u8], dst_ip: &[u8], tcp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;

    // Pseudo-header: Src IP + Dst IP + Zero + Protocol (6) + TCP Length
    for i in (0..4).step_by(2) {
        sum = sum.wrapping_add(u16::from_be_bytes([src_ip[i], src_ip[i + 1]]) as u32);
        sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[i], dst_ip[i + 1]]) as u32);
    }
    sum = sum.wrapping_add(6u32); // Protocol TCP
    sum = sum.wrapping_add(tcp_segment.len() as u32);

    // TCP Segment
    for i in (0..tcp_segment.len()).step_by(2) {
        let word = if i + 1 < tcp_segment.len() {
            u16::from_be_bytes([tcp_segment[i], tcp_segment[i + 1]]) as u32
        } else {
            u16::from_be_bytes([tcp_segment[i], 0]) as u32
        };
        sum = sum.wrapping_add(word);
    }

    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fastconnect_ack_synthesis() {
        let engine = FastConnectEngine::new(true);

        // Construct fake IPv4 + TCP packet with 10 bytes payload
        let mut pkt = vec![0u8; 50];
        pkt[0] = 0x45; // IPv4, IHL 5
        pkt[9] = 6;    // TCP
        pkt[12..16].copy_from_slice(&[192, 168, 1, 100]); // Client
        pkt[16..20].copy_from_slice(&[51, 89, 44, 12]);   // Server
        pkt[20..22].copy_from_slice(&45000u16.to_be_bytes()); // Client port
        pkt[22..24].copy_from_slice(&7777u16.to_be_bytes());  // Server port
        pkt[24..28].copy_from_slice(&1000u32.to_be_bytes());  // Seq 1000
        pkt[32] = 0x50; // TCP header 20 bytes (offset 5)
        // 50 - 40 = 10 bytes payload

        let ack = engine.synthesize_local_ack(&pkt).expect("Should synthesize ACK");
        assert_eq!(ack.len(), 40);
        // Dest port in ACK should be client port 45000
        let ack_dst_port = u16::from_be_bytes([ack[22], ack[23]]);
        assert_eq!(ack_dst_port, 45000);
        // Ack number should be 1000 + 10 = 1010
        let ack_num = u32::from_be_bytes([ack[28], ack[29], ack[30], ack[31]]);
        assert_eq!(ack_num, 1010);
    }
}
