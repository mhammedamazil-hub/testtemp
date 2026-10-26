#!/usr/bin/env bash
# Master recovery entry point. Rebuilds the entire virtual desktop from the
# pinned sources after a sandbox reset. Resumable: every stage is marker-gated.
#
# Usage:
#   recover.sh            full build + start desktop (no tablet bridge)
#   recover.sh --tablet   also start the noVNC tablet bridge
#   recover.sh --blender  also install the ~520MB bpy wheel
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
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
"$HERE/fetch.sh"

log "=== 3/6 dynamic linker registration ==="
echo "$VM_PREFIX/lib" | sudo tee /etc/ld.so.conf.d/arena-vm.conf >/dev/null
sudo /sbin/ldconfig

log "=== 4/6 X11 stack + Xvfb ==="
"$HERE/build_stack.sh" all

log "=== 5/6 desktop (dwm/st/dmenu/fonts) and applications ==="
"$HERE/build_suckless.sh" all
"$HERE/build_apps.sh" all
[ "$WANT_BLENDER" = 1 ] && "$HERE/install_blender.sh"

log "=== 6/6 start services ==="
"$HERE/start_desktop.sh" :99 1280x800x24
if [ "$WANT_TABLET" = 1 ]; then
  setsid "$HERE/start_tablet.sh" :99 6080 >"$VM_ROOT/run/tablet.log" 2>&1 &
  log "tablet bridge starting on port 6080 (credentials: run/tablet.cred)"
fi
log "recovery complete. Run tests/acceptance.sh to validate."
