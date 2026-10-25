#!/usr/bin/env bash
# Build the pinned, emulator-tested Stonewild Realms universal debug APK.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${1:-}"
if [[ -z "${OUTPUT_DIR}" ]]; then
  echo "usage: $0 OUTPUT_DIRECTORY" >&2
  exit 2
fi
mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd -- "${OUTPUT_DIR}" && pwd)"
OUTPUT_APK="${OUTPUT_DIR}/stonewild-realms-universal-debug.apk"
if [[ -e "${OUTPUT_APK}" ]]; then
  echo "refusing to overwrite existing output: ${OUTPUT_APK}" >&2
  exit 2
fi

WORK_ROOT="$(mktemp -d "${RUNNER_TEMP:-/tmp}/stonewild-android.XXXXXX")"
cleanup() { rm -rf "${WORK_ROOT}"; }
trap cleanup EXIT
"${SCRIPT_DIR}/prepare_upstream.sh" "${WORK_ROOT}/source" "${STONEWILD_DOWNLOAD_CACHE:-${RUNNER_TEMP:-/tmp}/stonewild-luanti-cache}"

(
  cd "${WORK_ROOT}/source/android"
  ./gradlew --no-daemon --stacktrace :app:assembleDebug
)

APK="$(find "${WORK_ROOT}/source/android/app/build/outputs/apk" \
  -type f -name '*universal-debug.apk' -print -quit)"
if [[ -z "${APK}" || ! -s "${APK}" ]]; then
  echo "Gradle completed but the universal debug APK was not found" >&2
  exit 1
fi
cp "${APK}" "${OUTPUT_APK}"
(
  cd "${OUTPUT_DIR}"
  sha256sum "$(basename -- "${OUTPUT_APK}")" > SHA256SUMS.txt
  stat --printf='APK_BYTES=%s\n' "$(basename -- "${OUTPUT_APK}")" > APK_SIZE.txt
)
printf 'Built %s (%s bytes)\n' "${OUTPUT_APK}" "$(stat --printf='%s' "${OUTPUT_APK}")"
