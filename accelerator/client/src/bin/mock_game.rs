//! Mock MMO Game (Test Harness for FastPing Accelerator)
//!
//! Simulates an MMO game client and server (Aion 2 pattern):
//! - TCP Port 7777: Skill casts and combat actions (triggers FastConnect immediate ACK)
//! - UDP Port 8000: Real-time movement & world state updates at 30 ticks/sec
//!
//! Can run on Windows as "mock_game.exe" (client) and on Linux VPS as server,
//! allowing end-to-end testing of ProcessWatcher, WinDivert, FastConnect, and Multipath GameTunnel.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tracing::{info, warn, error};

#[derive(Parser, Debug)]
#[command(author, version, about = "Mock MMO Game for testing FastPing WAN Accelerator")]
struct Cli {
    /// Role: "client" (simulate game on Windows) or "server" (simulate game server on VPS/remote)
    #[arg(short, long, default_value = "client")]
    mode: String,

    /// Remote game server IP (when running in client mode)
    #[arg(long, default_value = "127.0.0.1")]
    server_ip: String,

    /// Listen IP (when running in server mode)
    #[arg(long, default_value = "0.0.0.0")]
    listen_ip: String,

    /// TCP combat / command port
    #[arg(long, default_value_t = 7777)]
    tcp_port: u16,

    /// UDP movement / tick port
    #[arg(long, default_value_t = 8000)]
    udp_port: u16,
}

#[inline]
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Cli::parse();

    match args.mode.to_lowercase().as_str() {
        "server" => run_server(&args).await?,
        "client" => run_client(&args).await?,
        other => {
            error!("Unknown mode: {}. Use 'client' or 'server'.", other);
        }
    }

    Ok(())
}

// ==============================================================================
// SERVER MODE (Can run on VPS or local test machine)
// ==============================================================================
async fn run_server(args: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    info!("============================================================");
    info!("Starting Mock MMO Game Server");
    info!("TCP Port (Combat/Skills) : {}:{}", args.listen_ip, args.tcp_port);
    info!("UDP Port (Movement/Tick) : {}:{}", args.listen_ip, args.udp_port);
    info!("============================================================");

    // 1. TCP Server for skills / actions
    let tcp_listener = TcpListener::bind(format!("{}:{}", args.listen_ip, args.tcp_port)).await?;
    tokio::spawn(async move {
        loop {
            match tcp_listener.accept().await {
                Ok((mut socket, peer)) => {
                    info!("[MockServer-TCP] Client connected from {}", peer);
                    tokio::spawn(async move {
                        let mut buf = [0u8; 1024];
                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => {
                                    info!("[MockServer-TCP] Client {} disconnected", peer);
                                    break;
                                }
                                Ok(n) => {
                                    let msg = String::from_utf8_lossy(&buf[..n]);
                                    info!("[MockServer-TCP] Received command from {}: {}", peer, msg.trim());
                                    // Send server action reply
                                    let reply = format!("SERVER_ACK:SKILL_RESOLVED:{}\n", current_time_ms());
                                    if socket.write_all(reply.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!("[MockServer-TCP] Read error: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => error!("[MockServer-TCP] Accept error: {}", e),
            }
        }
    });

    // 2. UDP Server for real-time movement ticks
    let udp_socket = Arc::new(UdpSocket::bind(format!("{}:{}", args.listen_ip, args.udp_port)).await?);
    let mut buf = [0u8; 2048];
    loop {
        match udp_socket.recv_from(&mut buf).await {
            Ok((len, peer)) => {
                // Echo packet back with server timestamp
                let mut reply = buf[..len].to_vec();
                let server_ts = current_time_ms().to_be_bytes();
                reply.extend_from_slice(&server_ts);
                let _ = udp_socket.send_to(&reply, peer).await;
            }
            Err(e) => error!("[MockServer-UDP] Recv error: {}", e),
        }
    }
}

// ==============================================================================
// CLIENT MODE (Simulates game process on Windows, e.g. MockGame.exe)
// ==============================================================================
async fn run_client(args: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let pid = std::process::id();
    info!("============================================================");
    info!("Starting Mock MMO Game Client (Simulating Aion 2)");
    info!("Process PID             : {}", pid);
    info!("Target Server           : {}:{}", args.server_ip, args.tcp_port);
    info!("Target UDP Port         : {}:{}", args.server_ip, args.udp_port);
    info!("Process Name            : mock_game.exe (or MockGame.exe)");
    info!("============================================================");

    let target_tcp = format!("{}:{}", args.server_ip, args.tcp_port);
    let target_udp: SocketAddr = format!("{}:{}", args.server_ip, args.udp_port).parse()?;

    // 1. Establish TCP connection (Skill/Action loop)
    info!("[MockClient-TCP] Connecting to game server at {}...", target_tcp);
    let mut tcp_stream = match TcpStream::connect(&target_tcp).await {
        Ok(s) => {
            info!("[MockClient-TCP] Connected! Starting skill cast loop...");
            Some(s)
        }
        Err(e) => {
            warn!("[MockClient-TCP] Could not connect to {}: {}", target_tcp, e);
            warn!("[MockClient-TCP] (Make sure mock_game server is running on the target machine)");
            None
        }
    };

    // 2. Spawn TCP Skill Cast loop (FastConnect test)
    if let Some(mut stream) = tcp_stream.take() {
        tokio::spawn(async move {
            let mut skill_counter = 0u32;
            let mut interval = tokio::time::interval(Duration::from_millis(1500));
            let mut rx_buf = [0u8; 1024];

            loop {
                interval.tick().await;
                skill_counter += 1;
                let payload = format!("CAST_SKILL_ID:{}:TS:{}\n", skill_counter, current_time_ms());

                let send_time = Instant::now();
                if let Err(e) = stream.write_all(payload.as_bytes()).await {
                    warn!("[MockClient-TCP] Write failed: {}", e);
                    break;
                }
                let write_duration_us = send_time.elapsed().as_micros();

                // With FastConnect active, socket write/ACK completes in microseconds locally (<1ms)
                info!(
                    "[MockClient-TCP] Skill #{} Cast! Local TCP buffer ack latency: {} µs ({:.3} ms)",
                    skill_counter,
                    write_duration_us,
                    write_duration_us as f64 / 1000.0
                );

                // Read server response
                match stream.read(&mut rx_buf).await {
                    Ok(n) if n > 0 => {
                        let resp = String::from_utf8_lossy(&rx_buf[..n]);
                        info!("[MockClient-TCP] Server confirmed skill: {}", resp.trim());
                    }
                    _ => break,
                }
            }
        });
    }

    // 3. UDP Movement / State Synchronization loop (Multipath test at 30 Hz)
    let udp_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let udp_send = Arc::clone(&udp_socket);

    // Receiver task for RTT measurement
    let udp_recv = Arc::clone(&udp_socket);
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            if let Ok((len, _)) = udp_recv.recv_from(&mut buf).await {
                if len >= 12 {
                    let seq = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    let sent_ts = u64::from_be_bytes([
                        buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
                    ]);
                    let now = current_time_ms();
                    let rtt = now.saturating_sub(sent_ts);
                    info!(
                        "[MockClient-UDP] Tick #{} RTT: {} ms (Received via GameTunnel)",
                        seq, rtt
                    );
                }
            }
        }
    });

    // Sender task (30 Hz = every 33 ms)
    let mut seq = 0u32;
    let mut ticker = tokio::time::interval(Duration::from_millis(33));
    info!("[MockClient-UDP] Starting 30 Hz movement packet stream...");

    loop {
        ticker.tick().await;
        seq = seq.wrapping_add(1);
        let mut packet = vec![0u8; 32];
        packet[0..4].copy_from_slice(&seq.to_be_bytes());
        packet[4..12].copy_from_slice(&current_time_ms().to_be_bytes());
        // Dummy coordinate payload (X, Y, Z float)
        packet[12..16].copy_from_slice(&100.5f32.to_be_bytes());
        packet[16..20].copy_from_slice(&250.0f32.to_be_bytes());
        packet[20..24].copy_from_slice(&15.2f32.to_be_bytes());

        let _ = udp_send.send_to(&packet, target_udp).await;
    }
}
