#!/usr/bin/env bash
set -euo pipefail
binary=$(realpath "${1:?Usage: smoke-linux.sh path/to/nebulabook}")
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output=${NEBULABOOK_SMOKE_OUTPUT:-"$root/smoke-output"}
mkdir -p "$output"
output=$(realpath "$output")
# Invalidate prior success before even the fixture checks; failed reruns must
# never leave a reusable package-gate success for the same executable.
rm -f -- "$output/x11-result.json" "$output/wayland-result.json" "$output/x11-screenshots.json"
# Check deterministic fixtures and the independent test-only .nebula reader first.
python3 "$root/scripts/smoke_fixtures.py"
# Each backend gets an isolated user D-Bus session and application data directory.
# The larger Xvfb screen accommodates a real 2000×1400 client at 2× scaling.
dbus-run-session -- xvfb-run -a -s '-screen 0 2560x1800x24 -dpi 96 -nolisten tcp' \
  python3 "$root/scripts/smoke-linux.py" "$binary" x11 "$output"
dbus-run-session -- python3 "$root/scripts/smoke-linux.py" "$binary" wayland "$output"
