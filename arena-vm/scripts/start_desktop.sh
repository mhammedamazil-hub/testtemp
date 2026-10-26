#!/usr/bin/env bash
# Start (or restart) the virtual desktop: Xvfb + dwm.
# Usage: start_desktop.sh [display] [WxHxD]
set -euo pipefail
source "$(dirname "$0")/env.sh"

DISP="${1:-:99}"
GEOM="${2:-1280x800x24}"
RUN="$VM_ROOT/run"; mkdir -p "$RUN"
export DISPLAY="$DISP"
export LD_LIBRARY_PATH="$VM_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export FONTCONFIG_PATH="$VM_PREFIX/etc/fonts"
export XAUTHORITY="$RUN/.Xauthority"
export TERM=xterm-256color
export HOME="${HOME:-/home/user}"

"$VM_ROOT/scripts/stop_desktop.sh" "$DISP" 2>/dev/null || true

log "starting Xvfb on $DISP ($GEOM)"
setsid nohup "$VM_PREFIX/bin/Xvfb" "$DISP" -screen 0 "$GEOM" -ac -nolisten tcp \
  -fp "$VM_PREFIX/share/fonts/X11/misc" >"$RUN/Xvfb.log" 2>&1 &
echo $! > "$RUN/Xvfb.pid"

# wait for the X socket
SOCK="/tmp/.X11-unix/X${DISP#:}"
for i in $(seq 1 50); do [ -S "$SOCK" ] && break; sleep 0.1; done
[ -S "$SOCK" ] || fail "Xvfb socket never appeared"

# keyboard layout (best effort)
"$VM_PREFIX/bin/setxkbmap" -layout us 2>/dev/null || true

log "starting dwm"
setsid nohup "$VM_PREFIX/bin/dwm" >"$RUN/dwm.log" 2>&1 &
echo $! > "$RUN/dwm.pid"
sleep 1

log "desktop ready on DISPLAY=$DISP"
"$VM_PREFIX/bin/xdpyinfo" | grep -E "X.Org version|dimensions" | sed 's/^/  /'
