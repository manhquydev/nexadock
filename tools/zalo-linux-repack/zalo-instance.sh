#!/usr/bin/env bash
# zalo-instance.sh — launch one isolated Zalo desktop instance on Linux.
# Usage: zalo-instance.sh <instance-name> [extra electron args...]
# Each instance gets its own ZALO_DATA_DIR (= its own userData + its own
# Electron single-instance lock), so multiple accounts run in parallel.
set -euo pipefail

NAME="$(basename "${1:-default}")"; shift || true
if [ "$NAME" = "." ] || [ "$NAME" = ".." ]; then
  echo "FATAL: invalid instance name" >&2; exit 1
fi
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$HERE/app"
ELECTRON="$HERE/electron/electron"
test -d "$APP_DIR" || { echo "FATAL: app tree missing at $APP_DIR — run rebuild-zalo-linux.sh" >&2; exit 1; }

if [ ! -x "$ELECTRON" ]; then
  echo "FATAL: bundled runtime missing at $ELECTRON — run rebuild-zalo-linux.sh first (never auto-download a different Electron)" >&2
  exit 1
fi
DATA_ROOT="${ZALO_DATA_ROOT:-$HOME/.local/share/zalo-linux}"
DATA_DIR="$DATA_ROOT/$NAME"
mkdir -p "$DATA_DIR"

export ZALO_DATA_DIR="$DATA_DIR"
exec "$ELECTRON" "$APP_DIR" --no-sandbox "$@"
