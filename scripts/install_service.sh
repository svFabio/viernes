#!/usr/bin/env bash
# ==============================================================================
# Viernes Voice Assistant - systemd User Service Installer
# ==============================================================================
set -euo pipefail

SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
SERVICE_NAME="viernes.service"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICE_SOURCE="${REPO_DIR}/${SERVICE_NAME}"
TARGET_PATH="${SYSTEMD_USER_DIR}/${SERVICE_NAME}"

echo "==> Configuring systemd user service for Viernes..."

if [[ ! -f "${SERVICE_SOURCE}" ]]; then
    echo "Error: ${SERVICE_SOURCE} does not exist." >&2
    exit 1
fi

mkdir -p "${SYSTEMD_USER_DIR}"

if [[ -L "${TARGET_PATH}" || -f "${TARGET_PATH}" ]]; then
    echo "==> Removing existing service file at ${TARGET_PATH}..."
    rm -f "${TARGET_PATH}"
fi

echo "==> Creating symlink to ${SERVICE_SOURCE}..."
ln -s "${SERVICE_SOURCE}" "${TARGET_PATH}"

echo "==> Reloading systemd user daemon..."
systemctl --user daemon-reload

echo "==> Viernes service successfully installed!"
echo ""
echo "Manage the service with standard systemctl and journalctl commands:"
echo "  systemctl --user start viernes.service     # Start service immediately"
echo "  systemctl --user stop viernes.service      # Stop service cleanly"
echo "  systemctl --user status viernes.service    # Check service status"
echo "  systemctl --user enable viernes.service    # Enable auto-start on user login"
echo "  systemctl --user disable viernes.service   # Disable auto-start on user login"
echo "  journalctl --user -u viernes.service -f    # Inspect live streaming logs"
