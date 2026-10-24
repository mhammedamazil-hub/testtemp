#!/usr/bin/env bash
# Build and package Stonewild Realms for 64-bit Linux Mint 21+.
set -euo pipefail

VERSION="0.3.0"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OVERLAY_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
OUTPUT_DIR="${1:-}"
if [[ -z "${OUTPUT_DIR}" ]]; then
  echo "usage: $0 OUTPUT_DIRECTORY" >&2
  exit 2
fi
mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd -- "${OUTPUT_DIR}" && pwd)"
OUTPUT_DEB="${OUTPUT_DIR}/stonewild-realms_${VERSION}_amd64.deb"
if [[ -e "${OUTPUT_DEB}" ]]; then
  echo "refusing to overwrite existing output: ${OUTPUT_DEB}" >&2
  exit 2
fi

WORK_ROOT="$(mktemp -d "${RUNNER_TEMP:-/tmp}/stonewild-linux.XXXXXX")"
cleanup() { rm -rf "${WORK_ROOT}"; }
trap cleanup EXIT
SOURCE="${WORK_ROOT}/source"
BUILD="${WORK_ROOT}/build"
PACKAGE="${WORK_ROOT}/package"
"${SCRIPT_DIR}/prepare_upstream.sh" "${SOURCE}" "${STONEWILD_DOWNLOAD_CACHE:-${RUNNER_TEMP:-/tmp}/stonewild-luanti-cache}"

cmake -S "${SOURCE}" -B "${BUILD}" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX=/opt/stonewild-realms \
  -DRUN_IN_PLACE=TRUE \
  -DBUILD_CLIENT=TRUE \
  -DBUILD_SERVER=FALSE \
  -DBUILD_UNITTESTS=FALSE \
  -DENABLE_UPDATE_CHECKER=FALSE
cmake --build "${BUILD}" --parallel "${STONEWILD_BUILD_JOBS:-2}"
DESTDIR="${PACKAGE}" cmake --install "${BUILD}" --strip

install -d \
  "${PACKAGE}/usr/bin" \
  "${PACKAGE}/usr/share/applications" \
  "${PACKAGE}/usr/share/icons/hicolor/256x256/apps" \
  "${PACKAGE}/usr/share/doc/stonewild-realms" \
  "${PACKAGE}/opt/stonewild-realms/games" \
  "${PACKAGE}/DEBIAN"
cp -a "${SOURCE}/games/stonewild" "${PACKAGE}/opt/stonewild-realms/games/stonewild"
install -m 0644 "${SOURCE}/games/stonewild/minetest.conf" \
  "${PACKAGE}/opt/stonewild-realms/stonewild.conf"
install -m 0755 "${OVERLAY_DIR}/packaging/stonewild-realms" \
  "${PACKAGE}/usr/bin/stonewild-realms"
install -m 0644 "${OVERLAY_DIR}/packaging/org.stonewild.realms.desktop" \
  "${PACKAGE}/usr/share/applications/org.stonewild.realms.desktop"
install -m 0644 "${OVERLAY_DIR}/branding/linux_icon.png" \
  "${PACKAGE}/usr/share/icons/hicolor/256x256/apps/org.stonewild.realms.png"
install -m 0644 "${OVERLAY_DIR}/ENGINE_NOTICE.md" \
  "${PACKAGE}/usr/share/doc/stonewild-realms/ENGINE_NOTICE.md"
install -m 0644 "${SOURCE}/doc/lgpl-2.1.txt" \
  "${PACKAGE}/usr/share/doc/stonewild-realms/LUANTI-LGPL-2.1.txt"
install -m 0644 "${SOURCE}/games/stonewild/LICENSE.txt" \
  "${PACKAGE}/usr/share/doc/stonewild-realms/STONEWILD-MIT.txt"
install -m 0644 "${SOURCE}/games/stonewild/ASSET_LICENSE.md" \
  "${PACKAGE}/usr/share/doc/stonewild-realms/ASSET_LICENSE.md"

# CMake's generic desktop integration is intentionally replaced with the
# standalone Stonewild launcher and icon above. These paths exist only inside
# this newly-created package staging directory.
rm -rf \
  "${PACKAGE}/opt/stonewild-realms/unix/applications" \
  "${PACKAGE}/opt/stonewild-realms/unix/icons" \
  "${PACKAGE}/opt/stonewild-realms/unix/metainfo"
rm -f "${PACKAGE}/opt/stonewild-realms/bin/minetest"

BINARY="${PACKAGE}/opt/stonewild-realms/bin/luanti"
test -x "${BINARY}"

# Derive runtime library dependencies from the actual ELF rather than keeping a
# fragile handwritten list.
mkdir -p "${WORK_ROOT}/shlibs/debian"
cat > "${WORK_ROOT}/shlibs/debian/control" <<'CONTROL'
Source: stonewild-realms
Section: games
Priority: optional
Maintainer: Stonewild Contributors <noreply@example.invalid>

Package: stonewild-realms
Architecture: amd64
Description: Original realistic voxel survival game
CONTROL
DEPENDS="$(cd "${WORK_ROOT}/shlibs" && dpkg-shlibdeps -O -e"${BINARY}" | sed -n 's/^shlibs:Depends=//p')"
if [[ -z "${DEPENDS}" ]]; then
  echo "dpkg-shlibdeps did not produce runtime dependencies" >&2
  exit 1
fi
INSTALLED_SIZE="$(du -sk "${PACKAGE}" | cut -f1)"
cat > "${PACKAGE}/DEBIAN/control" <<CONTROL
Package: stonewild-realms
Version: ${VERSION}
Section: games
Priority: optional
Architecture: amd64
Maintainer: Stonewild Contributors <noreply@example.invalid>
Depends: ${DEPENDS}
Installed-Size: ${INSTALLED_SIZE}
Homepage: https://github.com/mhammedamazil-hub/testtemp
Description: Stonewild Realms original voxel survival game
 Explore a generated wilderness, gather realistic materials, craft tools,
 manage nourishment and health, and survive a full day-night cycle. This
 package embeds the independently maintained LGPL Luanti 5.17.0 engine.
CONTROL

# A headless local server launch loads both Stonewild mods, initializes mapgen,
# creates a real world database, and catches Lua/API failures before packaging.
RUNTIME_HOME="${WORK_ROOT}/runtime-home"
RUNTIME_WORLD="${WORK_ROOT}/runtime-world"
mkdir -p "${RUNTIME_HOME}"
set +e
LUANTI_USER_PATH="${RUNTIME_HOME}" timeout --signal=INT --kill-after=5s 12s \
  "${BINARY}" --server --world "${RUNTIME_WORLD}" --gameid stonewild \
  --config "${SOURCE}/games/stonewild/minetest.conf" --info \
  >"${WORK_ROOT}/runtime.log" 2>&1
RUNTIME_STATUS=$?
set -e
cat "${WORK_ROOT}/runtime.log"
if [[ "${RUNTIME_STATUS}" -ne 0 && "${RUNTIME_STATUS}" -ne 124 ]]; then
  echo "Stonewild runtime smoke test failed with status ${RUNTIME_STATUS}" >&2
  exit "${RUNTIME_STATUS}"
fi
if grep -E 'ERROR\[Main\]|ModError|LuaError|ServerError' "${WORK_ROOT}/runtime.log"; then
  echo "Stonewild runtime smoke test reported an engine or Lua error" >&2
  exit 1
fi
test -f "${RUNTIME_WORLD}/world.mt"

dpkg-deb --build --root-owner-group "${PACKAGE}" "${OUTPUT_DEB}"
printf 'Built %s\n' "${OUTPUT_DEB}"
