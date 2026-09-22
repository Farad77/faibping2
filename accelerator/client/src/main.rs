use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use tokio::sync::mpsc;
use tracing::{info, warn};

mod config;
mod fastconnect;
mod intercept;
mod registry;
mod transport;
mod watcher;

use config::GameProfile;
use fastconnect::FastConnectEngine;
use intercept::{InterceptedPacket, InterceptionEngine};
use registry::RegistryOptimizer;
use transport::MultipathTransport;
use watcher::ProcessWatcher;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "FastPing MMO Low-Latency WAN Accelerator Client (Target: Windows & Aion 2)"
)]
struct Args {
    /// Path to game profile JSON file
    #[arg(short, long, default_value = "profiles/aion2.json")]
    profile: PathBuf,

    /// Scan directory to auto-detect game executable (e.g. C:\Games\Aion2)
    #[arg(long)]
    scan_dir: Option<PathBuf>,

    /// VPS Gateway Host / IP
    #[arg(long, default_value = "127.0.0.1")]
    vps_host: String,

    /// VPS UDP Channel 1 Port
    #[arg(long, default_value_t = 51820)]
    vps_ch1: u16,

    /// VPS UDP Channel 2 Port
    #[arg(long, default_value_t = 4433)]
    vps_ch2: u16,

    /// Skip Windows TCP/IP registry tuning (useful for non-admin testing)
    #[arg(long)]
    no_registry: bool,
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
    info!("Starting FastPing WAN Accelerator Client");
    info!("============================================================");

    // 1. Resolve Game Profile (Declarative or Auto-detection)
    let profile = if let Some(ref scan_dir) = args.scan_dir {
        info!("Scanning directory {:?} for known game executables...", scan_dir);
        let profiles_dir = Path::new("profiles");
        let detected = scan_for_game(scan_dir, profiles_dir);
        match detected {
            Some(p) => {
                info!("Auto-detected matching profile: {} ({})", p.name, p.game_id);
                p
            }
            None => {
                warn!("No known game found in scan directory. Falling back to default profile: {:?}", args.profile);
                load_or_fallback_profile(&args.profile)?
            }
        }
    } else {
        load_or_fallback_profile(&args.profile)?
    };

    info!("Active Profile : {} [{}]", profile.name, profile.game_id);
    info!("Target Binaries: {:?}", profile.executables);
    info!("FastConnect    : {}", profile.optimizations.enable_fastconnect);
    info!("Multipath Dup  : {}", profile.optimizations.enable_multipath_dup);
    info!("DSCP Tag       : {}", profile.optimizations.dscp_tag);

    // 2. Windows Registry Low-Latency Tuning
    let mut registry_opt = RegistryOptimizer::new();
    if !args.no_registry {
        if let Err(e) = registry_opt.apply_optimizations() {
            warn!("[Registry] Failed to apply registry tweaks: {}", e);
        }
    } else {
        info!("[Registry] Bypassing registry optimizations (--no-registry).");
    }

    // 3. Resolve Remote VPS Endpoints
    let addr_ch1: SocketAddr = format!("{}:{}", args.vps_host, args.vps_ch1)
        .to_socket_addrs()?
        .next()
        .ok_or("Failed to resolve VPS Channel 1 address")?;
    let addr_ch2: SocketAddr = format!("{}:{}", args.vps_host, args.vps_ch2)
        .to_socket_addrs()?
        .next()
        .ok_or("Failed to resolve VPS Channel 2 address")?;

    // 4. Initialize Multipath Transport
    let transport = MultipathTransport::bind(
        addr_ch1,
        addr_ch2,
        profile.optimizations.dscp_tag,
    )
    .await?;
    transport.start().await;
    let transport_sender = transport.packet_sender();

    // 5. Initialize Process Watcher
    let watcher = ProcessWatcher::new(profile.clone());
    let process_state_rx = watcher.subscribe();
    watcher.start().await;

    // 6. Initialize FastConnect Engine
    let fastconnect = FastConnectEngine::new(profile.optimizations.enable_fastconnect);

    // 7. Initialize WinDivert Interception Engine
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<InterceptedPacket>(4096);
    let (_inbound_tx, inbound_rx) = mpsc::channel::<Vec<u8>>(4096);

    let interceptor = InterceptionEngine::new(
        profile.clone(),
        process_state_rx.clone(),
        outbound_tx,
        inbound_rx,
    );
    interceptor.start().await;

    // Forward intercepted game packets to Multipath Transport
    let fwd_transport = transport_sender.clone();
    tokio::spawn(async move {
        while let Some(packet) = outbound_rx.recv().await {
            // If TCP data and FastConnect enabled, synthesize local immediate ACK
            if packet.is_tcp && fastconnect.is_enabled() {
                if let Some(_ack) = fastconnect.synthesize_local_ack(&packet.raw) {
                    // Local ACK synthesized to unlock client animation
                }
            }

            // Relay payload packet over GameTunnel overlay
            let _ = fwd_transport.send(packet.raw).await;
        }
    });

    // 8. Background Telemetry Reporter
    let report_process = process_state_rx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let st = report_process.borrow().clone();
            info!(
                "[Status] Game Detected: {} | PIDs: {:?} | Intercepted Ports: {}",
                st.is_running,
                st.pids,
                st.tracked_ports.len()
            );
            transport.print_metrics().await;
        }
    });

    info!("============================================================");
    info!("FastPing WAN Accelerator running. Press Ctrl+C to terminate.");
    info!("============================================================");

    // 9. Graceful Shutdown Handler (Guarantees Registry Restoration)
    tokio::signal::ctrl_c().await?;
    info!("\n[*] Termination signal received. Restoring system state...");

    registry_opt.restore();
    info!("[+] Shutdown complete. Goodbye!");

    Ok(())
}

fn load_or_fallback_profile(path: &Path) -> Result<GameProfile, Box<dyn std::error::Error>> {
    if path.exists() {
        GameProfile::load_from_file(path)
    } else {
        // Look inside profiles/ subdir if relative
        let subpath = Path::new("accelerator/client/profiles").join(path.file_name().unwrap_or_default());
        if subpath.exists() {
            GameProfile::load_from_file(subpath)
        } else {
            // Built-in default fallback for Aion 2
            warn!("Profile {:?} not found, creating default Aion 2 profile.", path);
            Ok(GameProfile {
                game_id: "aion2".into(),
                name: "Aion 2 (MMORPG)".into(),
                executables: vec![
                    "Aion2.exe".into(),
                    "Aion2-Win64-Shipping.exe".into(),
                    "Aion2Launcher.exe".into(),
                ],
                routing: config::RoutingConfig {
                    protocol: "BOTH".into(),
                    ports: vec![
                        config::PortMatcher::Single(3724),
                        config::PortMatcher::Single(7777),
                        config::PortMatcher::Single(8000),
                        config::PortMatcher::Range(9000, 9100),
                    ],
                    target_subnets: vec!["0.0.0.0/0".into()],
                },
                optimizations: config::OptimizationsConfig {
                    enable_fastconnect: true,
                    enable_multipath_dup: true,
                    packet_duplication_rate: 2,
                    dscp_tag: 46,
                },
            })
        }
    }
}

fn scan_for_game(dir: &Path, profiles_dir: &Path) -> Option<GameProfile> {
    if !dir.exists() {
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(profile) = GameProfile::auto_detect_profile(profiles_dir, &path) {
                    return Some(profile);
                }
            }
        }
    }
    None
}
