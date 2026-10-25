#!/usr/bin/env bash
# Prepare a reproducible, standalone Stonewild source tree from pristine Luanti.
set -euo pipefail

LUANTI_VERSION="5.17.0"
LUANTI_ARCHIVE_SHA256="52e7dd315ae0e5c3868a23231e691578cd694bcca5af627fc57c03f32bdc846f"
LUANTI_URL="https://codeload.github.com/luanti-org/luanti/tar.gz/refs/tags/${LUANTI_VERSION}"
ANDROID_APP_ID="org.stonewild.realms"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OVERLAY_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd -- "${OVERLAY_DIR}/../.." && pwd)"
DESTINATION="${1:-}"
CACHE_DIR="${2:-${RUNNER_TEMP:-/tmp}/stonewild-luanti-cache}"

if [[ -z "${DESTINATION}" ]]; then
  echo "usage: $0 DESTINATION [DOWNLOAD_CACHE]" >&2
  exit 2
fi

case "${DESTINATION}" in
  /|"${REPO_ROOT}"|"${REPO_ROOT}/.git"|"${OVERLAY_DIR}")
    echo "refusing unsafe destination: ${DESTINATION}" >&2
    exit 2
    ;;
esac

if [[ -e "${DESTINATION}" ]]; then
  echo "destination already exists; choose a new empty path: ${DESTINATION}" >&2
  exit 2
fi

mkdir -p "${CACHE_DIR}"
ARCHIVE="${CACHE_DIR}/luanti-${LUANTI_VERSION}.tar.gz"
if [[ ! -f "${ARCHIVE}" ]] || ! echo "${LUANTI_ARCHIVE_SHA256}  ${ARCHIVE}" | sha256sum --check --status; then
  rm -f "${ARCHIVE}"
  curl --fail --location --retry 3 --retry-delay 2 --output "${ARCHIVE}" "${LUANTI_URL}"
fi
echo "${LUANTI_ARCHIVE_SHA256}  ${ARCHIVE}" | sha256sum --check --status

PARENT="$(dirname -- "${DESTINATION}")"
STAGING="${PARENT}/.$(basename -- "${DESTINATION}").prepare.$$"
rm -rf "${STAGING}"
mkdir -p "${STAGING}"
cleanup() { rm -rf "${STAGING}"; }
trap cleanup EXIT
tar --extract --gzip --file "${ARCHIVE}" --directory "${STAGING}" --strip-components=1

rm -rf "${STAGING}/games/devtest"
mkdir -p "${STAGING}/games"
cp -a "${OVERLAY_DIR}/games/stonewild" "${STAGING}/games/stonewild"
cp "${OVERLAY_DIR}/ENGINE_NOTICE.md" "${STAGING}/STONEWILD_ENGINE_NOTICE.md"

python3 - "${STAGING}" "${ANDROID_APP_ID}" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
app_id = sys.argv[2]


def replace_exact(path: Path, old: str, new: str, expected: int = 1) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} occurrence(s), found {count}: {old!r}")
    path.write_text(text.replace(old, new))

app_gradle = root / "android/app/build.gradle"
replace_exact(app_gradle, "applicationId 'net.minetest.minetest'", f"applicationId '{app_id}'")
replace_exact(
    app_gradle,
    '\t\tcopy {\n\t\t\tfrom "${localeFolder}" into "${assetsFolder}/locale"\n\t\t}\n',
    '\t\tcopy {\n\t\t\tfrom "${localeFolder}" into "${assetsFolder}/locale"\n\t\t}\n'
    '\t\tcopy {\n'
    '\t\t\tfrom "${projRoot}/games/stonewild" into "${assetsFolder}/games/stonewild"\n'
    '\t\t}\n'
    '\t\tcopy {\n'
    '\t\t\tfrom("${projRoot}/games/stonewild/minetest.conf") { rename { "stonewild.conf" } }\n'
    '\t\t\tinto assetsFolder\n'
    '\t\t}\n'
    '\t\tcopy {\n'
    '\t\t\tfrom "${projRoot}/STONEWILD_ENGINE_NOTICE.md" into assetsFolder\n'
    '\t\t}\n',
)

# Build one self-contained APK. arm64-v8a is the production phone/tablet ABI;
# x86_64 is included so the exact same APK can be parser/install/launch-tested
# in a hardware-accelerated Android emulator before publication.
abi_list = "'armeabi-v7a', 'arm64-v8a', 'x86', 'x86_64'"
universal_abis = "'arm64-v8a', 'x86_64'"
replace_exact(app_gradle, abi_list, universal_abis)
replace_exact(root / "android/native/build.gradle", abi_list, universal_abis)
replace_exact(
    app_gradle,
    "\t\t\tinclude 'arm64-v8a', 'x86_64'\n\t\t}\n",
    "\t\t\tinclude 'arm64-v8a', 'x86_64'\n\t\t\tuniversalApk true\n\t\t}\n",
)

manifest = root / "android/app/src/main/AndroidManifest.xml"
replace_exact(manifest, 'net.minetest.minetest.fileprovider', f'{app_id}.fileprovider')
replace_exact(manifest, 'net.minetest.minetest.documents', f'{app_id}.documents')
replace_exact(
    root / "android/app/src/main/java/net/minetest/minetest/GameActivity.java",
    '"net.minetest.minetest.fileprovider"',
    f'"{app_id}.fileprovider"',
)

# Keep Stonewild focused on local survival play while preserving the About tab
# and all upstream engine attribution. Network code remains available for later
# multiplayer phases, but the generic server browser and add-on store are not
# part of the standalone game's main interface.
replace_exact(
    root / "builtin/mainmenu/init.lua",
    "\ttv_main:add(tabs.local_game)\n\ttv_main:add(tabs.play_online)\n\ttv_main:add(tabs.content)\n\ttv_main:add(tabs.about)\n",
    "\ttv_main:add(tabs.local_game)\n\ttv_main:add(tabs.about)\n",
)

# Keep the established Java namespace to minimize the upstream patch while all
# externally visible Android IDs use the unique Stonewild application ID.
for strings in (root / "android/app/src/main/res").glob("values*/strings.xml"):
    text = strings.read_text()
    replacements = {
        "label": "Stonewild Realms",
        "unzip_notification_title": "Preparing Stonewild Realms",
        "game_notification_title": "Stonewild Realms is running",
    }
    for name, value in replacements.items():
        pattern = rf'(<string name="{re.escape(name)}">).*?(</string>)'
        text = re.sub(pattern, rf'\g<1>{value}\g<2>', text)
    text = text.replace("Notifications from Luanti", "Stonewild Realms notifications")
    strings.write_text(text)

service = root / "android/app/src/main/java/net/minetest/minetest/UnzipService.java"
replace_exact(
    service,
    "\t\t\tunzip(notificationBuilder, zipFile, userDataDirectory);\n",
    "\t\t\tunzip(notificationBuilder, zipFile, userDataDirectory);\n"
    "\t\t\tinstallStonewildDefaults(userDataDirectory);\n",
)
replace_exact(
    service,
    "\tprivate void publishProgress(@Nullable Notification.Builder notificationBuilder, @StringRes int message, int progress) {\n",
    "\tprivate void installStonewildDefaults(File userDataDirectory) throws IOException {\n"
    "\t\tFile settings = new File(userDataDirectory, \"minetest.conf\");\n"
    "\t\tif (settings.exists())\n"
    "\t\t\treturn;\n"
    "\t\tFile defaults = new File(userDataDirectory, \"stonewild.conf\");\n"
    "\t\ttry (InputStream in = new FileInputStream(defaults);\n"
    "\t\t     OutputStream out = new FileOutputStream(settings)) {\n"
    "\t\t\tbyte[] buffer = new byte[16384];\n"
    "\t\t\tint length;\n"
    "\t\t\twhile ((length = in.read(buffer)) != -1)\n"
    "\t\t\t\tout.write(buffer, 0, length);\n"
    "\t\t}\n"
    "\t}\n\n"
    "\tprivate void publishProgress(@Nullable Notification.Builder notificationBuilder, @StringRes int message, int progress) {\n",
)
PY

cp "${OVERLAY_DIR}/branding/android_launcher.png" \
  "${STAGING}/android/app/src/main/res/mipmap/ic_launcher.png"
cp "${OVERLAY_DIR}/branding/android_background.png" \
  "${STAGING}/android/app/src/main/res/drawable/background.png"

touch "${STAGING}/.stonewild-prepared-source"
mv "${STAGING}" "${DESTINATION}"
printf 'Prepared Stonewild on Luanti %s at %s\n' "${LUANTI_VERSION}" "${DESTINATION}"
