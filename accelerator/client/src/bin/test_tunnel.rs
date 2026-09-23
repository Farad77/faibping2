use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use accelerator_common::protocol::{GameTunnelHeader, HEADER_LEN};
use accelerator_common::checksum::recalculate_checksums;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vps_addr: SocketAddr = "72.61.111.131:51820".parse()?;
    println!("Testing connectivity to VPS Gateway at {}...", vps_addr);

    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(vps_addr).await?;

    let now_ms = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() & 0x3FFFFFFF) as u32;
    let seq_probe = now_ms;
    let seq_syn = now_ms + 1;
    let local_tcp_port = 40000 + (now_ms % 20000) as u16;

    // 1. Test Probe Ping
    let mut probe_buf = [0u8; HEADER_LEN];
    let probe_hdr = GameTunnelHeader::new(seq_probe, 1000, false, true); // is_probe = true
    probe_hdr.encode(&mut probe_buf)?;

    let t0 = Instant::now();
    sock.send(&probe_buf).await?;
    println!("Sent probe ping to VPS, awaiting ACK...");

    let mut recv_buf = vec![0u8; 1500];
    let rtt = match tokio::time::timeout(Duration::from_millis(3000), sock.recv(&mut recv_buf)).await {
        Ok(Ok(n)) => {
            let elapsed = t0.elapsed();
            if n >= HEADER_LEN {
                let resp_hdr = GameTunnelHeader::decode(&recv_buf[..n])?;
                println!("SUCCESS: Received probe ACK in {:.2?} (is_ack: {}, seq: {})", elapsed, resp_hdr.is_ack(), resp_hdr.sequence);
                Some(elapsed)
            } else {
                println!("Received short packet: {} bytes", n);
                None
            }
        }
        Ok(Err(e)) => {
            println!("Error receiving: {}", e);
            None
        }
        Err(_) => {
            println!("TIMEOUT: No response to probe from VPS within 3 seconds.");
            None
        }
    };

    if rtt.is_none() {
        println!("\n===> DIAGNOSIS: The accelerator-server daemon is NOT reachable on 72.61.111.131:51820!");
        return Ok(());
    }

    // 2. Test TUN Internet Forwarding (TCP SYN to 1.1.1.1:80)
    println!("\nTesting Internet Forwarding through VPS TUN (TCP SYN to 1.1.1.1:80 with seq {}, port {})...", seq_syn, local_tcp_port);
    let mut raw_syn = vec![0u8; 40];
    raw_syn[0] = 0x45; // IPv4, IHL 5
    raw_syn[8] = 64;   // TTL
    raw_syn[9] = 6;    // TCP
    raw_syn[12..16].copy_from_slice(&[10, 8, 0, 2]); // Virtual tunnel IP
    raw_syn[16..20].copy_from_slice(&[1, 1, 1, 1]);  // 1.1.1.1
    raw_syn[20..22].copy_from_slice(&local_tcp_port.to_be_bytes()); // Local src port
    raw_syn[22..24].copy_from_slice(&80u16.to_be_bytes());    // Dst port 80
    raw_syn[24..28].copy_from_slice(&100000u32.to_be_bytes()); // Seq number
    raw_syn[32] = 0x50; // Data offset 5
    raw_syn[33] = 0x02; // SYN flag
    raw_syn[34..36].copy_from_slice(&65535u16.to_be_bytes()); // Window
    recalculate_checksums(&mut raw_syn);

    let mut tunnel_buf = vec![0u8; HEADER_LEN + raw_syn.len()];
    let data_hdr = GameTunnelHeader::new(seq_syn, 2000, false, false);
    data_hdr.encode(&mut tunnel_buf)?;
    tunnel_buf[HEADER_LEN..].copy_from_slice(&raw_syn);

    let t1 = Instant::now();
    sock.send(&tunnel_buf).await?;
    println!("Sent encapsulated TCP SYN to 1.1.1.1:80 via tunnel...");

    let mut synack_received = false;
    let deadline = Instant::now() + Duration::from_millis(3000);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, sock.recv(&mut recv_buf)).await {
            Ok(Ok(n)) => {
                println!("Received UDP packet: {} bytes from VPS", n);
                if n > HEADER_LEN {
                    let resp_hdr = GameTunnelHeader::decode(&recv_buf[..n])?;
                    println!("Tunnel Header: seq={}, is_ack={}, is_dup={}", resp_hdr.sequence, resp_hdr.is_ack(), resp_hdr.is_dup());
                    let ip_packet = &recv_buf[HEADER_LEN..n];
                    if ip_packet.len() >= 20 {
                        let proto = ip_packet[9];
                        let src_ip = &ip_packet[12..16];
                        let dst_ip = &ip_packet[16..20];
                        println!("IP Packet: proto={}, src={}.{}.{}.{}, dst={}.{}.{}.{}",
                            proto, src_ip[0], src_ip[1], src_ip[2], src_ip[3],
                            dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3]);
                        if proto == 6 && ip_packet.len() >= 40 {
                            let flags = ip_packet[33];
                            let is_syn_ack = (flags & 0x12) == 0x12;
                            println!(
                                "SUCCESS: Received return TCP packet in {:.2?}! Flags: 0x{:02X} (SYN-ACK: {})",
                                t1.elapsed(), flags, is_syn_ack
                            );
                            synack_received = true;
                            break;
                        }
                    }
                }
            }
            _ => break,
        }
    }

    if !synack_received {
        // 3. Test UDP DNS query to 8.8.8.8:53 through tunnel
        println!("\nTesting UDP Forwarding through VPS TUN (DNS query to 8.8.8.8:53)...");
        let seq_dns = seq_syn + 1;
        // Simple DNS query for google.com (A record)
        let dns_payload: [u8; 28] = [
            0x12, 0x34, // ID
            0x01, 0x00, // Standard query, recursion desired
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
            0x06, b'g', b'o', b'o', b'g', b'l', b'e',
            0x03, b'c', b'o', b'm',
            0x00,       // Root null
            0x00, 0x01, // Type A
            0x00, 0x01, // Class IN
        ];

        let mut raw_dns = vec![0u8; 20 + 8 + dns_payload.len()];
        raw_dns[0] = 0x45;
        let total_len = raw_dns.len() as u16;
        raw_dns[2..4].copy_from_slice(&total_len.to_be_bytes());
        raw_dns[8] = 64;
        raw_dns[9] = 17; // UDP
        raw_dns[12..16].copy_from_slice(&[10, 8, 0, 2]); // Virtual tunnel IP
        raw_dns[16..20].copy_from_slice(&[8, 8, 8, 8]);  // 8.8.8.8
        let dns_src_port = 53000 + (now_ms % 10000) as u16;
        raw_dns[20..22].copy_from_slice(&dns_src_port.to_be_bytes());
        raw_dns[22..24].copy_from_slice(&53u16.to_be_bytes());
        let udp_len = (8 + dns_payload.len()) as u16;
        raw_dns[24..26].copy_from_slice(&udp_len.to_be_bytes());
        raw_dns[28..].copy_from_slice(&dns_payload);
        recalculate_checksums(&mut raw_dns);

        let mut dns_tunnel_buf = vec![0u8; HEADER_LEN + raw_dns.len()];
        let dns_hdr = GameTunnelHeader::new(seq_dns, 3000, false, false);
        dns_hdr.encode(&mut dns_tunnel_buf)?;
        dns_tunnel_buf[HEADER_LEN..].copy_from_slice(&raw_dns);

        let t2 = Instant::now();
        sock.send(&dns_tunnel_buf).await?;
        println!("Sent encapsulated DNS query to 8.8.8.8:53 via tunnel...");

        let deadline2 = Instant::now() + Duration::from_millis(3000);
        while Instant::now() < deadline2 {
            let rem = deadline2.saturating_duration_since(Instant::now());
            match tokio::time::timeout(rem, sock.recv(&mut recv_buf)).await {
                Ok(Ok(n)) => {
                    println!("Received UDP packet: {} bytes from VPS", n);
                    if n > HEADER_LEN {
                        let resp_hdr = GameTunnelHeader::decode(&recv_buf[..n])?;
                        println!("Tunnel Header: seq={}, is_ack={}, is_dup={}", resp_hdr.sequence, resp_hdr.is_ack(), resp_hdr.is_dup());
                        let ip_packet = &recv_buf[HEADER_LEN..n];
                        if ip_packet.len() >= 28 && ip_packet[9] == 17 {
                            println!("SUCCESS: Received return DNS response from tunnel in {:.2?}!", t2.elapsed());
                            synack_received = true;
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    if synack_received {
        println!("\n===> ALL SYSTEMS OPERATIONAL: VPS gateway is actively routing and returning packets!");
    } else {
        println!("\n===> DIAGNOSIS: Probe responded, BUT TCP SYN received NO reply from 1.1.1.1!");
        println!("This means Linux kernel packet forwarding or iptables MASQUERADE is missing on the VPS.");
    }

    Ok(())
}
