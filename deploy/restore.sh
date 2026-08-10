#!/usr/bin/env bash
# Restores a rustrsssrv SQLite backup (produced by deploy/backup.sh) in place,
# stopping and restarting the systemd service around the swap.
#
# Usage: sudo ./deploy/restore.sh <backup_file> [install_dir]
#   install_dir defaults to /opt/rustrsssrv, falling back to the repo root
set -euo pipefail

BACKUP_FILE="${1:?usage: restore.sh <backup_file> [install_dir]}"
if [[ ! -f "$BACKUP_FILE" ]]; then
    echo "backup file not found: $BACKUP_FILE" >&2
    exit 1
fi

if [[ -f "$BACKUP_FILE.sha256" ]] && ! sha256sum -c "$BACKUP_FILE.sha256" --status; then
    echo "checksum mismatch for $BACKUP_FILE" >&2
    exit 1
fi

sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" | grep -qx ok || {
    echo "backup file failed integrity check: $BACKUP_FILE" >&2
    exit 1
}

INSTALL_DIR="${2:-}"
if [[ -z "$INSTALL_DIR" ]]; then
    if [[ -f /opt/rustrsssrv/.env ]]; then
        INSTALL_DIR=/opt/rustrsssrv
    else
        INSTALL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    fi
fi

ENV_FILE="$INSTALL_DIR/.env"
[[ -f "$ENV_FILE" ]] || { echo "no .env found at $ENV_FILE" >&2; exit 1; }

DB_URL="$(grep -m1 '^DATABASE_URL=' "$ENV_FILE" | cut -d= -f2-)"
DB_PATH="${DB_URL#sqlite:}"
[[ "$DB_PATH" = /* ]] || DB_PATH="$INSTALL_DIR/$DB_PATH"

PORT="$(grep -m1 '^PORT=' "$ENV_FILE" | cut -d= -f2-)"
PORT="${PORT:-9119}"

SERVICE=rustrsssrv
USE_SYSTEMD=0
if systemctl list-unit-files "$SERVICE.service" &>/dev/null; then
    USE_SYSTEMD=1
fi

if [[ "$USE_SYSTEMD" -eq 1 ]]; then
    echo "stopping $SERVICE..."
    systemctl stop "$SERVICE"
else
    echo "no systemd unit for $SERVICE found; make sure the server process is stopped before continuing" >&2
fi

if [[ -f "$DB_PATH" ]]; then
    PRE_RESTORE="$DB_PATH.pre-restore-$(date -u +%Y%m%dT%H%M%SZ)"
    cp "$DB_PATH" "$PRE_RESTORE"
    echo "current database saved as $PRE_RESTORE"
fi

rm -f "$DB_PATH-wal" "$DB_PATH-shm"
cp "$BACKUP_FILE" "$DB_PATH"

if id "$SERVICE" &>/dev/null; then
    chown "$SERVICE:$SERVICE" "$DB_PATH"
fi

if [[ "$USE_SYSTEMD" -eq 1 ]]; then
    echo "starting $SERVICE..."
    systemctl start "$SERVICE"

    for _ in $(seq 1 10); do
        sleep 1
        if curl -fsS "http://127.0.0.1:$PORT/health" &>/dev/null; then
            echo "restore complete; $SERVICE is up and healthy on port $PORT"
            exit 0
        fi
    done

    echo "restore copied but $SERVICE did not report healthy within 10s; check: systemctl status $SERVICE" >&2
    exit 1
fi

echo "restore complete: $DB_PATH restored from $BACKUP_FILE"
echo "restart the server process manually"
