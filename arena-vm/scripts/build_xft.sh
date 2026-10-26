#!/usr/bin/env bash
# Build libXft with the hand-written meson wrapper in scripts/xft/.
set -euo pipefail
source "$(dirname "$0")/env.sh"
SRC="$VM_SRC/libxft"

# Autotools would substitute these #undef lines in Xft.h; do it with sed.
sed -e 's|^#undef XFT_MAJOR|#define XFT_MAJOR 2|' \
    -e 's|^#undef XFT_MINOR|#define XFT_MINOR 3|' \
    -e 's|^#undef XFT_REVISION|#define XFT_REVISION 2|' \
    "$SRC/include/X11/Xft/Xft.h.in" > "$SRC/include/X11/Xft/Xft.h"

cp "$VM_ROOT/scripts/xft/meson.build" "$SRC/meson.build"
meson_build "$SRC"
