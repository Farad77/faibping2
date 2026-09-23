#!/usr/bin/env bash
# ==============================================================================
# FastPing VPS Setup Wrapper
# Runs setup-vps.sh whether invoked from root or deploy directory
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -f "${SCRIPT_DIR}/setup-vps.sh" ]]; then
    TARGET_SCRIPT="${SCRIPT_DIR}/setup-vps.sh"
elif [[ -f "${SCRIPT_DIR}/accelerator/server/deploy/setup-vps.sh" ]]; then
    TARGET_SCRIPT="${SCRIPT_DIR}/accelerator/server/deploy/setup-vps.sh"
else
    TARGET_SCRIPT=$(find "${SCRIPT_DIR}" -name "setup-vps.sh" 2>/dev/null | head -n 1)
fi

if [[ -z "${TARGET_SCRIPT:-}" || ! -f "${TARGET_SCRIPT}" ]]; then
    echo "[!] Error: Deployment script setup-vps.sh could not be found."
    exit 1
fi

chmod +x "${TARGET_SCRIPT}"
exec "${TARGET_SCRIPT}" "$@"
