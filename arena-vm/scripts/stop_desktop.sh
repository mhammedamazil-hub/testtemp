#!/usr/bin/env bash
# Stop Xvfb/dwm for a display. Usage: stop_desktop.sh [:99]
set -uo pipefail
source "$(dirname "$0")/env.sh"
DISP="${1:-:99}"
RUN="$VM_ROOT/run"
for p in dwm Xvfb; do
  [ -f "$RUN/$p.pid" ] && kill "$(cat "$RUN/$p.pid")" 2>/dev/null || true
done
pkill -f "bin/Xvfb $DISP" 2>/dev/null || true
pkill -f "bin/dwm" 2>/dev/null || true
sleep 0.5
echo "stopped ($DISP)"
