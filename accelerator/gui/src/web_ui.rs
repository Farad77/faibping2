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
      background: var(--danger);
      box-shadow: 0 0 8px var(--danger);
    }

    /* Main Container */
    main {
      flex: 1;
      padding: 24px 28px;
      max-width: 1100px;
      margin: 0 auto;
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }

    /* Hero Action Card */
    .hero-card {
      background: linear-gradient(180deg, rgba(20, 24, 34, 0.95), rgba(16, 20, 28, 0.95));
      border: 1px solid var(--card-border);
      border-radius: 16px;
      padding: 24px 32px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    }
    .game-info h2 { font-size: 24px; font-weight: 700; margin-bottom: 6px; }
    .game-badge {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 10px;
      background: rgba(0, 210, 255, 0.1);
      border: 1px solid rgba(0, 210, 255, 0.3);
      color: var(--cyan);
      border-radius: 6px;
      font-size: 13px;
      font-weight: 600;
    }

    /* Power Switch */
    .btn-toggle {
      padding: 16px 36px;
      border-radius: 12px;
      font-size: 16px;
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

    /* Grid Layout */
    .grid-2 {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 20px;
    }

    .card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 14px;
      padding: 20px 24px;
    }
    .card-title {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 1px;
      margin-bottom: 16px;
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
      font-size: 42px;
      font-weight: 800;
      color: #fff;
      line-height: 1;
    }
    .metric-unit { color: var(--text-muted); font-size: 16px; font-weight: 600; }
    .metric-sub {
      margin-top: 8px;
      font-size: 13px;
      color: var(--text-muted);
      display: flex;
      gap: 16px;
    }
    .metric-sub span { color: var(--text); font-weight: 600; }

    /* FastConnect & Split-Tunnel Indicators */
    .features-list { display: flex; flex-direction: column; gap: 12px; }
    .feature-item {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 10px 14px;
      background: rgba(255, 255, 255, 0.02);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      font-size: 14px;
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
      margin-bottom: 14px;
    }
    .form-group label {
      font-size: 13px;
      color: var(--text-muted);
      font-weight: 500;
    }
    .form-input, .form-select {
      background: rgba(0, 0, 0, 0.35);
      border: 1px solid var(--card-border);
      color: #fff;
      padding: 10px 14px;
      border-radius: 8px;
      font-size: 14px;
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
      padding: 10px 18px;
      border-radius: 8px;
      font-weight: 600;
      cursor: pointer;
      width: 100%;
      margin-top: 6px;
      transition: all 0.2s;
    }
    .btn-save:hover { background: rgba(255, 255, 255, 0.15); }

    /* Live Canvas Chart */
    canvas#chart {
      width: 100%;
      height: 120px;
      display: block;
      margin-top: 10px;
    }
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
      <span id="status-text">INITIALISATION</span>
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
        <h2 id="hero-game-name">Farever (MMO)</h2>
        <div id="hero-game-badge" class="game-badge">🔍 Analyse des flux...</div>
      </div>
      <button id="btn-toggle" class="btn-toggle inactive" onclick="toggleAcceleration()">
        <span id="btn-icon">▶</span>
        <span id="btn-label">ACTIVER L'ACCÉLÉRATION</span>
      </button>
    </div>

    <!-- Telemetry Gauges (Grid 2) -->
    <div class="grid-2">
      <!-- Path 1 Gauge -->
      <div class="card">
        <div class="card-title">
          <span>Chemin Principal (UDP 51820)</span>
          <span style="color: var(--accent);">● LIVE</span>
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
          <span>Chemin Dupliqué (UDP 4433)</span>
          <span style="color: var(--cyan);">● MULTIPATH</span>
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
      <div class="card-title">Historique de Latence en Direct (Temps Réel)</div>
      <canvas id="chart" height="120"></canvas>
    </div>

    <!-- Settings and Advanced Features (Grid 2) -->
    <div class="grid-2">
      <!-- Active Optimizations -->
      <div class="card">
        <div class="card-title">Modules d'Optimisation Actifs</div>
        <div class="features-list">
          <div class="feature-item">
            <div class="feature-label">⚡ FastConnect (Anti Animation-Lock)</div>
            <div id="badge-fastconnect" class="tag-active">ACTIF (< 1 ms)</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">🔀 Multi-chemin UDP (Duplication x2)</div>
            <div id="badge-multipath" class="tag-active">0% PERTE CIBLE</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">🛡️ Strict Split-Tunneling</div>
            <div class="tag-active">ISOLATION ACTIVE</div>
          </div>
          <div class="feature-item">
            <div class="feature-label">🚀 Registre Windows (TCPNoDelay / MMCSS)</div>
            <div id="badge-registry" class="tag-active">OPTIMISÉ</div>
          </div>
        </div>
      </div>

      <!-- Settings Form -->
      <div class="card">
        <div class="card-title">Configuration de la Passerelle VPS</div>
        <div class="form-group">
          <label>Profil de Jeu Actif</label>
          <select id="setting-profile" class="form-select">
            <option value="profiles/farever.json">Farever (MMO en cours)</option>
            <option value="profiles/aion2.json">Aion 2 (MMORPG)</option>
            <option value="profiles/mock_game.json">Mock Game (Harnais de test)</option>
          </select>
        </div>
        <div class="form-group">
          <label>Adresse IP du VPS Linux</label>
          <input id="setting-vps-ip" class="form-input" type="text" value="72.61.111.131" placeholder="ex: 72.61.111.131">
        </div>
        <button class="btn-save" onclick="saveSettings()">💾 Sauvegarder les Paramètres</button>
      </div>
    </div>
  </main>

  <script>
    let isRunning = false;
    const history1 = new Array(50).fill(0);
    const history2 = new Array(50).fill(0);

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

        if (isRunning) {
          statusDot.className = 'status-dot active';
          statusText.innerText = 'ACCÉLÉRATION EN COURS';
          btnToggle.className = 'btn-toggle active';
          btnLabel.innerText = 'DÉCONNECTER / STOPPER';
          btnIcon.innerText = '⏹';
        } else {
          statusDot.className = 'status-dot inactive';
          statusText.innerText = 'EN VEILLE (STOPPÉ)';
          btnToggle.className = 'btn-toggle inactive';
          btnLabel.innerText = "ACTIVER L'ACCÉLÉRATION";
          btnIcon.innerText = '▶';
        }

        // Game status
        const heroName = document.getElementById('hero-game-name');
        const heroBadge = document.getElementById('hero-game-badge');
        heroName.innerText = data.game_name || 'Jeu non détecté';

        if (data.game_detected) {
          heroBadge.style.color = 'var(--accent)';
          heroBadge.style.borderColor = 'rgba(0, 255, 136, 0.3)';
          heroBadge.style.background = 'rgba(0, 255, 136, 0.1)';
          heroBadge.innerText = `● En ligne (PIDs: ${data.pids.join(', ')} | Sockets: ${data.tracked_ports_count})`;
        } else {
          heroBadge.style.color = 'var(--text-muted)';
          heroBadge.style.borderColor = 'var(--card-border)';
          heroBadge.style.background = 'rgba(255, 255, 255, 0.05)';
          heroBadge.innerText = '⏳ En attente du lancement du jeu...';
        }

        // Metrics
        document.getElementById('p1-rtt').innerText = data.path1_rtt > 0 ? data.path1_rtt.toFixed(1) : '--';
        document.getElementById('p1-jitter').innerText = (data.path1_jitter || 0).toFixed(2) + ' ms';
        document.getElementById('p1-tx').innerText = data.path1_tx || 0;
        document.getElementById('p1-probes').innerText = `${data.path1_acked || 0}/${data.path1_probes || 0}`;

        document.getElementById('p2-rtt').innerText = data.path2_rtt > 0 ? data.path2_rtt.toFixed(1) : '--';
        document.getElementById('p2-jitter').innerText = (data.path2_jitter || 0).toFixed(2) + ' ms';
        document.getElementById('p2-tx').innerText = data.path2_tx || 0;
        document.getElementById('p2-probes').innerText = `${data.path2_acked || 0}/${data.path2_probes || 0}`;

        // Update chart
        history1.shift();
        history1.push(data.path1_rtt || 0);
        history2.shift();
        history2.push(data.path2_rtt || 0);
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

    async function saveSettings() {
      const vps_host = document.getElementById('setting-vps-ip').value.trim();
      const active_profile = document.getElementById('setting-profile').value;

      try {
        const res = await fetch('/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ vps_host, active_profile })
        });
        if (res.ok) {
          alert("Paramètres sauvegardés avec succès dans settings.json !");
        }
      } catch (e) {
        alert("Erreur de sauvegarde: " + e);
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

      const maxVal = Math.max(300, ...history1, ...history2);

      // Draw Path 1 (Green)
      ctx.strokeStyle = '#00ff88';
      ctx.lineWidth = 2;
      ctx.beginPath();
      for (let i = 0; i < history1.length; i++) {
        const x = (i / (history1.length - 1)) * w;
        const y = h - (history1[i] / maxVal) * (h - 10);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Draw Path 2 (Cyan)
      ctx.strokeStyle = '#00d2ff';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let i = 0; i < history2.length; i++) {
        const x = (i / (history2.length - 1)) * w;
        const y = h - (history2[i] / maxVal) * (h - 10);
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
