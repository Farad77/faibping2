#!/usr/bin/env bash
# ==============================================================================
# FastPing GameTunnel VPS Gateway Deployment Script
# Targets: Ubuntu 22.04 / 24.04 LTS, Debian 12
# Configures: Kernel sysctl, BBR, CAKE qdisc, nftables NAT & MSS clamping
# ==============================================================================

set -euo pipefail

echo "[*] Initializing FastPing WAN Accelerator VPS Gateway setup..."

if [[ $EUID -ne 0 ]]; then
   echo "[!] This script must be run as root (sudo)."
   exit 1
fi

WAN_IFACE=$(ip route get 8.8.8.8 | awk -- '{print $5; exit}')
echo "[*] Detected default WAN interface: ${WAN_IFACE}"

# 1. System packages
echo "[*] Updating repositories and installing essential packages..."
apt-get update -qq
apt-get install -y -qq nftables iproute2 curl build-essential git ethtool

# 2. Kernel sysctl tuning (BBR, CAKE, IP Forwarding, Low-latency buffers)
echo "[*] Applying hardened low-latency sysctl configuration..."
cat <<EOF > /etc/sysctl.d/99-gametunnel.conf
# Enable IPv4 packet forwarding for tunnel gateway
net.ipv4.ip_forward = 1

# BBR Congestion Control & CAKE Queue Scheduler
net.core.default_qdisc = cake
net.ipv4.tcp_congestion_control = bbr

# MMO low-latency TCP tuning
net.ipv4.tcp_slow_start_after_idle = 0
net.ipv4.tcp_notsent_lowat = 16384
net.ipv4.tcp_fastopen = 3

# Buffer and backlog optimization for high-throughput / low-jitter
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216
net.core.netdev_max_backlog = 10000
EOF

# Load BBR module
modprobe tcp_bbr || echo "[!] Notice: BBR module might already be built into the kernel."
modprobe sch_cake || echo "[!] Notice: CAKE module might already be built into the kernel."

sysctl --system > /dev/null
echo "[+] Kernel parameters applied successfully."

# 3. Configure CAKE qdisc on WAN interface
echo "[*] Applying CAKE qdisc on ${WAN_IFACE}..."
tc qdisc replace dev "${WAN_IFACE}" root cake diffserv4 ack-filter || true

# 4. nftables configuration
echo "[*] Configuring nftables with MSS Clamping and Masquerading..."
mkdir -p /etc/nftables
sed "s/eth0/${WAN_IFACE}/g" "$(dirname "$0")/nftables.conf" > /etc/nftables.conf
nft -f /etc/nftables.conf
systemctl enable --now nftables

echo "[+] nftables configured and active."

# 5. Build and install accelerator-server
echo "[*] Installing Rust toolchain if not present..."
if ! command -v cargo &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "[*] Building accelerator-server release binary..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Find workspace root containing Cargo.toml
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
if [[ ! -f "${REPO_ROOT}/Cargo.toml" ]]; then
    REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
fi

echo "[*] Workspace root detected at: ${REPO_ROOT}"
cd "${REPO_ROOT}"
cargo build --release -p accelerator-server

# Locate compiled binary dynamically
BIN_SRC=""
if [[ -f "${REPO_ROOT}/target/release/accelerator-server" ]]; then
    BIN_SRC="${REPO_ROOT}/target/release/accelerator-server"
elif [[ -f "${SCRIPT_DIR}/../../target/release/accelerator-server" ]]; then
    BIN_SRC="${SCRIPT_DIR}/../../target/release/accelerator-server"
else
    BIN_SRC=$(find "${REPO_ROOT}" -type f -name "accelerator-server" | grep -v "\.d" | head -n 1)
fi

if [[ -z "${BIN_SRC}" || ! -f "${BIN_SRC}" ]]; then
    echo "[!] Error: accelerator-server binary could not be found after compilation."
    exit 1
fi

echo "[*] Installing binary from ${BIN_SRC} to /usr/local/bin/gametunnel-server..."
cp "${BIN_SRC}" /usr/local/bin/gametunnel-server
chmod +x /usr/local/bin/gametunnel-server

# 6. Systemd service creation
cat <<EOF > /etc/systemd/system/gametunnel-server.service
[Unit]
Description=FastPing GameTunnel VPS Gateway Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/gametunnel-server --listen-ch1 0.0.0.0:51820 --listen-ch2 0.0.0.0:4433 --tun-ip 10.8.0.1/24
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now gametunnel-server

echo "=========================================================================="
echo "[+] GameTunnel VPS Gateway deployment complete!"
echo "[+] Status: $(systemctl is-active gametunnel-server)"
echo "[+] Listening on UDP ports 51820 and 4433."
echo "=========================================================================="
