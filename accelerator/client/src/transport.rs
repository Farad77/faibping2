//! Multipath UDP Transport Engine with Active Probing & EWMA Telemetry
//!
//! Replicates packets across dual independent UDP paths (e.g. VPS port 51820 & 4433).
//! Computes RTT and Jitter via periodic 500ms lightweight probes using EWMA:
//! - RTT_EWMA = (1 - alpha) * RTT_EWMA + alpha * RTT_sample  (alpha = 0.125)
//! - Jitter_EWMA = Jitter_EWMA + (|RTT_sample - RTT_EWMA| - Jitter_EWMA) / 16 (RFC 3550)

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tracing::info;

use accelerator_common::deduplicator::Deduplicator;
use accelerator_common::protocol::{GameTunnelHeader, HEADER_LEN};
use crate::nat::TunnelNat;

#[derive(Debug, Clone, Default)]
pub struct TransportMetrics {
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub packets_sent: u64,
    pub probes_sent: u64,
    pub probes_acked: u64,
}

impl TransportMetrics {
    pub fn new() -> Self {
        Self {
            rtt_ms: 0.0,
            jitter_ms: 0.0,
            packets_sent: 0,
            probes_sent: 0,
            probes_acked: 0,
        }
    }
}

pub struct MultipathTransport {
    sock_ch1: Arc<UdpSocket>,
    sock_ch2: Arc<UdpSocket>,
    remote_ch1: SocketAddr,
    remote_ch2: SocketAddr,
    sequence: Arc<AtomicU32>,
    metrics_ch1: Arc<Mutex<TransportMetrics>>,
    metrics_ch2: Arc<Mutex<TransportMetrics>>,
    packet_tx: mpsc::Sender<Vec<u8>>,
    packet_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    inbound_tx: mpsc::Sender<Vec<u8>>,
    dedup: Arc<Mutex<Deduplicator>>,
    nat: Arc<TunnelNat>,
}

#[inline]
fn current_time_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32
}

impl MultipathTransport {
    pub async fn bind(
        remote_ch1: SocketAddr,
        remote_ch2: SocketAddr,
        dscp_tag: u8,
        inbound_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let sock1 = UdpSocket::bind("0.0.0.0:0").await?;
        let sock2 = UdpSocket::bind("0.0.0.0:0").await?;

        // Apply DSCP QoS tag if supported by socket
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            let raw1 = sock1.as_raw_socket();
            let raw2 = sock2.as_raw_socket();
            let tos = (dscp_tag << 2) as i32;
            unsafe {
                windows_sys::Win32::Networking::WinSock::setsockopt(
                    raw1 as usize,
                    windows_sys::Win32::Networking::WinSock::IPPROTO_IP as i32,
                    windows_sys::Win32::Networking::WinSock::IP_TOS as i32,
                    &tos as *const _ as *const u8,
                    std::mem::size_of::<i32>() as i32,
                );
                windows_sys::Win32::Networking::WinSock::setsockopt(
                    raw2 as usize,
                    windows_sys::Win32::Networking::WinSock::IPPROTO_IP as i32,
                    windows_sys::Win32::Networking::WinSock::IP_TOS as i32,
                    &tos as *const _ as *const u8,
                    std::mem::size_of::<i32>() as i32,
                );
            }
        }

        let (packet_tx, packet_rx) = mpsc::channel::<Vec<u8>>(4096);

        info!(
            "[MultipathTransport] Bound sockets. Path 1 -> {}, Path 2 -> {} (DSCP {})",
            remote_ch1, remote_ch2, dscp_tag
        );

        Ok(Self {
            sock_ch1: Arc::new(sock1),
            sock_ch2: Arc::new(sock2),
            remote_ch1,
            remote_ch2,
            sequence: Arc::new(AtomicU32::new(1)),
            metrics_ch1: Arc::new(Mutex::new(TransportMetrics::default())),
            metrics_ch2: Arc::new(Mutex::new(TransportMetrics::default())),
            packet_tx,
            packet_rx: Arc::new(Mutex::new(packet_rx)),
            inbound_tx,
            dedup: Arc::new(Mutex::new(Deduplicator::new())),
            nat: Arc::new(TunnelNat::new([10, 8, 0, 2])),
        })
    }

    pub fn packet_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.packet_tx.clone()
    }

    pub async fn get_metrics(&self) -> (f64, f64, u64, u64, u64, f64, f64, u64, u64, u64) {
        let m1 = self.metrics_ch1.lock().await;
        let m2 = self.metrics_ch2.lock().await;
        (
            m1.rtt_ms, m1.jitter_ms, m1.packets_sent, m1.probes_sent, m1.probes_acked,
            m2.rtt_ms, m2.jitter_ms, m2.packets_sent, m2.probes_sent, m2.probes_acked,
        )
    }

    pub async fn start(&self) {
        let sock1 = Arc::clone(&self.sock_ch1);
        let sock2 = Arc::clone(&self.sock_ch2);
        let dest1 = self.remote_ch1;
        let dest2 = self.remote_ch2;
        let seq = Arc::clone(&self.sequence);
        let metrics1 = Arc::clone(&self.metrics_ch1);
        let metrics2 = Arc::clone(&self.metrics_ch2);
        let rx = Arc::clone(&self.packet_rx);

        let nat_out = Arc::clone(&self.nat);
        let nat1 = Arc::clone(&self.nat);
        let nat2 = Arc::clone(&self.nat);

        // Task: Replicate outbound packets across both paths
        tokio::spawn(async move {
            let mut buf_ch1 = vec![0u8; 65535];
            let mut buf_ch2 = vec![0u8; 65535];
            let mut rx_lock = rx.lock().await;

            while let Some(mut raw_packet) = rx_lock.recv().await {
                // Apply Tunnel NAT: translate local LAN IP to 10.8.0.2 with checksum recalculation
                nat_out.translate_outbound(&mut raw_packet);

                let s = seq.fetch_add(1, Ordering::Relaxed);
                let ts = current_time_ms();

                // Build Primary Packet
                let h1 = GameTunnelHeader::new(s, ts, false, false);
                let _ = h1.encode(&mut buf_ch1);
                let len = HEADER_LEN + raw_packet.len();
                if len <= buf_ch1.len() {
                    buf_ch1[HEADER_LEN..len].copy_from_slice(&raw_packet);
                }

                // Build Duplicated Packet (IS_DUP = 1)
                let h2 = GameTunnelHeader::new(s, ts, true, false);
                let _ = h2.encode(&mut buf_ch2);
                if len <= buf_ch2.len() {
                    buf_ch2[HEADER_LEN..len].copy_from_slice(&raw_packet);
                }

                // Send concurrently on both paths
                let _ = sock1.send_to(&buf_ch1[..len], dest1).await;
                let _ = sock2.send_to(&buf_ch2[..len], dest2).await;

                {
                    let mut m1 = metrics1.lock().await;
                    m1.packets_sent += 1;
                    let mut m2 = metrics2.lock().await;
                    m2.packets_sent += 1;
                }
            }
        });

        // Task: Periodic Probing (500ms) for RTT & Jitter
        let p_sock1 = Arc::clone(&self.sock_ch1);
        let p_sock2 = Arc::clone(&self.sock_ch2);
        let p_dest1 = self.remote_ch1;
        let p_dest2 = self.remote_ch2;
        let p_metrics1 = Arc::clone(&self.metrics_ch1);
        let p_metrics2 = Arc::clone(&self.metrics_ch2);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(500));
            let mut probe_seq = 0u32;

            loop {
                ticker.tick().await;
                probe_seq = probe_seq.wrapping_add(1);
                let ts = current_time_ms();

                let probe_h = GameTunnelHeader::new(probe_seq, ts, false, true);
                let mut p_buf = [0u8; HEADER_LEN];
                if probe_h.encode(&mut p_buf).is_ok() {
                    let _ = p_sock1.send_to(&p_buf, p_dest1).await;
                    let _ = p_sock2.send_to(&p_buf, p_dest2).await;

                    p_metrics1.lock().await.probes_sent += 1;
                    p_metrics2.lock().await.probes_sent += 1;
                }
            }
        });

        // Task: Listen on Path 1 (Probe ACKs + Downlink Game Packets)
        let a_sock1 = Arc::clone(&self.sock_ch1);
        let a_metrics1 = Arc::clone(&self.metrics_ch1);
        let dedup1 = Arc::clone(&self.dedup);
        let in_tx1 = self.inbound_tx.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            while let Ok((len, _)) = a_sock1.recv_from(&mut buf).await {
                if len >= HEADER_LEN {
                    if let Ok(h) = GameTunnelHeader::decode(&buf[..len]) {
                        if h.is_ack() {
                            let now = current_time_ms();
                            let sample_rtt = now.saturating_sub(h.timestamp) as f64;
                            update_telemetry(&a_metrics1, sample_rtt).await;
                        } else if len > HEADER_LEN {
                            let mut d = dedup1.lock().await;
                            let accepted = d.process_packet(h.sequence);
                            drop(d);
                            if accepted {
                                let mut payload = buf[HEADER_LEN..len].to_vec();
                                nat1.translate_inbound(&mut payload);
                                let _ = in_tx1.send(payload).await;
                            }
                        }
                    }
                }
            }
        });

        // Task: Listen on Path 2 (Probe ACKs + Downlink Game Packets)
        let a_sock2 = Arc::clone(&self.sock_ch2);
        let a_metrics2 = Arc::clone(&self.metrics_ch2);
        let dedup2 = Arc::clone(&self.dedup);
        let in_tx2 = self.inbound_tx.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            while let Ok((len, _)) = a_sock2.recv_from(&mut buf).await {
                if len >= HEADER_LEN {
                    if let Ok(h) = GameTunnelHeader::decode(&buf[..len]) {
                        if h.is_ack() {
                            let now = current_time_ms();
                            let sample_rtt = now.saturating_sub(h.timestamp) as f64;
                            update_telemetry(&a_metrics2, sample_rtt).await;
                        } else if len > HEADER_LEN {
                            let mut d = dedup2.lock().await;
                            let accepted = d.process_packet(h.sequence);
                            drop(d);
                            if accepted {
                                let mut payload = buf[HEADER_LEN..len].to_vec();
                                nat2.translate_inbound(&mut payload);
                                let _ = in_tx2.send(payload).await;
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn print_metrics(&self) {
        let m1 = self.metrics_ch1.lock().await;
        let m2 = self.metrics_ch2.lock().await;
        info!(
            "[Metrics] Path 1 -> RTT: {:.1} ms | Jitter: {:.2} ms | Tx: {} | Probes: {}/{}",
            m1.rtt_ms, m1.jitter_ms, m1.packets_sent, m1.probes_acked, m1.probes_sent
        );
        info!(
            "[Metrics] Path 2 -> RTT: {:.1} ms | Jitter: {:.2} ms | Tx: {} | Probes: {}/{}",
            m2.rtt_ms, m2.jitter_ms, m2.packets_sent, m2.probes_acked, m2.probes_sent
        );
    }
}

async fn update_telemetry(metrics_lock: &Arc<Mutex<TransportMetrics>>, sample_rtt: f64) {
    let mut m = metrics_lock.lock().await;
    m.probes_acked += 1;

    if m.rtt_ms == 0.0 {
        m.rtt_ms = sample_rtt;
        m.jitter_ms = 0.0;
    } else {
        // RFC 6298 EWMA for RTT (alpha = 0.125 = 1/8)
        let alpha = 0.125;
        let diff = (sample_rtt - m.rtt_ms).abs();
        m.rtt_ms = (1.0 - alpha) * m.rtt_ms + alpha * sample_rtt;

        // RFC 3550 Jitter EWMA (beta = 1/16)
        m.jitter_ms += (diff - m.jitter_ms) / 16.0;
    }
}
