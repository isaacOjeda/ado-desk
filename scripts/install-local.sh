#!/usr/bin/env bash
# Build a release of ADO Desk and install it in /Applications (macOS only).
# Usage: scripts/install-local.sh [--no-open]
set -euo pipefail

APP_NAME="ADO Desk"
DEST_DIR="/Applications"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only supports macOS." >&2
  exit 1
fi

open_after=true
if [[ "${1:-}" == "--no-open" ]]; then
  open_after=false
elif [[ $# -gt 0 ]]; then
  echo "Usage: scripts/install-local.sh [--no-open]" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> Installing dependencies"
pnpm install --frozen-lockfile

echo "==> Building release bundle (first build takes several minutes)"
pnpm tauri build --bundles app

built_app="src-tauri/target/release/bundle/macos/${APP_NAME}.app"
if [[ ! -d "$built_app" ]]; then
  echo "Build finished but ${built_app} was not found." >&2
  exit 1
fi

BUNDLE_ID="com.isaacojeda.adodesk"
is_running() {
  pgrep -x "ado-desk" >/dev/null || pgrep -f "/${APP_NAME}.app/Contents/MacOS/" >/dev/null
}

if is_running; then
  echo "==> Quitting the running ADO Desk"
  osascript -e "if application id \"${BUNDLE_ID}\" is running then tell application id \"${BUNDLE_ID}\" to quit" >/dev/null 2>&1 || true
  for _ in {1..20}; do
    is_running || break
    sleep 0.5
  done
  if is_running; then
    echo "ADO Desk is still running (maybe 'pnpm tauri dev'). Close it and run this script again." >&2
    exit 1
  fi
fi

target="${DEST_DIR}/${APP_NAME}.app"
echo "==> Installing to ${target}"
rm -rf "$target"
ditto "$built_app" "$target"

if $open_after; then
  echo "==> Opening ${APP_NAME}"
  open "$target"
fi

echo "Done. On first launch, choose 'Always Allow' when macOS asks for keychain access."
