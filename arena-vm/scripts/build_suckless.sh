#!/usr/bin/env bash
# Build dwm (window manager), st (terminal), dmenu (launcher) and install TTFs.
set -euo pipefail
source "$(dirname "$0")/env.sh"
MARK="$VM_BUILD/markers"; mkdir -p "$MARK"
stage() { local n="$1"; shift; [ -f "$MARK/$n" ] && { log "SKIP $n"; return; }; "$@" && touch "$MARK/$n"; }

s_ncurses() {
  conf_build "$VM_SRC/ncurses" --with-shared --without-debug \
    --enable-widec --enable-ext-colors
}

s_fonts() {
  local dst="$VM_PREFIX/share/fonts/truetype/dejavu"
  mkdir -p "$dst"
  cp "$VM_VENV"/lib/python*/site-packages/matplotlib/mpl-data/fonts/ttf/DejaVu*.ttf "$dst/"
  mkdir -p "$VM_PREFIX/etc/fonts" "$VM_PREFIX/cache/fontconfig"
  # fontconfig must also scan our prefix (system /etc/fonts is not writable)
  cat > "$VM_PREFIX/etc/fonts/local.conf" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>$VM_PREFIX/share/fonts</dir>
  <cachedir>$VM_PREFIX/cache/fontconfig</cachedir>
</fontconfig>
EOF
  FONTCONFIG_PATH="$VM_PREFIX/etc/fonts" fc-cache -f "$VM_PREFIX/share/fonts" >/dev/null
  FONTCONFIG_PATH="$VM_PREFIX/etc/fonts" fc-list | grep -c DejaVu
}

s_dwm() {
  make -C "$VM_SRC/dwm" clean >/dev/null 2>&1 || true
  make -C "$VM_SRC/dwm" -j"$(nproc)" PREFIX="$VM_PREFIX" \
    X11INC="$VM_PREFIX/include" X11LIB="$VM_PREFIX/lib" \
    FREETYPEINC="$VM_PREFIX/include/freetype2"
  make -C "$VM_SRC/dwm" PREFIX="$VM_PREFIX" install
}

s_st() {
  make -C "$VM_SRC/st" clean >/dev/null 2>&1 || true
  make -C "$VM_SRC/st" -j"$(nproc)" PREFIX="$VM_PREFIX" \
    X11INC="$VM_PREFIX/include" X11LIB="$VM_PREFIX/lib"
  # install without the terminfo/tic step (tic arrives with ncurses later)
  install -Dm755 "$VM_SRC/st/st" "$VM_PREFIX/bin/st"
  install -Dm644 "$VM_SRC/st/st.1" "$VM_PREFIX/share/man/man1/st.1"
  install -Dm644 "$VM_SRC/st/st.desktop" "$VM_PREFIX/share/applications/st.desktop" 2>/dev/null || true
}

s_dmenu() {
  make -C "$VM_SRC/dmenu" clean >/dev/null 2>&1 || true
  make -C "$VM_SRC/dmenu" -j"$(nproc)" PREFIX="$VM_PREFIX" \
    X11INC="$VM_PREFIX/include" X11LIB="$VM_PREFIX/lib" \
    FREETYPEINC="$VM_PREFIX/include/freetype2"
  make -C "$VM_SRC/dmenu" PREFIX="$VM_PREFIX" install
}

case "${1:-all}" in
  all) stage ncurses s_ncurses; stage fonts s_fonts
       stage dwm s_dwm; stage st s_st; stage dmenu s_dmenu ;;
  *) stage "$1" "s_$1" ;;
esac
log "suckless build finished: $1"
