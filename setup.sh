#!/usr/bin/env bash
# ==============================================================================
# FastPing VPS Setup Wrapper
# Runs the full deployment script from accelerator/server/deploy/setup-vps.sh
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_SCRIPT="${SCRIPT_DIR}/accelerator/server/deploy/setup-vps.sh"

if [[ ! -f "${TARGET_SCRIPT}" ]]; then
    echo "[!] Error: Deployment script not found at ${TARGET_SCRIPT}"
    exit 1
fi

chmod +x "${TARGET_SCRIPT}"
exec "${TARGET_SCRIPT}" "$@"
