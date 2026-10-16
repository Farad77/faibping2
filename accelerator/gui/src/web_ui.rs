pub const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>FastPing - MMO WAN Accelerator</title>
  <style>
    :root {
      --bg: #0b0e14;
      --card-bg: #141822;
      --card-border: #232936;
      --accent: #00ff88;
      --accent-glow: rgba(0, 255, 136, 0.35);
      --cyan: #00d2ff;
      --cyan-glow: rgba(0, 210, 255, 0.3);
      --orange: #ff9f43;
      --orange-glow: rgba(255, 159, 67, 0.3);
      --danger: #ff3366;
      --danger-glow: rgba(255, 51, 102, 0.4);
      --text: #f0f3f8;
      --text-muted: #8b949e;
      --font: 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, sans-serif;
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: var(--font);
      user-select: none;
      min-height: 100vh;
      overflow-x: hidden;
      display: flex;
      flex-direction: column;
    }

    /* Top Bar */
    header {
      background: rgba(20, 24, 34, 0.85);
      backdrop-filter: blur(12px);
      border-bottom: 1px solid var(--card-border);
      padding: 14px 28px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      position: sticky;
      top: 0;
      z-index: 100;
    }
    .logo-area { display: flex; align-items: center; gap: 12px; }
    .logo-icon {
      width: 32px;
      height: 32px;
      background: linear-gradient(135deg, var(--accent), var(--cyan));
      border-radius: 8px;
      display: flex;
      align-items: center;
      justify-content: center;
      font-weight: 900;
      color: #0b0e14;
      font-size: 18px;
      box-shadow: 0 0 16px var(--accent-glow);
    }
    .logo-text { font-size: 18px; font-weight: 700; letter-spacing: 0.5px; }
    .logo-text span { color: var(--accent); }
    .status-badge {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 14px;
      border-radius: 20px;
      font-size: 12px;
      font-weight: 600;
      background: rgba(255, 255, 255, 0.05);
      border: 1px solid var(--card-border);
    }
    .status-dot {
      width: 10px;
      height: 10px;
      border-radius: 50%;
      background: #8b949e;
      transition: all 0.3s;
    }
    .status-dot.active {
      background: var(--accent);
      box-shadow: 0 0 10px var(--accent);
    }
    .status-dot.inactive {
      background: var(--orange);
      box-shadow: 0 0 8px var(--orange);
    }

    /* Main Container */
    main {
      flex: 1;
      padding: 20px 28px;
      max-width: 1120px;
      margin: 0 auto;
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }

    /* Hero Action Card */
    .hero-card {
      background: linear-gradient(180deg, rgba(20, 24, 34, 0.95), rgba(16, 20, 28, 0.95));
      border: 1px solid var(--card-border);
      border-radius: 16px;
      padding: 20px 28px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    }
    .game-info h2 { font-size: 24px; font-weight: 700; margin-bottom: 6px; }
    .badges-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
    .game-badge {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 10px;
      background: rgba(0, 210, 255, 0.1);
      border: 1px solid rgba(0, 210, 255, 0.3);
      color: var(--cyan);
      border-radius: 6px;
      font-size: 12px;
      font-weight: 600;
    }

    /* Power Switch */
    .btn-toggle {
      padding: 16px 32px;
      border-radius: 12px;
      font-size: 15px;
      font-weight: 700;
      letter-spacing: 0.5px;
      cursor: pointer;
      border: none;
      transition: all 0.25s ease;
      display: flex;
      align-items: center;
      gap: 10px;
      box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
    }
    .btn-toggle.active {
      background: linear-gradient(135deg, #ff3366, #ff1a40);
      color: #fff;
      box-shadow: 0 0 24px var(--danger-glow);
    }
    .btn-toggle.inactive {
      background: linear-gradient(135deg, #00ff88, #00d2ff);
      color: #0b0e14;
      box-shadow: 0 0 24px var(--accent-glow);
    }
    .btn-toggle:hover { transform: translateY(-2px); }
    .btn-toggle:active { transform: translateY(1px); }

    /* Grid Layouts */
    .grid-2 {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 18px;
    }
    .grid-3 {
      display: grid;
      grid-template-columns: 1fr 1fr 1fr;
      gap: 16px;
    }

    .card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 14px;
      padding: 18px 22px;
    }
    .card-highlight {
      border-color: rgba(0, 255, 136, 0.35);
      background: linear-gradient(180deg, rgba(20, 28, 36, 0.95), rgba(20, 24, 34, 0.95));
    }
    .card-title {
      font-size: 13px;
      font-weight: 600;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.8px;
      margin-bottom: 12px;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }

    /* Metrics HUD */
    .metric-value-box {
      display: flex;
      align-items: baseline;
      gap: 8px;
    }
    .metric-big {
      font-size: 38px;
      font-weight: 800;
      color: #fff;
      line-height: 1;
    }
    .metric-unit { color: var(--text-muted); font-size: 15px; font-weight: 600; }
    .metric-sub {
      margin-top: 8px;
      font-size: 12px;
      color: var(--text-muted);
      display: flex;
      gap: 14px;
      flex-wrap: wrap;
    }
    .metric-sub span { color: var(--text); font-weight: 600; }

    /* FastConnect & Split-Tunnel Indicators */
    .features-list { display: flex; flex-direction: column; gap: 10px; }
    .feature-item {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 9px 12px;
      background: rgba(255, 255, 255, 0.02);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      font-size: 13px;
    }
    .feature-label { display: flex; align-items: center; gap: 8px; font-weight: 500; }
    .tag-active {
      background: rgba(0, 255, 136, 0.15);
      color: var(--accent);
      padding: 3px 8px;
      border-radius: 4px;
      font-size: 11px;
      font-weight: 700;
    }

    /* Settings Form */
    .form-group {
      display: flex;
      flex-direction: column;
      gap: 6px;
      margin-bottom: 12px;
    }
    .form-group label {
      font-size: 12px;
      color: var(--text-muted);
      font-weight: 500;
    }
    .form-input, .form-select {
      background: rgba(0, 0, 0, 0.35);
      border: 1px solid var(--card-border);
      color: #fff;
      padding: 9px 12px;
      border-radius: 8px;
      font-size: 13px;
      outline: none;
      transition: border 0.2s;
    }
    .form-input:focus, .form-select:focus {
      border-color: var(--cyan);
      box-shadow: 0 0 10px var(--cyan-glow);
    }
    .btn-save {
      background: rgba(255, 255, 255, 0.08);
      color: #fff;
      border: 1px solid var(--card-border);
      padding: 9px 16px;
      border-radius: 8px;
      font-weight: 600;
      cursor: pointer;
      width: 100%;
      margin-top: 4px;
      transition: all 0.2s;
    }
    .btn-save:hover { background: rgba(255, 255, 255, 0.15); }

    /* Live Canvas Chart */
    canvas#chart {
      width: 100%;
      height: 125px;
      display: block;
      margin-top: 8px;
    }
    .chart-legend {
      display: flex;
      gap: 16px;
      font-size: 11px;
      text-transform: none;
      letter-spacing: 0;
    }
    .legend-item { display: flex; align-items: center; gap: 6px; }
    .legend-dot { width: 10px; height: 3px; border-radius: 2px; }
  </style>
</head>
<body>
  <header>
    <div class="logo-area">
      <div class="logo-icon">⚡</div>
      <div class="logo-text">Fast<span>Ping</span> MMO Accelerator</div>
    </div>
    <div class="status-badge">
      <div id="status-dot" class="status-dot"></div>
      <span id="status-text">SURVEILLANCE ACTIVE</span>
    </div>
  </header>

  <main>
    <!-- Admin Warning Banner -->
    <div id="admin-warning" style="display: none; background: rgba(255, 51, 102, 0.15); border: 1px solid var(--danger); color: #ff99aa; padding: 14px 20px; border-radius: 12px; font-size: 13px; font-weight: 600; align-items: center; gap: 12px; box-shadow: 0 4px 16px var(--danger-glow);">
      <span style="font-size: 22px;">⚠️</span>
      <div>
        <div style="color: #fff; font-size: 14px; margin-bottom: 2px;">PRIVILÈGES ADMINISTRATEUR REQUIS</div>
        L'interception de paquets noyau (WinDivert) et les réglages du Registre nécessitent les droits administrateur. Relancez via <b style="color: #fff;">Lancer-FastPing.bat</b> ou faites un clic droit &rarr; <i>"Exécuter en tant qu'administrateur"</i>.
      </div>
    </div>

    <!-- Hero Toggle Card -->
    <div class="hero-card">
      <div class="game-info">
        <h2 id="hero-game-name">Aion 2 (MMORPG)</h2>
        <div class="badges-row">
          <div id="hero-game-badge" class="game-badge">🔍 Analyse des processus...</div>
          <div id="hero-target-badge" class="game-badge" style="color: var(--orange); border-color: rgba(255,159,67,0.35); background: rgba(255,159,67,0.1);">🎯 Serveur: --</div>
        </div>
      </div>
      <button id="btn-toggle" class="btn-toggle inactive" onclick="toggleAcceleration()">
        <span id="btn-icon">▶</span>
        <span id="btn-label">ACTIVER L'ACCÉLÉRATION</span>
      </button>
    </div>

    <!-- Before / After Live Comparison Row (Grid 3) -->
    <div class="grid-3">
      <!-- Before (Direct ISP) -->
      <div class="card">
        <div class="card-title">
          <span>Avant (Direct FAI / Sans Boost)</span>
          <span style="color: var(--orange);">● DIRECT LIVE</span>
        </div>
        <div class="metric-value-box">
          <div id="direct-rtt" class="metric-big" style="color: var(--orange);">--</div>
          <div class="metric-unit">ms</div>
        </div>
        <div class="metric-sub">
          <div>Gigue Directe: <span id="direct-jitter">-- ms</span></div>
          <div>Référence: <span id="baseline-rtt">-- ms</span></div>
        </div>
      </div>

      <!-- After (FastPing Tunnel) -->
      <div class="card card-highlight">
        <div class="card-title">
          <span>Après (Tunnel FastPing)</span>
          <span id="tunnel-mode-tag" style="color: var(--accent);">● PRÉVISUALISATION</span>
        </div>
        <div class="metric-value-box">
          <div id="best-tunnel-rtt" class="metric-big" style="color: var(--accent);">--</div>
          <div class="metric-unit">ms</div>
        </div>
        <div class="metric-sub">
          <div>Gigue Tunnel: <span id="best-tunnel-jitter">-- ms</span></div>
          <div>Perte Paquets: <span style="color: var(--accent);">0.0% (x2)</span></div>
        </div>
      </div>

      <!-- Delta / Comparison Summary -->
      <div class="card">
        <div class="card-title">
          <span>Comparatif Avant / Après</span>
          <span id="comp-status-tag" style="color: var(--cyan);">TEMPS RÉEL</span>
        </div>
        <div class="metric-value-box">
          <div id="delta-rtt" class="metric-big">--</div>
          <div id="delta-unit" class="metric-unit">ms</div>
        </div>
        <div class="metric-sub">
          <div>Gain Stabilité: <span id="jitter-gain" style="color: var(--accent);">--</span></div>
          <div>Anti-Loss: <span style="color: var(--cyan);">Décalage 3ms</span></div>
        </div>
      </div>
    </div>

    <!-- Multipath Channels (Grid 2) -->
    <div class="grid-2">
      <!-- Path 1 Gauge -->
      <div class="card">
        <div class="card-title">
          <span>Canal 1 — Principal (UDP 51820)</span>
          <span style="color: var(--accent);">● 0ms DELAY</span>
        </div>
        <div class="metric-value-box">
          <div id="p1-rtt" class="metric-big">--</div>
          <div class="metric-unit">ms</div>
        </div>
        <div class="metric-sub">
          <div>Gigue: <span id="p1-jitter">-- ms</span></div>
          <div>Paquets Tx: <span id="p1-tx">0</span></div>
          <div>Sondes: <span id="p1-probes">0/0</span></div>
        </div>
      </div>

      <!-- Path 2 Gauge -->
      <div class="card">
        <div class="card-title">
          <span>Canal 2 — Redondant Décalé (UDP 4433)</span>
          <span style="color: var(--cyan);">● +3ms STAGGER</span>
        </div>
        <div class="metric-value-box">
          <div id="p2-rtt" class="metric-big">--</div>
          <div class="metric-unit">ms</div>
        </div>
        <div class="metric-sub">
          <div>Gigue: <span id="p2-jitter">-- ms</span></div>
          <div>Paquets Tx: <span id="p2-tx">0</span></div>
          <div>Sondes: <span id="p2-probes">0/0</span></div>
        </div>
      </div>
    </div>

    <!-- Live Latency Chart -->
    <div class="card">
      <div class="card-title">
        <span>Graphique Comparatif de Latence en Direct (Avant vs Après)</span>
        <div class="chart-legend">
          <div class="legend-item"><div class="legend-dot" style="background: #ff9f43;"></div><span style="color: #ff9f43;">Direct (Sans Boost)</span></div>
          <div class="legend-item"><div class="legend-dot" style="background: #00ff88;"></div><span style="color: #00ff88;">FastPing Canal 1</span></div>
          <div class="legend-item"><div class="legend-dot" style="background: #00d2ff;"></div><span style="color: #00d2ff;">FastPing Canal 2</span></div>
        </div>
      </div>
      <canvas id="chart" height="125"></canvas>
    </div>

    <!-- Settings and Advanced Features (Grid 2) -->
    <div class="grid-2">
      <!-- Active Optimizations -->
      <div class="card">
        <div class="card-title">Technologies d'Optimisation (Niveau ExitLag / LagZapper)</div>
        <div class="features-list">
          <div class="feature-item">
            <div class="feature-label">🔀 Multipath UDP + Décalage 3ms (Anti Wi-Fi/4G Drop)</div>
            <div class="tag-active">ACTIF (x2)</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">✂️ Clamping TCP MSS Dynamique (1360B Anti-Fragmentation)</div>
            <div class="tag-active">0% FRAG</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">🎯 Auto-Détection Sockets par PID + FPS CPU Boost</div>
            <div class="tag-active">PRIORITÉ HAUTE</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">🚀 Registre Windows (TCPNoDelay / AckFreq=1 / AFD / MMCSS)</div>
            <div class="tag-active">OPTIMISÉ</div>
          </div>
        </div>
      </div>

      <!-- Settings Form -->
      <div class="card">
        <div class="card-title">Configuration du Jeu & Passerelle VPS</div>
        <div class="form-group">
          <label>Profil de Jeu Sélectionné (Mesure Directe Automatique)</label>
          <select id="setting-profile" class="form-select" onchange="onProfileSelectChange()">
            <option value="profiles/aion2.json">Aion 2 (MMORPG)</option>
            <option value="profiles/pathofexile.json">Path of Exile (ARPG / Lockstep)</option>
            <option value="profiles/farever.json">Farever (MMO)</option>
            <option value="profiles/mock_game.json">Mock Game (Harnais de test)</option>
          </select>
        </div>
        <div class="form-group">
          <label>Adresse IP du VPS Linux</label>
          <input id="setting-vps-ip" class="form-input" type="text" value="72.61.111.131" placeholder="ex: 72.61.111.131">
        </div>
        <button class="btn-save" onclick="saveSettings(true)">💾 Sauvegarder les Paramètres</button>
      </div>
    </div>
  </main>

  <script>
    let isRunning = false;
    let profilesLoaded = false;
    const historyDirect = new Array(50).fill(0);
    const history1 = new Array(50).fill(0);
    const history2 = new Array(50).fill(0);

    async function loadProfiles(activeProfile) {
      if (profilesLoaded) return;
      try {
        const res = await fetch('/api/profiles');
        if (res.ok) {
          const list = await res.json();
          if (list && list.length > 0) {
            const sel = document.getElementById('setting-profile');
            sel.innerHTML = '';
            for (const p of list) {
              const opt = document.createElement('option');
              opt.value = p.path;
              opt.innerText = p.name;
              if (activeProfile && (p.path === activeProfile || p.path.endsWith(activeProfile))) {
                opt.selected = true;
              }
              sel.appendChild(opt);
            }
            profilesLoaded = true;
          }
        }
      } catch (e) {
        console.error("Failed to load profiles:", e);
      }
    }

    async function onProfileSelectChange() {
      await saveSettings(false);
    }

    async function pollStatus() {
      try {
        const res = await fetch('/api/status');
        const data = await res.json();

        isRunning = data.running;

        const adminWarning = document.getElementById('admin-warning');
        if (data.is_admin === false) {
          adminWarning.style.display = 'flex';
        } else {
          adminWarning.style.display = 'none';
        }

        const statusDot = document.getElementById('status-dot');
        const statusText = document.getElementById('status-text');
        const btnToggle = document.getElementById('btn-toggle');
        const btnLabel = document.getElementById('btn-label');
        const btnIcon = document.getElementById('btn-icon');
        const tunnelModeTag = document.getElementById('tunnel-mode-tag');

        if (isRunning) {
          statusDot.className = 'status-dot active';
          statusText.innerText = 'ACCÉLÉRATION EN COURS';
          btnToggle.className = 'btn-toggle active';
          btnLabel.innerText = 'DÉCONNECTER / STOPPER';
          btnIcon.innerText = '⏹';
          tunnelModeTag.innerText = '● BOOST ACTIF';
          tunnelModeTag.style.color = 'var(--accent)';
        } else {
          statusDot.className = 'status-dot inactive';
          statusText.innerText = 'SONDE DIRECTE ACTIVE (BOOST OFF)';
          btnToggle.className = 'btn-toggle inactive';
          btnLabel.innerText = "ACTIVER L'ACCÉLÉRATION";
          btnIcon.innerText = '▶';
          tunnelModeTag.innerText = '● PRÉVISUALISATION TUNNEL';
          tunnelModeTag.style.color = 'var(--cyan)';
        }

        // Game & Server status
        const heroName = document.getElementById('hero-game-name');
        const heroBadge = document.getElementById('hero-game-badge');
        const targetBadge = document.getElementById('hero-target-badge');
        heroName.innerText = data.game_name || 'Jeu sélectionné';
        targetBadge.innerText = `🎯 ${data.target_endpoint || 'Analyse...'}`;

        if (!profilesLoaded && data.active_profile) {
          loadProfiles(data.active_profile);
        }
        if (data.vps_host) {
          const ipInput = document.getElementById('setting-vps-ip');
          if (document.activeElement !== ipInput && !ipInput.value) {
            ipInput.value = data.vps_host;
          }
        }

        if (data.game_detected) {
          heroBadge.style.color = 'var(--accent)';
          heroBadge.style.borderColor = 'rgba(0, 255, 136, 0.3)';
          heroBadge.style.background = 'rgba(0, 255, 136, 0.1)';
          heroBadge.innerText = `● Jeu Détecté (PIDs: ${data.pids.join(', ')} | Sockets: ${data.tracked_ports_count})`;
        } else {
          heroBadge.style.color = 'var(--text-muted)';
          heroBadge.style.borderColor = 'var(--card-border)';
          heroBadge.style.background = 'rgba(255, 255, 255, 0.05)';
          heroBadge.innerText = '⏳ Jeu fermé (Mesure sur serveur de référence)';
        }

        // Direct Metrics (Sans Boost)
        const dRtt = data.direct_rtt || 0;
        const dJit = data.direct_jitter || 0;
        const baseRtt = data.baseline_rtt || dRtt;
        document.getElementById('direct-rtt').innerText = dRtt > 0 ? dRtt.toFixed(1) : '--';
        document.getElementById('direct-jitter').innerText = dJit.toFixed(2) + ' ms';
        document.getElementById('baseline-rtt').innerText = baseRtt > 0 ? baseRtt.toFixed(1) + ' ms' : '-- ms';

        // Tunnel Metrics (Avec Boost)
        const p1 = data.path1_rtt || 0;
        const p2 = data.path2_rtt || 0;
        const j1 = data.path1_jitter || 0;
        const j2 = data.path2_jitter || 0;

        let bestTunnelRtt = 0;
        let bestTunnelJit = 0;
        if (p1 > 0 && p2 > 0) {
          bestTunnelRtt = Math.min(p1, p2);
          bestTunnelJit = Math.min(j1, j2);
        } else if (p1 > 0) {
          bestTunnelRtt = p1;
          bestTunnelJit = j1;
        } else if (p2 > 0) {
          bestTunnelRtt = p2;
          bestTunnelJit = j2;
        }

        document.getElementById('best-tunnel-rtt').innerText = bestTunnelRtt > 0 ? bestTunnelRtt.toFixed(1) : '--';
        document.getElementById('best-tunnel-jitter').innerText = bestTunnelJit.toFixed(2) + ' ms';

        // Comparison Delta
        const deltaEl = document.getElementById('delta-rtt');
        const jitGainEl = document.getElementById('jitter-gain');
        if (dRtt > 0 && bestTunnelRtt > 0) {
          const diff = bestTunnelRtt - dRtt;
          const sign = diff > 0 ? '+' : '';
          deltaEl.innerText = `${sign}${diff.toFixed(0)}`;
          deltaEl.style.color = diff <= 5 ? 'var(--accent)' : 'var(--cyan)';

          if (dJit > bestTunnelJit && dJit > 0.5) {
            const pct = Math.min(99, Math.round(((dJit - bestTunnelJit) / dJit) * 100));
            jitGainEl.innerText = `-${pct}% de gigue`;
          } else {
            jitGainEl.innerText = 'Flux lissé (x2)';
          }
        } else {
          deltaEl.innerText = '--';
          jitGainEl.innerText = '--';
        }

        // Individual Channel Cards
        document.getElementById('p1-rtt').innerText = p1 > 0 ? p1.toFixed(1) : '--';
        document.getElementById('p1-jitter').innerText = j1.toFixed(2) + ' ms';
        document.getElementById('p1-tx').innerText = data.path1_tx || 0;
        document.getElementById('p1-probes').innerText = `${data.path1_acked || 0}/${data.path1_probes || 0}`;

        document.getElementById('p2-rtt').innerText = p2 > 0 ? p2.toFixed(1) : '--';
        document.getElementById('p2-jitter').innerText = j2.toFixed(2) + ' ms';
        document.getElementById('p2-tx').innerText = data.path2_tx || 0;
        document.getElementById('p2-probes').innerText = `${data.path2_acked || 0}/${data.path2_probes || 0}`;

        // Update chart
        historyDirect.shift();
        historyDirect.push(dRtt);
        history1.shift();
        history1.push(p1);
        history2.shift();
        history2.push(p2);
        drawChart();

      } catch (e) {
        console.error("Poll error:", e);
      }
    }

    async function toggleAcceleration() {
      try {
        await fetch('/api/toggle', { method: 'POST' });
        setTimeout(pollStatus, 200);
      } catch (e) {
        alert("Erreur de basculement: " + e);
      }
    }

    async function saveSettings(showToast = true) {
      const vps_host = document.getElementById('setting-vps-ip').value.trim();
      const active_profile = document.getElementById('setting-profile').value;

      try {
        const res = await fetch('/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ vps_host, active_profile })
        });
        if (res.ok) {
          if (showToast) {
            alert("Paramètres sauvegardés avec succès !");
          }
          pollStatus();
        }
      } catch (e) {
        if (showToast) alert("Erreur de sauvegarde: " + e);
      }
    }

    function drawChart() {
      const canvas = document.getElementById('chart');
      const ctx = canvas.getContext('2d');
      const w = canvas.width = canvas.offsetWidth;
      const h = canvas.height = canvas.offsetHeight;

      ctx.clearRect(0, 0, w, h);

      // Draw grid lines
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
      ctx.lineWidth = 1;
      for (let y = 0; y < h; y += 30) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }

      const maxVal = Math.max(350, ...historyDirect, ...history1, ...history2) * 1.1;

      // 1. Draw Direct Sans Boost (Orange Dashed)
      ctx.strokeStyle = '#ff9f43';
      ctx.lineWidth = 2;
      ctx.setLineDash([5, 4]);
      ctx.beginPath();
      for (let i = 0; i < historyDirect.length; i++) {
        const x = (i / (historyDirect.length - 1)) * w;
        const y = h - (historyDirect[i] / maxVal) * (h - 12);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.setLineDash([]);

      // 2. Draw Path 1 (Green Solid)
      ctx.strokeStyle = '#00ff88';
      ctx.lineWidth = 2.2;
      ctx.beginPath();
      for (let i = 0; i < history1.length; i++) {
        const x = (i / (history1.length - 1)) * w;
        const y = h - (history1[i] / maxVal) * (h - 12);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // 3. Draw Path 2 (Cyan Solid)
      ctx.strokeStyle = '#00d2ff';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let i = 0; i < history2.length; i++) {
        const x = (i / (history2.length - 1)) * w;
        const y = h - (history2[i] / maxVal) * (h - 12);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }

    setInterval(pollStatus, 1000);
    pollStatus();
  </script>
</body>
</html>
"#;
