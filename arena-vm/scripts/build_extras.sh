#!/usr/bin/env bash
# Optional extras for the Blender (bpy) wheel: bison, libxkbcommon, glvnd.
# Kept separate from build_stack.sh so a toolchain gap can never block the
# desktop build. bison comes from a release-tarball mirror: VCS snapshots
# require flex plus a full gnulib/gettext bootstrap (see docs/ dead-end notes).
set -euo pipefail
source "$(dirname "$0")/env.sh"
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"
stage() { local name="$1"; shift; [ -f "$MARK/$name" ] && { log "SKIP $name"; return 0; }; log "STAGE $name"; "$@" && touch "$MARK/$name" || fail "stage $name failed"; }

s_bison() {
  # GNU bison 3.0.4 release mirror. Release layout ships pregenerated
  # scan-*.c / parse-gram.c, configure and Makefile.in: no flex or gnulib
  # bootstrap needed. See docs/bison-source-notes.md for the rejected VCS path.
  [ -d "$VM_SRC/bison-rel" ] || git clone --depth 1 \
    https://github.com/tar-mirror/gnu-bison.git "$VM_SRC/bison-rel"
  # git clones flatten mtimes; stop make regenerating aclocal.m4 with the
  # automake-1.15 the tarball was built against (toolchain provides 1.17)
  touch "$VM_SRC/bison-rel/aclocal.m4" "$VM_SRC/bison-rel/configure" \
        $(find "$VM_SRC/bison-rel" -name 'Makefile.in')
  # modern glibc (>=2.32) hides _IO_ftrylockfile and sets __GNU_LIBRARY__!=1;
  # the bundled gnulib fseterr.c then hits its #error portability trap
  apply_patch "$VM_SRC/bison-rel" bison-fseterr-glibc
  rm -rf "$VM_BUILD/bison-rel"; mkdir -p "$VM_BUILD/bison-rel"
  ( cd "$VM_BUILD/bison-rel" && \
    "$VM_SRC/bison-rel/configure" --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-nls && make -j"$(nproc)" && make install )
  bison --version | head -1
}
s_xkbcommon() {
  # This tree generates src/xkbcomp/parser.c at build time and hard-requires
  # bison >= 3.6 in the version check; our 3.0.4 accepts the grammar, so drop
  # only the version constraint (parser generation is still executed).
  if ! grep -q "required: true)$" "$VM_SRC/libxkbcommon/meson.build"; then
    sed -i "s/find_program('bison', 'win_bison', required: true, version: '>= 3.6')/find_program('bison', 'win_bison', required: true)/" \
      "$VM_SRC/libxkbcommon/meson.build"
  fi
  # bison 3.0 knows parse.error 'verbose'; 'detailed' is its 3.6+ rename
  # (verbose remains accepted as an alias, so the patch is forward-compatible)
  sed -i 's/%define parse.error detailed/%define parse.error verbose/' \
    "$VM_SRC/libxkbcommon/src/xkbcomp/parser.y"
  # libxkbregistry needs libxml-2.0, which is not part of this toolchain
  meson_build "$VM_SRC/libxkbcommon" -Denable-wayland=false -Denable-x11=true \
    -Denable-docs=false -Denable-xkbregistry=false
}
s_glvnd() {
  # egl/gles1/gles2 are boolean options, x11/glx are feature options
  meson_build "$VM_SRC/libglvnd" -Dx11=enabled -Degl=false -Dglx=enabled \
    -Dgles1=false -Dgles2=false -Dheaders=true
}

case "${1:-all}" in
  all) stage bison s_bison
       stage xkbcommon s_xkbcommon; stage glvnd s_glvnd ;;
  *) stage "$1" "s_$1" ;;
esac
log "build_extras finished: $1"
