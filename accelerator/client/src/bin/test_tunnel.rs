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

    // 1. Test Probe Ping
    let mut probe_buf = [0u8; HEADER_LEN];
    let probe_hdr = GameTunnelHeader::new(1, 1000, false, true); // is_probe = true
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
    println!("\nTesting Internet Forwarding through VPS TUN (TCP SYN to 1.1.1.1:80)...");
    let mut raw_syn = vec![0u8; 40];
    raw_syn[0] = 0x45; // IPv4, IHL 5
    raw_syn[8] = 64;   // TTL
    raw_syn[9] = 6;    // TCP
    raw_syn[12..16].copy_from_slice(&[10, 8, 0, 2]); // Virtual tunnel IP
    raw_syn[16..20].copy_from_slice(&[1, 1, 1, 1]);  // 1.1.1.1
    raw_syn[20..22].copy_from_slice(&45678u16.to_be_bytes()); // Local src port
    raw_syn[22..24].copy_from_slice(&80u16.to_be_bytes());    // Dst port 80
    raw_syn[24..28].copy_from_slice(&100000u32.to_be_bytes()); // Seq number
    raw_syn[32] = 0x50; // Data offset 5
    raw_syn[33] = 0x02; // SYN flag
    raw_syn[34..36].copy_from_slice(&65535u16.to_be_bytes()); // Window
    recalculate_checksums(&mut raw_syn);

    let mut tunnel_buf = vec![0u8; HEADER_LEN + raw_syn.len()];
    let data_hdr = GameTunnelHeader::new(2, 2000, false, false);
    data_hdr.encode(&mut tunnel_buf)?;
    tunnel_buf[HEADER_LEN..].copy_from_slice(&raw_syn);

    let t1 = Instant::now();
    sock.send(&tunnel_buf).await?;
    println!("Sent encapsulated TCP SYN to 1.1.1.1:80 via tunnel...");

    let mut synack_received = false;
    let deadline = Instant::now() + Duration::from_millis(4000);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, sock.recv(&mut recv_buf)).await {
            Ok(Ok(n)) => {
                if n > HEADER_LEN {
                    let _resp_hdr = GameTunnelHeader::decode(&recv_buf[..n])?;
                    let ip_packet = &recv_buf[HEADER_LEN..n];
                    if ip_packet.len() >= 40 && ip_packet[9] == 6 {
                        let flags = ip_packet[33];
                        let is_syn_ack = (flags & 0x12) == 0x12; // SYN + ACK
                        println!(
                            "SUCCESS: Received return TCP packet from tunnel in {:.2?}! Flags: 0x{:02X} (SYN-ACK: {})",
                            t1.elapsed(),
                            flags,
                            is_syn_ack
                        );
                        synack_received = true;
                        break;
                    }
                }
            }
            _ => break,
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
