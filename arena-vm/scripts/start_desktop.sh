#!/usr/bin/env bash
# Start (or restart) the virtual desktop: Xvfb + window manager.
# Openbox is the primary WM (explicit requirement); dwm remains as fallback.
# Usage: start_desktop.sh [display] [WxHxD] [wm]
set -euo pipefail
source "$(dirname "$0")/env.sh"

DISP="${1:-:99}"
GEOM="${2:-1280x800x24}"
WM="${3:-${VM_WM:-openbox}}"
RUN="$VM_ROOT/run"; mkdir -p "$RUN"
export DISPLAY="$DISP"
export LD_LIBRARY_PATH="$VM_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export FONTCONFIG_PATH="$VM_PREFIX/etc/fonts"
export XAUTHORITY="$RUN/.Xauthority"
export TERM=xterm-256color
export HOME="${HOME:-/home/user}"
# glib/pango/openbox look up XDG config under this prefix
export XDG_CONFIG_DIRS="$VM_PREFIX/etc/xdg${XDG_CONFIG_DIRS:+:$XDG_CONFIG_DIRS}"
export XDG_DATA_DIRS="$VM_PREFIX/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"

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

case "$WM" in
  openbox)
    if [ ! -x "$VM_PREFIX/bin/openbox" ]; then
      log "openbox not installed; falling back to dwm"; WM=dwm
    fi ;;
esac

log "starting window manager: $WM"
case "$WM" in
  openbox)
    setsid nohup "$VM_PREFIX/bin/openbox" >"$RUN/openbox.log" 2>&1 &
    echo $! > "$RUN/openbox.pid"; cp "$RUN/openbox.pid" "$RUN/wm.pid" ;;
  dwm)
    setsid nohup "$VM_PREFIX/bin/dwm" >"$RUN/dwm.log" 2>&1 &
    echo $! > "$RUN/dwm.pid"; cp "$RUN/dwm.pid" "$RUN/wm.pid" ;;
  *) fail "unknown WM: $WM (use openbox|dwm)" ;;
esac
sleep 1

log "desktop ready on DISPLAY=$DISP (WM=$WM)"
"$VM_PREFIX/bin/xdpyinfo" | grep -E "X.Org version|dimensions" | sed 's/^/  /'
