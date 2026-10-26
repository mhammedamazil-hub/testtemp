# Sourced by every build script. Defines the local toolchain/prefix environment.
# All builds are relocatable inside arena-vm/ and need no root access.
export VM_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export VM_SRC="$VM_ROOT/src"
export VM_BUILD="$VM_ROOT/build"
export VM_PREFIX="$VM_ROOT/local"
export VM_VENV="$VM_PREFIX/venv"

mkdir -p "$VM_SRC" "$VM_BUILD" "$VM_PREFIX/lib/pkgconfig" "$VM_PREFIX/share/pkgconfig"

# Python tooling (meson/ninja/cmake) live in the venv
export PATH="$VM_VENV/bin:$VM_PREFIX/bin:$PATH"

# pkg-config (pkgconf) once built; before that these vars are harmless
export PKG_CONFIG_PATH="$VM_PREFIX/lib/pkgconfig:$VM_PREFIX/share/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="/"
export PKG_CONFIG_LIBDIR="$VM_PREFIX/lib/pkgconfig:$VM_PREFIX/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig"

# Compiler flags: find local headers/libs and bake an rpath into every binary
export CPPFLAGS="-I$VM_PREFIX/include"
export CFLAGS="-O2 -g0 -pipe -I$VM_PREFIX/include"
export CXXFLAGS="-O2 -g0 -pipe -I$VM_PREFIX/include"
export LDFLAGS="-L$VM_PREFIX/lib -Wl,-rpath,$VM_PREFIX/lib -Wl,-rpath-link,$VM_PREFIX/lib"
export LIBRARY_PATH="$VM_PREFIX/lib"
# Runtime loader path for prebuilt wheels (bpy) that find system-style libs
# like libxkbcommon.so.0 at dlopen time rather than via build rpath.
[ "${VM_LD_PATH_SET:-}" = 1 ] || export LD_LIBRARY_PATH="$VM_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
VM_LD_PATH_SET=1

# Terminfo from the locally built ncurses (st/ranger/vim curses support)
[ -d "$VM_PREFIX/share/terminfo" ] && export TERMINFO="$VM_PREFIX/share/terminfo"
export C_INCLUDE_PATH="$VM_PREFIX/include"
export CPLUS_INCLUDE_PATH="$VM_PREFIX/include"

# Meson default options (used by meson_setup helper)
export VM_MESON_COMMON=(--prefix="$VM_PREFIX" --libdir=lib --buildtype=debugoptimized -Ddefault_library=shared -Db_lundef=false)

log()  { printf '\n\033[1;36m[VM]\033[0m %s\n' "$*"; }
fail() { printf '\n\033[1;31m[FAIL]\033[0m %s\n' "$*" >&2; exit 1; }

# Idempotently apply scripts/patches/<name>.patch to a source tree
apply_patch() {
  local src="$1"
  local name="$2"
  local p="$VM_ROOT/scripts/patches/$name.patch"
  [ -f "$p" ] || { log "no patch $name, skip"; return 0; }
  if ( cd "$src" && patch -R -p1 --dry-run -s < "$p" ) 2>/dev/null; then
    log "patch $name already applied"; return 0
  fi
  log "apply patch $name"
  ( cd "$src" && patch -N -p1 --no-backup-if-mismatch -s < "$p" ) || fail "patch $name"
}

# Pin every mtime in a source tree to one fixed timestamp. GNU make treats
# equal mtimes as up-to-date, which neutralizes autotools remake rules that
# would otherwise demand autoreconf tools absent from this image.
pin_timestamps() {
  local src="$1"
  find "$src" -type f -exec touch -h -d '2020-01-01 00:00:00 UTC' {} +
}

# configure-based build (autotools dist trees carry a pre-generated configure)
conf_build() {
  local src="$1"; shift
  log "configure+make: $(basename "$src")"
  pin_timestamps "$src"
  rm -rf "$VM_BUILD/b-$(basename "$src")"
  mkdir -p "$VM_BUILD/b-$(basename "$src")"
  ( cd "$VM_BUILD/b-$(basename "$src")" && \
    "$src/configure" --prefix="$VM_PREFIX" --libdir="$VM_PREFIX/lib" "$@" && \
    make -j"$(nproc)" && make install ) || fail "conf_build $src"
}

# meson-based build; args after src are meson options
meson_build() {
  local src="$1"; shift
  local name
  name="$(basename "$src")"
  log "meson: $name"
  rm -rf "$VM_BUILD/m-$name"
  meson setup "$VM_BUILD/m-$name" "$src" "${VM_MESON_COMMON[@]}" "$@" || fail "meson setup $name"
  meson compile -C "$VM_BUILD/m-$name" || fail "meson compile $name"
  meson install -C "$VM_BUILD/m-$name" || fail "meson install $name"
}

# cmake-based build
cmake_build() {
  local src="$1"; shift
  local name
  name="$(basename "$src")"
  log "cmake: $name"
  rm -rf "$VM_BUILD/c-$name"
  cmake -S "$src" -B "$VM_BUILD/c-$name" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$VM_PREFIX" \
    -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_SKIP_RPATH=OFF \
    -DCMAKE_INSTALL_RPATH="$VM_PREFIX/lib" -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON "$@" || fail "cmake $name"
  cmake --build "$VM_BUILD/c-$name" -j"$(nproc)" || fail "cmake build $name"
  cmake --install "$VM_BUILD/c-$name" || fail "cmake install $name"
}

# plain makefile build (suckless-style); pass extra make vars as args
make_build() {
  local src="$1"; shift
  log "make: $(basename "$src")"
  make -C "$src" -j"$(nproc)" PREFIX="$VM_PREFIX" "$@" || fail "make $src"
  make -C "$src" PREFIX="$VM_PREFIX" "$@" install || fail "make install $src"
}
