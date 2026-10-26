#!/usr/bin/env bash
# Application-level builds on top of the X stack (build_stack.sh).
# Resumable via markers, same conventions as build_stack.sh.
set -uo pipefail
source "$(dirname "$0")/env.sh"
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"
stage() { local name="$1"; shift; [ -f "$MARK/$name" ] && { log "SKIP $name"; return 0; }; log "STAGE $name"; "$@" && touch "$MARK/$name" || fail "stage $name failed"; }

# ---------- autotools chain (unblocks autogen-only packages) ----------
s_m4() {
  # docs need help2man/makeinfo (absent): build lib + binary only
  rm -rf "$VM_BUILD/b-m4"; mkdir -p "$VM_BUILD/b-m4"
  ( cd "$VM_BUILD/b-m4" && "$VM_SRC/m4/configure" --prefix="$VM_PREFIX" \
      --libdir="$VM_PREFIX/lib" && make -C lib && make -C src install )
}
s_autoconf() { conf_build "$VM_SRC/autoconf"; }
s_automake() {
  conf_build "$VM_SRC/automake-1.17"
  # expose unversioned names expected by autogen.sh scripts
  ln -sf automake-1.17 "$VM_PREFIX/bin/automake"
  ln -sf aclocal-1.17  "$VM_PREFIX/bin/aclocal"
}
s_libtool()  {
  conf_build "$VM_SRC/libtool"
  ln -sf libtoolize  "$VM_PREFIX/bin/glibtoolize" 2>/dev/null || true
}

# ---------- FLTK 1.3 (dillo toolkit), no GL ----------
s_fltk13() {
  cmake_build "$VM_SRC/fltk13" \
    -DOPTION_USE_GL=OFF -DFLTK_BUILD_TEST=OFF -DFLTK_BUILD_EXAMPLES=OFF \
    -DFLTK_BUILD_FLUID=OFF -DOPTION_BUILD_HTML_DOCUMENTATION=OFF \
    -DOPTION_BUILD_PDF_DOCUMENTATION=OFF -DOPTION_USE_SYSTEM_ZLIB=ON \
    -DOPTION_USE_SYSTEM_LIBPNG=ON -DOPTION_USE_SYSTEM_LIBJPEG=OFF
}

# ---------- mbedTLS 2.28 LTS (dillo HTTPS) ----------
s_mbedtls() {
  cmake_build "$VM_SRC/mbedtls" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DENABLE_TESTING=OFF -DENABLE_PROGRAMS=OFF \
    -DUSE_SHARED_MBEDTLS_LIBRARY=ON -DUSE_STATIC_MBEDTLS_LIBRARY=OFF
}

# ---------- dillo browser (autogen-only upstream tree) ----------
s_dillo() {
  ( cd "$VM_SRC/dillo" && \
    autoreconf -fi && \
    "$VM_SRC/dillo/configure" --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" \
      --disable-dbg --enable-cookies --disable-openssl \
      --with-mbedtls-inc="$VM_PREFIX/include" \
      --with-mbedtls-lib="$VM_PREFIX/lib" && \
    make -j"$(nproc)" && make install ) || return 1
}

# ---------- OpenSSL 3.0 (x11vnc crypto, bpy wheel runtime) ----------
s_openssl() {
  # OpenSSL builds inside its source tree; keep markers so re-runs are cheap
  ( cd "$VM_SRC/openssl" && \
    [ -x apps/openssl ] || ./Configure shared --prefix="$VM_PREFIX" --libdir=lib \
      --openssldir="$VM_PREFIX/etc/ssl" no-tests no-legacy ) || return 1
  ( cd "$VM_SRC/openssl" && make -j"$(nproc)" && make install_sw && make install_ssldirs ) || return 1
  install -m755 "$VM_SRC/openssl/apps/openssl" "$VM_PREFIX/bin/openssl"
}

# ---------- libjpeg-turbo (libvncserver dependency) ----------
s_libjpeg() {
  cmake_build "$VM_SRC/libjpeg-turbo" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DENABLE_SHARED=TRUE -DENABLE_STATIC=FALSE -DWITH_SIMD=TRUE
}

# ---------- libvncserver + x11vnc (tablet remote access) ----------
s_libvnc() {
  cmake_build "$VM_SRC/libvncserver" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DWITH_OPENSSL=OFF -DWITH_GCRYPT=OFF -DWITH_GNUTLS=OFF -DWITH_SYSTEMD=OFF \
    -DWITH_SASL=OFF -DWITH_THREADS=ON -DBUILD_SHARED_LIBS=ON
}
s_x11vnc() {
  ( cd "$VM_SRC/x11vnc" && [ -x configure ] || autoreconf -fi ) || return 1
  rm -rf "$VM_BUILD/b-x11vnc"; mkdir -p "$VM_BUILD/b-x11vnc"
  ( cd "$VM_BUILD/b-x11vnc" && \
    "$VM_SRC/x11vnc/configure" --prefix="$VM_PREFIX" --without-avahi --without-systemd && \
    make -j"$(nproc)" && make install ) || return 1
}

# ---------- vim terminal editor ----------
s_vim() {
  rm -rf "$VM_BUILD/b-vim"; mkdir -p "$VM_BUILD/b-vim"
  ( cd "$VM_BUILD/b-vim" && \
    "$VM_SRC/vim/src/auto/configure" --prefix="$VM_PREFIX" --with-features=normal \
      --without-x --disable-gui --disable-netbeans --enable-multibyte \
      --with-tlib=ncursesw && \
    make -j"$(nproc)" && make install ) || return 1
}

case "${1:-all}" in
  all)
    stage m4       s_m4
    stage autoconf s_autoconf
    stage automake s_automake
    stage libtool  s_libtool
    stage openssl  s_openssl
    stage fltk13   s_fltk13
    stage mbedtls  s_mbedtls
    stage dillo    s_dillo
    stage libjpeg  s_libjpeg
    stage libvnc   s_libvnc
    stage x11vnc   s_x11vnc
    stage vim      s_vim
    ;;
  *) stage "$1" "s_$1" ;;
esac
log "build_apps finished: $1"
