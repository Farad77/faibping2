//! FastPing Desktop GUI & Control Center
//!
//! Provides a modern zero-terminal desktop user interface.
//! Serves an embedded high-performance dark gaming dashboard on 127.0.0.1:4040
//! and launches a dedicated native application window.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{error, info};

mod web_ui;

use accelerator_client::config::GameProfile;
use accelerator_client::intercept::{InterceptedPacket, InterceptionEngine};
use accelerator_client::registry::RegistryOptimizer;
use accelerator_client::settings::AppSettings;
use accelerator_client::transport::MultipathTransport;
use accelerator_client::watcher::ProcessWatcher;

#[derive(Default, Clone)]
struct GuiState {
    running: bool,
    is_admin: bool,
    vps_host: String,
    vps_ch1: u16,
    vps_ch2: u16,
    active_profile_path: String,
    game_name: String,
    game_detected: bool,
    pids: Vec<u32>,
    tracked_ports_count: usize,
    path1_rtt: f64,
    path1_jitter: f64,
    path1_tx: u64,
    path1_probes: u64,
    path1_acked: u64,
    path2_rtt: f64,
    path2_jitter: f64,
    path2_tx: u64,
    path2_probes: u64,
    path2_acked: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    info!("============================================================");
    info!("Starting FastPing Desktop GUI & Control Center");
    info!("============================================================");

    let settings = Arc::new(Mutex::new(AppSettings::load_or_default("settings.json")));
    let initial_settings = settings.lock().await.clone();
    let initial_game_name = GameProfile::load_from_file(&initial_settings.active_profile)
        .map(|p| p.name)
        .unwrap_or_else(|_| "Path of Exile".to_string());

    let is_admin = RegistryOptimizer::is_admin();
    let state = Arc::new(Mutex::new(GuiState {
        running: false,
        is_admin,
        vps_host: initial_settings.vps_host.clone(),
        vps_ch1: initial_settings.vps_ch1,
        vps_ch2: initial_settings.vps_ch2,
        active_profile_path: initial_settings.active_profile.clone(),
        game_name: initial_game_name,
        ..Default::default()
    }));

    let stop_signal = Arc::new(AtomicBool::new(false));

    // Bind local web server on 127.0.0.1:4040
    let listener = TcpListener::bind("127.0.0.1:4040").await?;
    info!("[GUI Server] Dashboard running at http://127.0.0.1:4040");

    // Launch standalone application window in Edge App mode (or fallback browser)
    #[cfg(windows)]
    {
        info!("[GUI Window] Opening dedicated FastPing desktop app window...");
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "msedge", "--app=http://127.0.0.1:4040"])
            .spawn();
    }

    let state_http = Arc::clone(&state);
    let settings_http = Arc::clone(&settings);
    let stop_http = Arc::clone(&stop_signal);

    // HTTP Request Dispatcher Loop
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    let st = Arc::clone(&state_http);
                    let sett = Arc::clone(&settings_http);
                    let stop = Arc::clone(&stop_http);

                    tokio::spawn(async move {
                        let mut buf = [0u8; 4096];
                        if let Ok(n) = socket.read(&mut buf).await {
                            let req = String::from_utf8_lossy(&buf[..n]);
                            handle_http_request(req.as_ref(), &mut socket, st, sett, stop).await;
                        }
                    });
                }
                Err(e) => error!("TCP accept error: {}", e),
            }
        }
    });

    // Keep process alive until Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("[GUI] Shutdown signal received. Exiting.");
    Ok(())
}

async fn handle_http_request(
    req: &str,
    socket: &mut tokio::net::TcpStream,
    state: Arc<Mutex<GuiState>>,
    settings: Arc<Mutex<AppSettings>>,
    stop_signal: Arc<AtomicBool>,
) {
    let first_line = req.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    if method == "GET" && path == "/" {
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            web_ui::DASHBOARD_HTML.len(),
            web_ui::DASHBOARD_HTML
        );
        let _ = socket.write_all(resp.as_bytes()).await;
    } else if method == "GET" && path == "/api/status" {
        let s = state.lock().await;
        let json = serde_json::json!({
            "running": s.running,
            "is_admin": s.is_admin,
            "vps_host": s.vps_host,
            "active_profile": s.active_profile_path,
            "game_name": s.game_name,
            "game_detected": s.game_detected,
            "pids": s.pids,
            "tracked_ports_count": s.tracked_ports_count,
            "path1_rtt": s.path1_rtt,
            "path1_jitter": s.path1_jitter,
            "path1_tx": s.path1_tx,
            "path1_probes": s.path1_probes,
            "path1_acked": s.path1_acked,
            "path2_rtt": s.path2_rtt,
            "path2_jitter": s.path2_jitter,
            "path2_tx": s.path2_tx,
            "path2_probes": s.path2_probes,
            "path2_acked": s.path2_acked,
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json.len(),
            json
        );
        let _ = socket.write_all(resp.as_bytes()).await;
    } else if method == "GET" && path == "/api/profiles" {
        let mut profiles_list = Vec::new();
        if let Ok(entries) = std::fs::read_dir("profiles") {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(prof) = GameProfile::load_from_file(&p) {
                        profiles_list.push(serde_json::json!({
                            "path": p.to_string_lossy().replace('\\', "/"),
                            "id": prof.game_id,
                            "name": prof.name,
                        }));
                    }
                }
            }
        }
        let json = serde_json::to_string(&profiles_list).unwrap_or_else(|_| "[]".into());
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json.len(),
            json
        );
        let _ = socket.write_all(resp.as_bytes()).await;
    } else if method == "POST" && path == "/api/toggle" {
        let mut s = state.lock().await;
        s.running = !s.running;
        let now_running = s.running;
        drop(s);

        if now_running {
            stop_signal.store(false, Ordering::Relaxed);
            let state_engine = Arc::clone(&state);
            let stop_engine = Arc::clone(&stop_signal);
            tokio::spawn(async move {
                run_engine_worker(state_engine, stop_engine).await;
            });
        } else {
            stop_signal.store(true, Ordering::Relaxed);
        }

        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
        let _ = socket.write_all(resp.as_bytes()).await;
    } else if method == "POST" && path == "/api/settings" {
        // Parse JSON payload
        if let Some(body_start) = req.find("\r\n\r\n") {
            let body = &req[body_start + 4..];
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
                let mut sett = settings.lock().await;
                if let Some(host) = val.get("vps_host").and_then(|h| h.as_str()) {
                    sett.vps_host = host.to_string();
                }
                if let Some(prof) = val.get("active_profile").and_then(|p| p.as_str()) {
                    sett.active_profile = prof.to_string();
                }
                let _ = sett.save("settings.json");

                let mut s = state.lock().await;
                s.vps_host = sett.vps_host.clone();
                s.active_profile_path = sett.active_profile.clone();
                if let Ok(p) = GameProfile::load_from_file(&sett.active_profile) {
                    s.game_name = p.name;
                }
            }
        }
        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
        let _ = socket.write_all(resp.as_bytes()).await;
    } else {
        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = socket.write_all(resp.as_bytes()).await;
    }
}

async fn run_engine_worker(state: Arc<Mutex<GuiState>>, stop_signal: Arc<AtomicBool>) {
    info!("[Engine] Starting FastPing Acceleration Engine...");

    let (vps_host, vps_ch1, vps_ch2, profile_path) = {
        let s = state.lock().await;
        (s.vps_host.clone(), s.vps_ch1, s.vps_ch2, s.active_profile_path.clone())
    };

    // Load Profile
    let p_path = PathBuf::from(&profile_path);
    let profile = match GameProfile::load_from_file(&p_path) {
        Ok(p) => p,
        Err(_) => {
            let sub = Path::new("accelerator/client/profiles").join(p_path.file_name().unwrap_or_default());
            GameProfile::load_from_file(sub).unwrap_or_else(|_| GameProfile {
                game_id: "farever".into(),
                name: "Farever".into(),
                executables: vec!["Farever.exe".into()],
                routing: accelerator_client::config::RoutingConfig {
                    protocol: "TCP".into(),
                    ports: vec![accelerator_client::config::PortMatcher::Single(6022)],
                    target_subnets: vec!["0.0.0.0/0".into()],
                },
                optimizations: accelerator_client::config::OptimizationsConfig {
                    enable_fastconnect: true,
                    enable_multipath_dup: true,
                    packet_duplication_rate: 2,
                    dscp_tag: 46,
                },
            })
        }
    };

    {
        let mut s = state.lock().await;
        s.game_name = profile.name.clone();
    }

    // Apply Registry Tuning
    let mut reg_opt = RegistryOptimizer::new();
    let _ = reg_opt.apply_optimizations();

    // Resolve VPS endpoints
    let addr_ch1: SocketAddr = match format!("{}:{}", vps_host, vps_ch1).parse() {
        Ok(a) => a,
        Err(e) => {
            error!("Invalid VPS address {}:{}: {}", vps_host, vps_ch1, e);
            return;
        }
    };
    let addr_ch2: SocketAddr = match format!("{}:{}", vps_host, vps_ch2).parse() {
        Ok(a) => a,
        Err(e) => {
            error!("Invalid VPS address {}:{}: {}", vps_host, vps_ch2, e);
            return;
        }
    };

    // Initialize Channels
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<InterceptedPacket>(4096);
    let (in_tx, in_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(4096);

    // Bind Transport
    let transport = match MultipathTransport::bind(
        addr_ch1,
        addr_ch2,
        profile.optimizations.dscp_tag,
        in_tx,
    )
    .await {
        Ok(t) => Arc::new(t),
        Err(e) => {
            error!("Transport bind error: {}", e);
            return;
        }
    };
    transport.start().await;
    let transport_tx = transport.packet_sender();

    // Process Watcher
    let watcher = ProcessWatcher::new(profile.clone());
    let watcher_rx = watcher.subscribe();
    watcher.start().await;

    // Interception Engine (FastConnect local ACK injection is executed internally)
    let interceptor = InterceptionEngine::new(profile.clone(), watcher_rx.clone(), out_tx, in_rx);
    interceptor.start().await;

    // Forward intercepted packets
    let fwd_transport = transport_tx.clone();
    tokio::spawn(async move {
        while let Some(packet) = out_rx.recv().await {
            let _ = fwd_transport.send(packet.raw).await;
        }
    });

    // Telemetry Sync Loop with UI State (Real Live Metrics from VPS)
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    while !stop_signal.load(Ordering::Relaxed) {
        ticker.tick().await;
        let proc = watcher_rx.borrow().clone();
        let (rtt1, jit1, tx1, prb1, ack1, rtt2, jit2, tx2, prb2, ack2) = transport.get_metrics().await;

        let mut s = state.lock().await;
        s.game_detected = proc.is_running;
        s.pids = proc.pids;
        s.tracked_ports_count = proc.tracked_ports.len();

        s.path1_rtt = rtt1;
        s.path1_jitter = jit1;
        s.path1_tx = tx1;
        s.path1_probes = prb1;
        s.path1_acked = ack1;

        s.path2_rtt = rtt2;
        s.path2_jitter = jit2;
        s.path2_tx = tx2;
        s.path2_probes = prb2;
        s.path2_acked = ack2;
    }

    info!("[Engine] Stopping FastPing engine and restoring registry...");
    interceptor.stop();
    reg_opt.restore();

    {
        let mut s = state.lock().await;
        s.running = false;
        s.path1_rtt = 0.0;
        s.path2_rtt = 0.0;
    }
}
