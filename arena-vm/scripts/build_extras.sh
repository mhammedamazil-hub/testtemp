#!/usr/bin/env bash
# Optional extras for the Blender (bpy) wheel: bison, libxkbcommon, glvnd.
# Kept separate from build_stack.sh because bison needs a VCS bootstrap.
set -euo pipefail
source "$(dirname "$0")/env.sh"
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"
stage() { local name="$1"; shift; [ -f "$MARK/$name" ] && { log "SKIP $name"; return 0; }; log "STAGE $name"; "$@" && touch "$MARK/$name" || fail "stage $name failed"; }

# bison VCS tree ships empty gnulib submodule; fill it from a GitHub mirror
s_gnulib() {
  [ -d "$VM_SRC/gnulib/.git" ] || git clone --depth 1 \
    https://github.com/external-mirrors/gnulib.git "$VM_SRC/gnulib"
  [ -x "$VM_SRC/gnulib/gnulib-tool" ]
}
s_bison_bootstrap() {
  pin_timestamps "$VM_SRC/bison"
  ( cd "$VM_SRC/bison" && \
    GNULIB_SRCDIR="$VM_SRC/gnulib" ./bootstrap --no-git --gnulib-srcdir="$VM_SRC/gnulib" \
      --skip-po )
  pin_timestamps "$VM_SRC/bison"
  rm -rf "$VM_BUILD/b-bison"; mkdir -p "$VM_BUILD/b-bison"
  ( cd "$VM_BUILD/b-bison" && \
    "$VM_SRC/bison/configure" --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-nls && make -j"$(nproc)" && make install )
}
s_xkbcommon() {
  meson_build "$VM_SRC/libxkbcommon" -Denable-wayland=false -Denable-x11=true \
    -Denable-docs=false
}
s_glvnd() {
  meson_build "$VM_SRC/libglvnd" -Dx11=enabled -Degl=false -Dglx=disabled \
    -Dgles1=disabled -Dgles2=disabled -Dheaders=true
}

case "${1:-all}" in
  all) stage gnulib s_gnulib; stage bison s_bison_bootstrap
       stage xkbcommon s_xkbcommon; stage glvnd s_glvnd ;;
  *) stage "$1" "s_$1" ;;
esac
log "build_extras finished: $1"
