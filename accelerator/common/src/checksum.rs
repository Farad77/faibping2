//! Network Packet Checksum Calculation & Recalculation
//!
//! Provides RFC 1071 / RFC 1624 compliant 16-bit one's complement checksum algorithms
//! for IPv4, TCP, and UDP packets. Used during IP address translation (NAT)
//! and packet synthesis (FastConnect).

/// Computes standard 16-bit one's complement Internet Checksum (RFC 1071).
pub fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u16::from_be_bytes([chunk[0], chunk[1]]) as u32);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        sum = sum.wrapping_add(u16::from_be_bytes([remainder[0], 0]) as u32);
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Computes IPv4 header checksum over the first `ihl * 4` bytes.
pub fn compute_ip_checksum(header: &[u8]) -> u16 {
    compute_checksum(header)
}

/// Computes TCP checksum covering pseudo-header + TCP segment.
pub fn compute_tcp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;

    // Pseudo-header: Src IP + Dst IP
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32);

    // Protocol: 6 (TCP)
    sum = sum.wrapping_add(6u32);

    // TCP Length
    sum = sum.wrapping_add(tcp_segment.len() as u32);

    // TCP segment data
    let mut chunks = tcp_segment.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u16::from_be_bytes([chunk[0], chunk[1]]) as u32);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        sum = sum.wrapping_add(u16::from_be_bytes([remainder[0], 0]) as u32);
    }

    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Computes UDP checksum covering pseudo-header + UDP segment.
pub fn compute_udp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], udp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;

    // Pseudo-header: Src IP + Dst IP
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32);

    // Protocol: 17 (UDP)
    sum = sum.wrapping_add(17u32);

    // UDP Length
    sum = sum.wrapping_add(udp_segment.len() as u32);

    // UDP segment data
    let mut chunks = udp_segment.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u16::from_be_bytes([chunk[0], chunk[1]]) as u32);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        sum = sum.wrapping_add(u16::from_be_bytes([remainder[0], 0]) as u32);
    }

    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let res = !(sum as u16);
    if res == 0 {
        0xFFFF // In UDP, checksum 0 is transmitted as 0xFFFF (0 means disabled)
    } else {
        res
    }
}

/// Recalculates both the IPv4 header checksum and L4 (TCP/UDP) checksum in-place.
pub fn recalculate_checksums(ip_packet: &mut [u8]) -> bool {
    if ip_packet.len() < 20 || (ip_packet[0] >> 4) != 4 {
        return false;
    }

    let ihl = (ip_packet[0] & 0x0F) as usize * 4;
    if ip_packet.len() < ihl {
        return false;
    }

    let proto = ip_packet[9];
    let src_ip: [u8; 4] = [ip_packet[12], ip_packet[13], ip_packet[14], ip_packet[15]];
    let dst_ip: [u8; 4] = [ip_packet[16], ip_packet[17], ip_packet[18], ip_packet[19]];

    // 1. Recalculate IPv4 header checksum
    ip_packet[10] = 0;
    ip_packet[11] = 0;
    let ip_csum = compute_ip_checksum(&ip_packet[..ihl]);
    ip_packet[10..12].copy_from_slice(&ip_csum.to_be_bytes());

    // 2. Recalculate L4 transport checksum
    if proto == 6 /* TCP */ && ip_packet.len() >= ihl + 20 {
        let tcp_offset = ihl;
        ip_packet[tcp_offset + 16] = 0;
        ip_packet[tcp_offset + 17] = 0;
        let tcp_csum = compute_tcp_checksum(&src_ip, &dst_ip, &ip_packet[tcp_offset..]);
        ip_packet[tcp_offset + 16..tcp_offset + 18].copy_from_slice(&tcp_csum.to_be_bytes());
        true
    } else if proto == 17 /* UDP */ && ip_packet.len() >= ihl + 8 {
        let udp_offset = ihl;
        ip_packet[udp_offset + 6] = 0;
        ip_packet[udp_offset + 7] = 0;
        let udp_csum = compute_udp_checksum(&src_ip, &dst_ip, &ip_packet[udp_offset..]);
        ip_packet[udp_offset + 6..udp_offset + 8].copy_from_slice(&udp_csum.to_be_bytes());
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_checksum() {
        let mut ip_hdr = [
            0x45, 0x00, 0x00, 0x3c,
            0x1c, 0x46, 0x40, 0x00,
            0x40, 0x06, 0x00, 0x00, // zero checksum
            0xc0, 0xa8, 0x01, 0x64, // 192.168.1.100
            0x33, 0x59, 0x2c, 0x0c, // 51.89.44.12
        ];
        let csum = compute_ip_checksum(&ip_hdr);
        assert_ne!(csum, 0);
        ip_hdr[10..12].copy_from_slice(&csum.to_be_bytes());
        // Verifying checksum over header with checksum inserted should yield 0
        assert_eq!(compute_ip_checksum(&ip_hdr), 0);
    }

    #[test]
    fn test_recalculate_checksums() {
        let mut packet = vec![0u8; 40];
        packet[0] = 0x45;
        packet[9] = 6; // TCP
        packet[12..16].copy_from_slice(&[10, 8, 0, 2]);
        packet[16..20].copy_from_slice(&[188, 42, 43, 98]);
        packet[20..22].copy_from_slice(&50000u16.to_be_bytes());
        packet[22..24].copy_from_slice(&6112u16.to_be_bytes());
        packet[32] = 0x50; // 20 bytes TCP

        assert!(recalculate_checksums(&mut packet));
        let ip_csum = u16::from_be_bytes([packet[10], packet[11]]);
        let tcp_csum = u16::from_be_bytes([packet[36], packet[37]]);
        assert_ne!(ip_csum, 0);
        assert_ne!(tcp_csum, 0);
    }
}
