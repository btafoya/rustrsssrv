#!/usr/bin/env bash
# Installs rustrsssrv as a systemd service on Ubuntu 22.04.
# Run as root: sudo ./deploy/install.sh
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "must be run as root (sudo ./deploy/install.sh)" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
BINARY="$REPO_ROOT/target/release/rustrsssrv"
INSTALL_DIR=/opt/rustrsssrv
SERVICE_NAME=rustrsssrv

if [[ ! -x "$BINARY" ]]; then
    echo "release binary not found at $BINARY" >&2
    echo "build it first: npm run build:all" >&2
    exit 1
fi

if ! id "$SERVICE_NAME" &>/dev/null; then
    useradd --system --home-dir "$INSTALL_DIR" --shell /usr/sbin/nologin "$SERVICE_NAME"
fi

mkdir -p "$INSTALL_DIR/data" "$INSTALL_DIR/logs"
install -m 755 "$BINARY" "$INSTALL_DIR/rustrsssrv"
install -m 755 "$SCRIPT_DIR/backup.sh" "$INSTALL_DIR/backup.sh"
install -m 755 "$SCRIPT_DIR/restore.sh" "$INSTALL_DIR/restore.sh"

ENV_FILE="$INSTALL_DIR/.env"
if [[ ! -f "$ENV_FILE" ]]; then
    JWT_SECRET="$(openssl rand -hex 32)"
    cat > "$ENV_FILE" <<EOF
DATABASE_URL=sqlite:./data/rustrsssrv.db
JWT_SECRET=$JWT_SECRET
PORT=9119
ENABLE_CRAWLER=true
LOG_DIR=./logs
RUST_LOG=info
EOF
    chmod 600 "$ENV_FILE"
    echo "generated $ENV_FILE with a random JWT_SECRET"
fi

chown -R "$SERVICE_NAME:$SERVICE_NAME" "$INSTALL_DIR"

install -m 644 "$SCRIPT_DIR/rustrsssrv.service" /etc/systemd/system/rustrsssrv.service
systemctl daemon-reload
systemctl enable rustrsssrv
systemctl restart rustrsssrv

echo "rustrsssrv installed and started. Check status with: systemctl status rustrsssrv"
