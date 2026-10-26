#!/usr/bin/env bash
# Expose the live desktop to a tablet browser:
#   x11vnc (loopback only, shared) -> websockify + noVNC (0.0.0.0, HTTP Basic Auth)
# The AI keeps full control via XTEST on the same X display; VNC is view+input
# shared, so tablet input and AI input coexist on one desktop.
set -euo pipefail
source "$(dirname "$0")/env.sh"
RUN="$VM_ROOT/run"; mkdir -p "$RUN"
DISP="${1:-:99}"
PORT="${2:-6080}"
export DISPLAY="$DISP"

# ---------- 1. VNC server attached to the live X display ----------
if ! pgrep -f "x11vnc.*$DISP" >/dev/null; then
  log "starting x11vnc on 127.0.0.1:5900 (shared, loopback-only)"
  setsid "$VM_PREFIX/bin/x11vnc" -display "$DISP" -rfbport 5900 -localhost \
    -allow 127.0.0.1 -forever -shared -nopw -quiet -bg -o "$RUN/x11vnc.log"
fi

# ---------- 2. credentials (generated once, never committed) ----------
CRED="$RUN/tablet.cred"
if [ ! -f "$CRED" ]; then
  USER="arena"
  PASS="$(head -c9 /dev/urandom | base64 | tr -d '/+=' | cut -c1-12)"
  echo "$USER:$PASS" > "$CRED"
  chmod 600 "$CRED"
fi
AUTH="$(cat "$CRED")"

# ---------- 3. noVNC web socket bridge with Basic Auth ----------
pkill -f "websockify.*$PORT" 2>/dev/null || true
sleep 0.3
log "starting noVNC bridge on 0.0.0.0:$PORT (auth required)"
cd "$VM_PREFIX/share/novnc"
# landing page: straight into the noVNC client
printf '<!doctype html><meta http-equiv="refresh" content="0;url=vnc.html?autoconnect=1&resize=scale">\n' \
  > "$VM_PREFIX/share/novnc/index.html"

exec "$VM_VENV/bin/websockify" \
  --web "$VM_PREFIX/share/novnc" --web-auth \
  --auth-plugin=websockify.auth_plugins.BasicHTTPAuth \
  --auth-source="$AUTH" \
  "0.0.0.0:$PORT" 127.0.0.1:5900
