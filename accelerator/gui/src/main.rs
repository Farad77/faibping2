//! FastPing Desktop GUI & Control Center
//!
//! Provides a modern zero-terminal desktop user interface.
//! Serves an embedded high-performance dark gaming dashboard on 127.0.0.1:4040
//! and launches a dedicated native application window.
//! Includes an Always-On Latency & Process Monitor for live Before/After comparison.

use std::collections::HashSet;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{error, info};

mod web_ui;

use accelerator_client::config::GameProfile;
use accelerator_client::intercept::{InterceptedPacket, InterceptionEngine, DIRECT_PROBE_PORT};
use accelerator_client::registry::RegistryOptimizer;
use accelerator_client::settings::AppSettings;
use accelerator_client::transport::MultipathTransport;
use accelerator_client::watcher::{scan_game_state, ProcessWatcher};
use accelerator_common::protocol::{GameTunnelHeader, HEADER_LEN};

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
    target_endpoint: String,
    direct_rtt: f64,
    direct_jitter: f64,
    baseline_rtt: f64,
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
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("fastping.log")
        .unwrap_or_else(|_| std::fs::File::create("fastping_backup.log").unwrap());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(log_file)
        .init();

    info!("============================================================");
    info!("Starting FastPing Desktop GUI & Control Center");
    info!("============================================================");

    let settings = Arc::new(Mutex::new(AppSettings::load_or_default("settings.json")));
    let initial_settings = settings.lock().await.clone();
    let initial_profile = load_profile_with_fallback(&initial_settings.active_profile);
    let initial_game_name = initial_profile.name.clone();
    let initial_target = format!("{} (Cible)", initial_profile.default_benchmark_endpoint());

    let is_admin = RegistryOptimizer::is_admin();
    let state = Arc::new(Mutex::new(GuiState {
        running: false,
        is_admin,
        vps_host: initial_settings.vps_host.clone(),
        vps_ch1: initial_settings.vps_ch1,
        vps_ch2: initial_settings.vps_ch2,
        active_profile_path: initial_settings.active_profile.clone(),
        game_name: initial_game_name,
        target_endpoint: initial_target,
        ..Default::default()
    }));

    let stop_signal = Arc::new(AtomicBool::new(false));

    // Spawn Always-On Background Game & Latency Monitor (runs even when Boost is OFF)
    let state_monitor = Arc::clone(&state);
    tokio::spawn(async move {
        run_always_on_monitor(state_monitor).await;
    });

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

fn load_profile_with_fallback(profile_path: &str) -> GameProfile {
    let p_path = PathBuf::from(profile_path);
    match GameProfile::load_from_file(&p_path) {
        Ok(p) => p,
        Err(_) => {
            let sub = Path::new("accelerator/client/profiles")
                .join(p_path.file_name().unwrap_or_default());
            GameProfile::load_from_file(sub).unwrap_or_else(|_| GameProfile {
                game_id: "aion2".into(),
                name: "Aion 2 (MMORPG)".into(),
                executables: vec!["AION2.exe".into(), "Aion2.exe".into()],
                routing: accelerator_client::config::RoutingConfig {
                    protocol: "BOTH".into(),
                    ports: vec![accelerator_client::config::PortMatcher::Single(7777)],
                    target_subnets: vec!["0.0.0.0/0".into()],
                    benchmark_endpoint: Some("13.232.110.60:443".into()),
                },
                optimizations: accelerator_client::config::OptimizationsConfig {
                    enable_fastconnect: false,
                    enable_multipath_dup: true,
                    packet_duplication_rate: 2,
                    dscp_tag: 46,
                },
            })
        }
    }
}

/// Runs continuously in the background (even when Boost is OFF) to:
/// 1. Detect the currently selected game process and its live remote server IP:Port.
/// 2. Measure Direct ISP Latency & Jitter to the game server (using a bypassed probe port).
/// 3. Measure VPS Tunnel Latency & Jitter when Boost is OFF so Before/After comparison is always live.
async fn run_always_on_monitor(state: Arc<Mutex<GuiState>>) {
    let mut ticker = tokio::time::interval(Duration::from_millis(1500));
    let mut boosted_pids = HashSet::new();
    let mut probe_seq = 1u32;

    loop {
        ticker.tick().await;
        probe_seq = probe_seq.wrapping_add(1);

        let (is_running, profile_path, vps_host, vps_ch1, vps_ch2) = {
            let s = state.lock().await;
            (
                s.running,
                s.active_profile_path.clone(),
                s.vps_host.clone(),
                s.vps_ch1,
                s.vps_ch2,
            )
        };

        let profile = load_profile_with_fallback(&profile_path);
        let proc_state = scan_game_state(&profile, &mut boosted_pids);

        // Determine target game server SocketAddr (live socket if game is running, else profile benchmark target)
        let (target_addr_opt, endpoint_label) = if let Some(live_addr) = proc_state.active_remote_endpoint {
            (Some(live_addr), format!("{} (Serveur Actif)", live_addr))
        } else {
            let fallback_str = profile.default_benchmark_endpoint();
            let resolved = fallback_str
                .to_socket_addrs()
                .ok()
                .and_then(|mut it| it.find(|a| a.is_ipv4()));
            (resolved, format!("{} (Serveur Cible)", fallback_str))
        };

        // Measure Direct ISP Latency (bypassing WinDivert via DIRECT_PROBE_PORT)
        let direct_sample = if let Some(target_addr) = target_addr_opt {
            measure_direct_tcp_rtt(target_addr).await
        } else {
            None
        };

        // When Boost is OFF, also probe VPS tunnel channels so Before/After comparison is ready before activation
        let (idle_p1, idle_p2) = if !is_running {
            let ch1_addr: Option<SocketAddr> = format!("{}:{}", vps_host, vps_ch1).parse().ok();
            let ch2_addr: Option<SocketAddr> = format!("{}:{}", vps_host, vps_ch2).parse().ok();
            let (r1, r2) = tokio::join!(
                async {
                    if let Some(a) = ch1_addr {
                        probe_vps_udp(a, probe_seq).await
                    } else {
                        None
                    }
                },
                async {
                    if let Some(a) = ch2_addr {
                        probe_vps_udp(a, probe_seq).await
                    } else {
                        None
                    }
                }
            );
            (r1, r2)
        } else {
            (None, None)
        };

        let mut s = state.lock().await;
        s.game_name = profile.name;
        s.target_endpoint = endpoint_label;

        if !s.running {
            s.game_detected = proc_state.is_running;
            s.pids = proc_state.pids;
            s.tracked_ports_count = proc_state.tracked_ports.len();
        }

        if let Some(sample) = direct_sample {
            let (nrtt, njit) = compute_ewma(s.direct_rtt, s.direct_jitter, sample);
            s.direct_rtt = nrtt;
            s.direct_jitter = njit;
            if !s.running || s.baseline_rtt == 0.0 {
                s.baseline_rtt = s.direct_rtt;
            }
        }

        if !s.running {
            if let Some(r1) = idle_p1 {
                s.path1_probes += 1;
                s.path1_acked += 1;
                let (nrtt, njit) = compute_ewma(s.path1_rtt, s.path1_jitter, r1);
                s.path1_rtt = nrtt;
                s.path1_jitter = njit;
            }
            if let Some(r2) = idle_p2 {
                s.path2_probes += 1;
                s.path2_acked += 1;
                let (nrtt, njit) = compute_ewma(s.path2_rtt, s.path2_jitter, r2);
                s.path2_rtt = nrtt;
                s.path2_jitter = njit;
            }
        }
    }
}

fn compute_ewma(rtt: f64, jitter: f64, sample: f64) -> (f64, f64) {
    if rtt == 0.0 {
        (sample, 0.0)
    } else {
        let diff = (sample - rtt).abs();
        let new_rtt = 0.8 * rtt + 0.2 * sample;
        let new_jitter = jitter + (diff - jitter) / 8.0;
        (new_rtt, new_jitter)
    }
}

async fn measure_direct_tcp_rtt(target: SocketAddr) -> Option<f64> {
    let sock = tokio::net::TcpSocket::new_v4().ok()?;
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().ok()?;
    sock.bind(bind_addr).ok()?;
    if let Ok(local_addr) = sock.local_addr() {
        DIRECT_PROBE_PORT.store(local_addr.port(), Ordering::Relaxed);
    }
    let t0 = Instant::now();
    let res = tokio::time::timeout(Duration::from_millis(1200), sock.connect(target)).await;
    match res {
        Ok(Ok(_stream)) => Some(t0.elapsed().as_secs_f64() * 1000.0),
        _ => None,
    }
}

async fn probe_vps_udp(vps_addr: SocketAddr, seq: u32) -> Option<f64> {
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let mut buf = [0u8; HEADER_LEN];
    let hdr = GameTunnelHeader::new(seq, 0, false, true);
    hdr.encode(&mut buf).ok()?;
    let t0 = Instant::now();
    sock.send_to(&buf, vps_addr).await.ok()?;
    let mut recv_buf = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(1000), sock.recv_from(&mut recv_buf)).await {
        Ok(Ok((n, _))) if n >= HEADER_LEN => {
            if let Ok(resp) = GameTunnelHeader::decode(&recv_buf[..n]) {
                if resp.is_ack() {
                    return Some(t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
            None
        }
        _ => None,
    }
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
            "target_endpoint": s.target_endpoint,
            "direct_rtt": s.direct_rtt,
            "direct_jitter": s.direct_jitter,
            "baseline_rtt": s.baseline_rtt,
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
                let mut profile_changed = false;
                if let Some(prof) = val.get("active_profile").and_then(|p| p.as_str()) {
                    if sett.active_profile != prof {
                        profile_changed = true;
                    }
                    sett.active_profile = prof.to_string();
                }
                let _ = sett.save("settings.json");

                let mut s = state.lock().await;
                s.vps_host = sett.vps_host.clone();
                s.active_profile_path = sett.active_profile.clone();
                let loaded = load_profile_with_fallback(&sett.active_profile);
                s.game_name = loaded.name.clone();
                if profile_changed {
                    s.direct_rtt = 0.0;
                    s.direct_jitter = 0.0;
                    s.baseline_rtt = 0.0;
                    s.target_endpoint = format!("{} (Cible)", loaded.default_benchmark_endpoint());
                }
            }
        }
        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
        let _ = socket.write_all(resp.as_bytes()).await;
    } else if method == "POST" && path == "/api/exit" {
        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
        let _ = socket.write_all(resp.as_bytes()).await;
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            std::process::exit(0);
        });
    } else {
        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = socket.write_all(resp.as_bytes()).await;
    }
}

async fn run_engine_worker(state: Arc<Mutex<GuiState>>, stop_signal: Arc<AtomicBool>) {
    info!("[Engine] Starting FastPing Acceleration Engine...");

    let (vps_host, vps_ch1, vps_ch2, profile_path) = {
        let s = state.lock().await;
        (
            s.vps_host.clone(),
            s.vps_ch1,
            s.vps_ch2,
            s.active_profile_path.clone(),
        )
    };

    let profile = load_profile_with_fallback(&profile_path);

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
    .await
    {
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

    // Interception Engine
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
        let (rtt1, jit1, tx1, prb1, ack1, rtt2, jit2, tx2, prb2, ack2) =
            transport.get_metrics().await;

        let mut s = state.lock().await;
        s.game_detected = proc.is_running;
        s.pids = proc.pids;
        s.tracked_ports_count = proc.tracked_ports.len();

        if rtt1 > 0.0 {
            s.path1_rtt = rtt1;
            s.path1_jitter = jit1;
        }
        s.path1_tx = tx1;
        s.path1_probes = prb1;
        s.path1_acked = ack1;

        if rtt2 > 0.0 {
            s.path2_rtt = rtt2;
            s.path2_jitter = jit2;
        }
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
    }
}
