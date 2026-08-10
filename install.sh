#!/usr/bin/env bash
# Installs rustrsssrv as a systemd service on Ubuntu 22.04 without checking out the repo.
# Downloads the latest release from GitHub.
#
# Usage: curl -fsSL https://raw.githubusercontent.com/btafoya/rustrsssrv/main/install.sh | sudo bash
set -euo pipefail

REPO=btafoya/rustrsssrv
INSTALL_DIR=/opt/rustrsssrv
TARBALL=rustrsssrv-x86_64-linux.tar.gz

if [[ $EUID -ne 0 ]]; then
    echo "must be run as root: curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | sudo bash" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL -o "$TMP/$TARBALL" "https://github.com/$REPO/releases/latest/download/$TARBALL"
curl -fsSL -o "$TMP/SHA256SUMS" "https://github.com/$REPO/releases/latest/download/SHA256SUMS"
(cd "$TMP" && grep " $TARBALL\$" SHA256SUMS | sha256sum -c -)
tar xzf "$TMP/$TARBALL" -C "$TMP"

if ! id rustrsssrv &>/dev/null; then
    useradd --system --home-dir "$INSTALL_DIR" --shell /usr/sbin/nologin rustrsssrv
fi

mkdir -p "$INSTALL_DIR/data" "$INSTALL_DIR/logs"
install -m 755 "$TMP/rustrsssrv" "$INSTALL_DIR/rustrsssrv"

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

chown -R rustrsssrv:rustrsssrv "$INSTALL_DIR"

install -m 644 "$TMP/rustrsssrv.service" /etc/systemd/system/rustrsssrv.service
systemctl daemon-reload
systemctl enable --now rustrsssrv

echo "rustrsssrv installed and started. Check status with: systemctl status rustrsssrv"
