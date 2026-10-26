#!/usr/bin/env bash
# Openbox WM and its glib/pango/libxml2 dependency chain.
# Openbox is the explicit lightweight-WM requirement (dwm is interim).
# Stages are marker-gated; safe to resume after a failure.
set -euo pipefail
source "$(dirname "$0")/env.sh"
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"
stage() { local name="$1"; shift; [ -f "$MARK/$name" ] && { log "SKIP $name"; return 0; }; log "STAGE $name"; "$@" && touch "$MARK/$name" || fail "stage $name failed"; }

# 1. libffi (VCS tag, no generated configure) — autotools
s_libffi() {
  ( cd "$VM_SRC/libffi" && autoreconf -fi )
  rm -rf "$VM_BUILD/b-libffi"; mkdir -p "$VM_BUILD/b-libffi"
  ( cd "$VM_BUILD/b-libffi" && "$VM_SRC/libffi/configure" \
      --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-docs && make -j"$(nproc)" && make install )
}

# 2. pcre2 — cmake, static+shared, no tools/tests needed beyond the lib
s_pcre2() {
  cmake_build "$VM_SRC/pcre2" \
    -DBUILD_SHARED_LIBS=ON -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DPCRE2_BUILD_PCRE2GREP=OFF -DPCRE2_BUILD_TESTS=OFF \
    -DPCRE2_SUPPORT_JIT=ON
}

# 3. glib 2.80 LTS — meson, everything optional stripped out
s_glib() {
  meson_build "$VM_SRC/glib" \
    -Dnls=disabled -Dtests=false -Dintrospection=disabled \
    -Dlibmount=disabled -Dselinux=disabled -Dsysprof=disabled \
    -Dlibelf=disabled -Ddtrace=false -Dsystemtap=false \
    -Dglib_debug=disabled -Dmultiarch=false
}

# 4. fribidi — meson, lib only
s_fribidi() {
  meson_build "$VM_SRC/fribidi" -Ddocs=false -Dtests=false -Dbin=false
}

# 5. pango with Xft backend; cairo/introspection dropped (openbox uses Xft)
s_pango() {
  # pkgconf 3.x matches module names case-sensitively; pango asks for 'xft'
  [ -e "$VM_PREFIX/lib/pkgconfig/xft.pc" ] || \
    ln -s Xft.pc "$VM_PREFIX/lib/pkgconfig/xft.pc"
  meson_build "$VM_SRC/pango" \
    -Dcairo=disabled -Dfontconfig=enabled -Dfreetype=enabled \
    -Dxft=enabled -Dintrospection=disabled \
    -Ddocumentation=false -Dgtk_doc=false \
    -Dbuild-testsuite=false -Dbuild-examples=false -Dsysprof=disabled
}

# 6. libxml2 — meson, parser library only
s_libxml2() {
  meson_build "$VM_SRC/libxml2" \
    -Dpython=false -Dicu=disabled -Dlegacy=true -Dlzma=disabled
}

# 7. gettext-runtime (NLS programs + aclocal m4) and autopoint. This tree
#    keeps autopoint in gettext-tools/misc (not installed by runtime); the
#    script is a pure shell template needing only path substitutions, and
#    archive.dir.tar is the prebuilt infrastructure archive it extracts.
s_gettext() {
  pin_timestamps "$VM_SRC/gettext/gettext-runtime"
  rm -rf "$VM_BUILD/b-gettext"; mkdir -p "$VM_BUILD/b-gettext"
  ( cd "$VM_BUILD/b-gettext" && "$VM_SRC/gettext/gettext-runtime/configure" \
      --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-csharp --disable-java && make -j"$(nproc)" && make install )
  local misc="$VM_SRC/gettext/gettext-tools/misc"
  local ver; ver=$("$VM_PREFIX/bin/gettext" --version | head -1 | awk '{print $NF}')
  sed -e "s|@prefix@|$VM_PREFIX|g" \
      -e "s|@exec_prefix@|$VM_PREFIX|g" \
      -e "s|@datarootdir@|$VM_PREFIX/share|g" \
      -e "s|@datadir@|$VM_PREFIX/share|g" \
      -e "s|@bindir@|$VM_PREFIX/bin|g" \
      -e "s|@PACKAGE@|gettext|g" \
      -e "s|@VERSION@|$ver|g" \
      -e "s|@ARCHIVE_VERSION@|$ver|g" \
      -e "s|@ARCHIVE_FORMAT@|dirgz|g" \
      -e "s|@PATH_SEPARATOR@|:|g" \
      -e "s|@RELOCATABLE@|0|g" \
      "$misc/autopoint.in" > "$VM_PREFIX/bin/autopoint"
  [ -s "$VM_PREFIX/bin/autopoint" ] || fail "autopoint substitution produced an empty script"
  chmod +x "$VM_PREFIX/bin/autopoint"
  mkdir -p "$VM_PREFIX/share/gettext"
  gzip -c "$misc/archive.dir.tar" > "$VM_PREFIX/share/gettext/archive.dir.tar.gz"
  autopoint --version | head -1
}

# 8. openbox itself: bootstrap (autopoint/libtoolize/autotools), then configure
s_openbox() {
  pin_timestamps "$VM_SRC/openbox"
  ( cd "$VM_SRC/openbox" && ./bootstrap )
  pin_timestamps "$VM_SRC/openbox"
  rm -rf "$VM_BUILD/b-openbox"; mkdir -p "$VM_BUILD/b-openbox"
  ( cd "$VM_BUILD/b-openbox" && "$VM_SRC/openbox/configure" \
      --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-nls --disable-startup-notification \
      --disable-imlib2 --disable-librsvg \
      --disable-gtk-doc && make -j"$(nproc)" && make install )
  openbox --version | head -1
}

case "${1:-all}" in
  all) stage libffi  s_libffi
       stage pcre2   s_pcre2
       stage glib    s_glib
       stage fribidi s_fribidi
       stage pango   s_pango
       stage libxml2 s_libxml2
       stage gettext s_gettext
       stage openbox s_openbox ;;
  *) stage "$1" "s_$1" ;;
esac
log "build_openbox finished: $1"
