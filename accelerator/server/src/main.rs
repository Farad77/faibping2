use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::{error, info};

mod deduplicator;
mod tun;

use accelerator_common::deduplicator::Deduplicator;
use accelerator_common::protocol::{GameTunnelHeader, FLAG_IS_DUP, HEADER_LEN};

#[derive(Parser, Debug)]
#[command(author, version, about = "FastPing GameTunnel VPS Gateway Daemon")]
struct Args {
    /// UDP Channel 1 Listen Address
    #[arg(long, default_value = "0.0.0.0:51820")]
    listen_ch1: SocketAddr,

    /// UDP Channel 2 Listen Address
    #[arg(long, default_value = "0.0.0.0:4433")]
    listen_ch2: SocketAddr,

    /// Linux TUN device name
    #[arg(long, default_value = "tun-game")]
    tun_name: String,

    /// TUN device IP and CIDR
    #[arg(long, default_value = "10.8.0.1/24")]
    tun_ip: String,
}

#[derive(Default)]
struct ClientEndpoints {
    ch1: Option<SocketAddr>,
    ch2: Option<SocketAddr>,
}

#[inline]
fn current_time_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    info!("============================================================");
    info!("Starting FastPing GameTunnel VPS Gateway");
    info!("Channel 1 listening on : {}", args.listen_ch1);
    info!("Channel 2 listening on : {}", args.listen_ch2);
    info!("TUN Device             : {} ({})", args.tun_name, args.tun_ip);
    info!("============================================================");

    let sock_ch1 = Arc::new(UdpSocket::bind(args.listen_ch1).await?);
    let sock_ch2 = Arc::new(UdpSocket::bind(args.listen_ch2).await?);

    let tun = tun::create_tun(&args.tun_name, &args.tun_ip)?;
    info!("TUN interface initialized successfully.");

    let dedup = Arc::new(Mutex::new(Deduplicator::new()));
    let client_endpoints = Arc::new(Mutex::new(ClientEndpoints::default()));
    let downlink_seq = Arc::new(AtomicU32::new(1));

    let tun_tx = tun.tx_handle();

    // -------------------------------------------------------------
    // Task: Process Uplink Traffic on UDP Channel 1
    // -------------------------------------------------------------
    let sock1_recv = Arc::clone(&sock_ch1);
    let dedup1 = Arc::clone(&dedup);
    let clients1 = Arc::clone(&client_endpoints);
    let tun_tx1 = tun_tx.clone();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match sock1_recv.recv_from(&mut buf).await {
                Ok((len, peer_addr)) => {
                    handle_uplink_packet(
                        &sock1_recv,
                        peer_addr,
                        &buf[..len],
                        1,
                        &dedup1,
                        &clients1,
                        &tun_tx1,
                    )
                    .await;
                }
                Err(e) => {
                    error!("Error receiving from channel 1: {}", e);
                }
            }
        }
    });

    // -------------------------------------------------------------
    // Task: Process Uplink Traffic on UDP Channel 2
    // -------------------------------------------------------------
    let sock2_recv = Arc::clone(&sock_ch2);
    let dedup2 = Arc::clone(&dedup);
    let clients2 = Arc::clone(&client_endpoints);
    let tun_tx2 = tun_tx.clone();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match sock2_recv.recv_from(&mut buf).await {
                Ok((len, peer_addr)) => {
                    handle_uplink_packet(
                        &sock2_recv,
                        peer_addr,
                        &buf[..len],
                        2,
                        &dedup2,
                        &clients2,
                        &tun_tx2,
                    )
                    .await;
                }
                Err(e) => {
                    error!("Error receiving from channel 2: {}", e);
                }
            }
        }
    });

    // -------------------------------------------------------------
    // Task: Process Downlink Traffic (TUN -> Replicate to Client UDP)
    // -------------------------------------------------------------
    let sock1_down = Arc::clone(&sock_ch1);
    let sock2_down = Arc::clone(&sock_ch2);
    let clients_down = Arc::clone(&client_endpoints);
    let seq_down = Arc::clone(&downlink_seq);
    let mut tun_rx = tun;

    tokio::spawn(async move {
        let mut out_buf = vec![0u8; 65535];
        while let Some(ip_packet) = tun_rx.recv().await {
            let seq = seq_down.fetch_add(1, Ordering::Relaxed);
            let ts = current_time_ms();

            let header = GameTunnelHeader::new(seq, ts, false, false);
            if header.encode(&mut out_buf).is_err() {
                continue;
            }

            let total_len = HEADER_LEN + ip_packet.len();
            if total_len > out_buf.len() {
                continue;
            }
            out_buf[HEADER_LEN..total_len].copy_from_slice(&ip_packet);

            let endpoints = clients_down.lock().await;

            // Replicate on Channel 1
            if let Some(peer1) = endpoints.ch1 {
                let _ = sock1_down.send_to(&out_buf[..total_len], peer1).await;
            }

            // Replicate on Channel 2 (mark IS_DUP)
            if let Some(peer2) = endpoints.ch2 {
                out_buf[2] |= FLAG_IS_DUP;
                let _ = sock2_down.send_to(&out_buf[..total_len], peer2).await;
            }
        }
    });

    // -------------------------------------------------------------
    // Periodic Stats Reporter
    // -------------------------------------------------------------
    let dedup_stats = Arc::clone(&dedup);
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
    loop {
        ticker.tick().await;
        let d = dedup_stats.lock().await;
        info!(
            "[Stats] Rx: {} | In-Order: {} | Out-Of-Order: {} | Dup Dropped: {} | Stale Dropped: {}",
            d.total_received,
            d.accepted_in_order,
            d.accepted_out_of_order,
            d.duplicates_dropped,
            d.stale_dropped
        );
    }
}


async fn handle_uplink_packet(
    socket: &UdpSocket,
    peer_addr: SocketAddr,
    data: &[u8],
    channel_id: u8,
    dedup: &Arc<Mutex<Deduplicator>>,
    clients: &Arc<Mutex<ClientEndpoints>>,
    tun_tx: &tokio::sync::mpsc::Sender<Vec<u8>>,
) {
    if data.len() < HEADER_LEN {
        return;
    }

    let header = match GameTunnelHeader::decode(data) {
        Ok(h) => h,
        Err(_) => return, // Invalid packet or magic
    };

    // Update peer address for downlink
    {
        let mut c = clients.lock().await;
        if channel_id == 1 {
            if c.ch1 != Some(peer_addr) {
                tracing::info!("Channel 1 client endpoint updated: {:?} -> {}", c.ch1, peer_addr);
                c.ch1 = Some(peer_addr);
                let mut d = dedup.lock().await;
                d.reset();
            }
        } else {
            if c.ch2 != Some(peer_addr) {
                tracing::info!("Channel 2 client endpoint updated: {:?} -> {}", c.ch2, peer_addr);
                c.ch2 = Some(peer_addr);
            }
        }
    }

    // Handle probe / keepalive ping
    if header.is_probe() {
        let mut ack_buf = [0u8; HEADER_LEN];
        let mut ack_header = header;
        ack_header.set_ack();
        if ack_header.encode(&mut ack_buf).is_ok() {
            let _ = socket.send_to(&ack_buf, peer_addr).await;
        }
        return;
    }

    // Process data payload through deduplicator
    let mut d = dedup.lock().await;
    let accepted = d.process_packet(header.sequence);
    drop(d);

    if accepted {
        let payload = &data[HEADER_LEN..];
        if !payload.is_empty() {
            let _ = tun_tx.send(payload.to_vec()).await;
        }
    }
}
