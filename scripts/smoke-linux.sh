#!/usr/bin/env bash
set -euo pipefail
binary=$(realpath "${1:?Usage: smoke-linux.sh path/to/nebulabook}")
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output=${NEBULABOOK_SMOKE_OUTPUT:-"$root/smoke-output"}
mkdir -p "$output"
output=$(realpath "$output")
# Each backend gets an isolated user D-Bus session and application data directory.
dbus-run-session -- xvfb-run -a -s '-screen 0 1280x900x24 -nolisten tcp' \
  python3 "$root/scripts/smoke-linux.py" "$binary" x11 "$output"
dbus-run-session -- python3 "$root/scripts/smoke-linux.py" "$binary" wayland "$output"
