#!/usr/bin/env bash
# Master recovery entry point. Rebuilds the entire virtual desktop from the
# pinned sources after a sandbox reset. Resumable: every stage is marker-gated.
#
# Usage:
#   recover.sh            full build + start desktop (no tablet bridge)
#   recover.sh --tablet   also start the noVNC tablet bridge
#   recover.sh --blender  also install the ~520MB bpy wheel
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
chmod +x "$HERE"/*.sh 2>/dev/null || true
source "$HERE/env.sh"
WANT_TABLET=0; WANT_BLENDER=0
for a in "$@"; do case "$a" in
  --tablet) WANT_TABLET=1;; --blender) WANT_BLENDER=1;; esac; done

log "=== 1/6 python toolchain (venv) ==="
if [ ! -x "$VM_VENV/bin/meson" ]; then
  python3 -m venv "$VM_VENV"
  "$VM_VENV/bin/pip" install --upgrade pip
  "$VM_VENV/bin/pip" install -r "$VM_ROOT/requirements.txt"
fi

log "=== 2/6 source checkouts (pinned in pins.env) ==="
bash "$HERE/fetch.sh"

log "=== 3/6 dynamic linker registration ==="
echo "$VM_PREFIX/lib" | sudo tee /etc/ld.so.conf.d/arena-vm.conf >/dev/null
sudo /sbin/ldconfig

log "=== 4/6 X11 stack + Xvfb ==="
bash "$HERE/build_stack.sh" all

log "=== 5/6 window manager, desktop and applications ==="
bash "$HERE/build_suckless.sh" all
# Openbox is the required WM (dwm above stays available as a fallback);
# this also builds its glib/pango/libxml2/gettext chain.
bash "$HERE/build_openbox.sh" all
bash "$HERE/build_apps.sh" all
if [ "$WANT_BLENDER" = 1 ]; then
  # bison/libxkbcommon/libglvnd must exist before the bpy wheel can dlopen
  bash "$HERE/build_extras.sh" all
  bash "$HERE/install_blender.sh"
fi

log "=== 6/6 start services ==="
bash "$HERE/start_desktop.sh" :99 1280x800x24
if [ "$WANT_TABLET" = 1 ]; then
  setsid bash "$HERE/start_tablet.sh" :99 6080 >"$VM_ROOT/run/tablet.log" 2>&1 &
  log "tablet bridge starting on port 6080 (credentials: run/tablet.cred)"
fi
log "recovery complete. Run tests/acceptance.sh to validate."
