#!/usr/bin/env bash
# Takes a WAL-safe snapshot of the rustrsssrv SQLite database via `sqlite3 .backup`
# (safe to run against a live, running server — unlike `cp`, which can miss
# data still sitting in the -wal file).
#
# Usage: ./deploy/backup.sh [install_dir] [backup_dir]
#   install_dir defaults to /opt/rustrsssrv, falling back to the repo root
#   backup_dir  defaults to <install_dir>/backups
set -euo pipefail

INSTALL_DIR="${1:-}"
if [[ -z "$INSTALL_DIR" ]]; then
    if [[ -f /opt/rustrsssrv/.env ]]; then
        INSTALL_DIR=/opt/rustrsssrv
    else
        INSTALL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    fi
fi

ENV_FILE="$INSTALL_DIR/.env"
if [[ ! -f "$ENV_FILE" ]]; then
    echo "no .env found at $ENV_FILE" >&2
    exit 1
fi

DB_URL="$(grep -m1 '^DATABASE_URL=' "$ENV_FILE" | cut -d= -f2-)"
DB_PATH="${DB_URL#sqlite:}"
[[ "$DB_PATH" = /* ]] || DB_PATH="$INSTALL_DIR/$DB_PATH"

if [[ ! -f "$DB_PATH" ]]; then
    echo "database not found at $DB_PATH" >&2
    exit 1
fi

BACKUP_DIR="${2:-$INSTALL_DIR/backups}"
mkdir -p "$BACKUP_DIR"

TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$BACKUP_DIR/rustrsssrv-$TIMESTAMP.db"

sqlite3 "$DB_PATH" ".backup '$OUT'"
sqlite3 "$OUT" "PRAGMA integrity_check;" | grep -qx ok || {
    echo "backup integrity check failed: $OUT" >&2
    exit 1
}

sha256sum "$OUT" > "$OUT.sha256"
echo "backup written: $OUT"
echo "checksum:        $OUT.sha256"
