#!/usr/bin/env bash
# Package the verified production build as a distributable static site archive.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(node -p "require('./package.json').version")"
OUT="release/sourceforge-web-${VERSION}.zip"
ABS_OUT="$(pwd)/${OUT}"

node scripts/validate-build.mjs
[ -f dist/index.html ] || { echo "error: run npm run build first" >&2; exit 1; }
mkdir -p release
rm -f "$OUT"
(cd dist && zip -qr "$ABS_OUT" .)
echo "built $OUT ($(du -h "$OUT" | cut -f1))"

unzip -l "$OUT" | grep -q "index.html" || { echo "FAIL: index.html missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "sw.js" || { echo "FAIL: sw.js missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "manifest.webmanifest" || { echo "FAIL: manifest missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "icons/icon-512.png" || { echo "FAIL: icons missing" >&2; exit 1; }
echo "OK: web archive is a complete static PWA build"
