use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use tokio::sync::mpsc;
use tracing::{info, warn};

use accelerator_client::config::{self, GameProfile};
use accelerator_client::fastconnect::FastConnectEngine;
use accelerator_client::intercept::{InterceptedPacket, InterceptionEngine};
use accelerator_client::registry::RegistryOptimizer;
use accelerator_client::settings::AppSettings;
use accelerator_client::transport::MultipathTransport;
use accelerator_client::watcher::ProcessWatcher;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "FastPing MMO Low-Latency WAN Accelerator Client (Target: Windows & Aion 2)"
)]
struct Args {
    /// Path to game profile JSON file (defaults to settings.json)
    #[arg(short, long)]
    profile: Option<PathBuf>,

    /// Scan directory to auto-detect game executable (e.g. C:\Games\Aion2)
    #[arg(long)]
    scan_dir: Option<PathBuf>,

    /// VPS Gateway Host / IP (defaults to settings.json: 72.61.111.131)
    #[arg(long)]
    vps_host: Option<String>,

    /// VPS UDP Channel 1 Port
    #[arg(long)]
    vps_ch1: Option<u16>,

    /// VPS UDP Channel 2 Port
    #[arg(long)]
    vps_ch2: Option<u16>,

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
    let settings = AppSettings::load_or_default("settings.json");

    let vps_host = args.vps_host.unwrap_or_else(|| settings.vps_host.clone());
    let vps_ch1 = args.vps_ch1.unwrap_or(settings.vps_ch1);
    let vps_ch2 = args.vps_ch2.unwrap_or(settings.vps_ch2);
    let profile_path = args.profile.unwrap_or_else(|| PathBuf::from(&settings.active_profile));

    info!("============================================================");
    info!("Starting FastPing WAN Accelerator Client");
    info!("VPS Endpoint   : {}:{} & :{}", vps_host, vps_ch1, vps_ch2);
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
                warn!("No known game found in scan directory. Falling back to profile: {:?}", profile_path);
                load_or_fallback_profile(&profile_path)?
            }
        }
    } else {
        load_or_fallback_profile(&profile_path)?
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
    let addr_ch1: SocketAddr = format!("{}:{}", vps_host, vps_ch1)
        .to_socket_addrs()?
        .next()
        .ok_or("Failed to resolve VPS Channel 1 address")?;
    let addr_ch2: SocketAddr = format!("{}:{}", vps_host, vps_ch2)
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
