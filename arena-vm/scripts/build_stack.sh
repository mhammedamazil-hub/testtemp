#!/usr/bin/env bash
# Resumable full-stack build. Each stage writes a marker on success so re-runs
# continue where they left off. Logs: evidence/build_stack.log
set -uo pipefail
source "$(dirname "$0")/env.sh"
LOG="$VM_ROOT/evidence/build_stack.log"
mkdir -p "$(dirname "$LOG")"
exec > >(tee -a "$LOG") 2>&1
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"

stage() { # stage name ; runs remaining args as a command
  local name="$1"; shift
  if [ -f "$MARK/$name" ]; then log "SKIP $name (marker)"; return 0; fi
  log "STAGE $name"
  if "$@"; then touch "$MARK/$name"; log "DONE  $name"; else fail "stage $name failed"; fi
}

# gettext is unavailable; install no-op shims so NLS make rules still pass
s_shims() {
  mkdir -p "$VM_PREFIX/bin"
  install -m755 "$VM_ROOT/scripts/shims/xgettext" "$VM_PREFIX/bin/xgettext"
  install -m755 "$VM_ROOT/scripts/shims/msgfmt"   "$VM_PREFIX/bin/msgfmt"
  ln -sf xgettext "$VM_PREFIX/bin/msgmerge"
  ln -sf xgettext "$VM_PREFIX/bin/gmsgfmt"
  install -m755 "$VM_ROOT/scripts/shims/makeinfo" "$VM_PREFIX/bin/makeinfo"
  install -m755 "$VM_ROOT/scripts/shims/help2man" "$VM_PREFIX/bin/help2man"
  ln -sf makeinfo "$VM_PREFIX/bin/texi2any"
}

# ---------- S1-S4 base libraries ----------
s_zlib() {
  ( cd "$VM_SRC/zlib" && ./configure --prefix="$VM_PREFIX" --shared && make -j"$(nproc)" && make install )
}
s_libpng() { cmake_build "$VM_SRC/libpng1.6" -DCMAKE_PREFIX_PATH="$VM_PREFIX" -DPNG_TESTS=OFF -DPNG_SHARED=ON -DPNG_STATIC=OFF; }
s_expat() { cmake_build "$VM_SRC/expat/expat" -DEXPAT_BUILD_TESTS=OFF -DEXPAT_BUILD_DOCS=OFF -DEXPAT_SHARED_LIBS=ON; }
s_freetype() { meson_build "$VM_SRC/freetype" -Dpng=disabled -Dharfbuzz=disabled -Dbzip2=disabled -Dbrotli=disabled -Dtests=disabled; }
s_harfbuzz() { meson_build "$VM_SRC/harfbuzz" -Dglib=disabled -Dgobject=disabled -Dicu=disabled \
  -Dcairo=disabled -Dchafa=disabled -Dtests=disabled -Ddocs=disabled -Dintrospection=disabled \
  -Dgraphite=disabled -Dgraphite2=disabled -Dfontations=disabled -Dutilities=disabled \
  -Dbenchmark=disabled -Dfreetype=enabled; }

# ---------- S5-S10 X protocol core ----------
s_xorgproto() { meson_build "$VM_SRC/xorgproto"; }
s_xtrans()   { conf_build "$VM_SRC/xtrans"; }
s_libxau()   { conf_build "$VM_SRC/libxau"; }
s_libxdmcp() { conf_build "$VM_SRC/libxdmcp"; }
s_xcbproto() {
  conf_build "$VM_SRC/xcb-proto"
  # glibc >=2.34 merged libpthread: ship an empty pthread-stubs.pc
  cat > "$VM_PREFIX/lib/pkgconfig/pthread-stubs.pc" <<EOF
prefix=$VM_PREFIX
exec_prefix=\${prefix}
libdir=\${exec_prefix}/lib
includedir=\${prefix}/include
Name: pthread-stubs
Description: pthread stubs (unused on modern glibc)
Version: 0.4
Libs: -pthread
EOF
}
s_libxcb() { conf_build "$VM_SRC/libxcb" --enable-xinput --enable-xkb; }
s_libx11() { conf_build "$VM_SRC/libx11" --disable-silent-rules; }

# ---------- S11 extension libraries ----------
s_xmacros() {
  # util-macros is normally m4 data; dist configure trees only need its .pc file
  cat > "$VM_PREFIX/lib/pkgconfig/xorg-macros.pc" <<EOF
prefix=$VM_PREFIX
datarootdir=\${prefix}/share
pkgdatadir=\${datarootdir}/xorg/util-macros
Name: xorg-macros
Description: X.Org autotools rebuild infrastructure (stub, version 1.20.2)
Version: 1.20.2
EOF
}
s_extlibs() {
  for d in libxext libxrender libxfixes libxi libxrandr libxinerama libxcursor \
           libxcomposite libxdamage libxtst libxss libice libsm libxt libxmu \
           libxpm libxv libxres; do
    [ -f "$VM_ROOT/scripts/patches/$d.patch" ] && apply_patch "$VM_SRC/$d" "$d"
    conf_build "$VM_SRC/$d"
  done
}

# ---------- S12-S17 server font/kbd stack ----------
s_gperf() {
  # gperf's top-level make wants makeinfo (absent); install lib + binary only
  pin_timestamps "$VM_SRC/gperf"
  rm -rf "$VM_BUILD/b-gperf"; mkdir -p "$VM_BUILD/b-gperf"
  ( cd "$VM_BUILD/b-gperf" && "$VM_SRC/gperf/configure" --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      && make -C lib install && make -C src install )
}
s_pixman()    { meson_build "$VM_SRC/pixman" -Dtests=disabled -Dlibpng=disabled -Dgtk=disabled; }
s_fontenc()   { conf_build "$VM_SRC/libfontenc"; }
s_xfont()     { conf_build "$VM_SRC/libxfont" --disable-devel-docs; }
s_xkbfile()   { conf_build "$VM_SRC/libxkbfile"; }
s_fontconfig(){ meson_build "$VM_SRC/fontconfig" -Ddoc=disabled -Dnls=disabled -Dtests=disabled -Dtools=enabled; }
s_xft()       { bash "$VM_ROOT/scripts/build_xft.sh"; }

# ---------- S18-S21 keyboard + display server ----------
s_xkbdata() { meson_build "$VM_SRC/xkeyboard-config" -Dnls=false -Dcompat-rules=true; }
s_xkbcomp() {
  conf_build "$VM_SRC/x11-xkb-utils/xkbcomp"
}
s_xauth()    { conf_build "$VM_SRC/xauth"; }
s_xbitmaps() { conf_build "$VM_SRC/xbitmaps"; }
s_libmd() { conf_build "$VM_SRC/libmd"; }
s_xvfb() {
  meson_build "$VM_SRC/xorg-server" \
    -Dxorg=false -Dxephyr=false -Dxnest=false -Dxwin=false -Dxquartz=false \
    -Dxvfb=true \
    -Dglamor=false -Dglx=false -Ddri3=false -Ddri2=false -Ddri1=false \
    -Ddrm=false -Dpciaccess=false -Dudev=false -Dudev_kms=false \
    -Dint10=false -Dvgahw=false -Dlinux_acpi=false -Dlinux_apm=false \
    -Dsecure-rpc=false -Dxf86-input-inputtest=false \
    -Ddocs=false -Ddevel-docs=false -Ddocs-pdf=false \
    -Dsha1=libmd -Dxkb_bin_dir="$VM_PREFIX/bin" -Dxkb_dir="$VM_PREFIX/share/X11/xkb" \
    -Ddefault_font_path="$VM_PREFIX/share/fonts/X11/misc,$VM_PREFIX/share/fonts/X11/TTF"
  mkdir -p "$VM_PREFIX/share/fonts/X11/misc" "$VM_PREFIX/share/fonts/X11/TTF"
}

# ---------- S22-S23 utilities ----------
s_xutils() {
  local rc=0
  for u in xdpyinfo xprop xwininfo xev; do
    conf_build "$VM_SRC/x11-utils/$u" || { echo "WARN: $u failed"; rc=1; }
  done
  for u in xrandr xrdb xset xmodmap xhost; do
    conf_build "$VM_SRC/x11-xserver-utils/$u" || { echo "WARN: $u failed"; rc=1; }
  done
  return $rc
}
s_xautomation() { conf_build "$VM_SRC/xautomation"; }

# ---------- extra client libs required by Blender (bpy wheel) ----------
s_xxf86vm()  { conf_build "$VM_SRC/libxxf86vm"; }
s_xkbcommon(){ meson_build "$VM_SRC/libxkbcommon" -Denable-wayland=false -Denable-x11=true -Denable-docs=false; }
s_glvnd() {
  # Headless GL dispatch stubs: libraries resolve at runtime via glvnd; no GPU
  # driver is present, so GL context creation is impossible but linking works.
  meson_build "$VM_SRC/libglvnd" -Dx11=enabled -Degl=false -Dglx=disabled \
    -Dgles1=disabled -Dgles2=disabled -Dheaders=true
}

case "${1:-all}" in
  all)
    stage shims      s_shims
    stage zlib       s_zlib
    stage libpng     s_libpng
    stage expat      s_expat
    stage freetype   s_freetype
    stage harfbuzz   s_harfbuzz
    stage xorgproto  s_xorgproto
    stage xtrans     s_xtrans
    stage libxau     s_libxau
    stage libxdmcp   s_libxdmcp
    stage xcbproto   s_xcbproto
    stage libxcb     s_libxcb
    stage libx11     s_libx11
    stage xmacros    s_xmacros
    stage extlibs    s_extlibs
    stage gperf      s_gperf
    stage pixman     s_pixman
    stage fontenc    s_fontenc
    stage xfont      s_xfont
    stage xkbfile    s_xkbfile
    stage fontconfig s_fontconfig
    stage xft        s_xft
    stage xkbdata    s_xkbdata
    stage xkbcomp    s_xkbcomp
    stage xauth      s_xauth
    stage xbitmaps   s_xbitmaps
    stage libmd      s_libmd
    stage xvfb       s_xvfb
    stage xutils     s_xutils
    stage xautomation s_xautomation
    stage xxf86vm    s_xxf86vm
    stage xkbcommon  s_xkbcommon
    stage glvnd      s_glvnd
    ;;
  *) stage "$1" "s_$1" ;;
esac
log "build_stack finished: $1"
