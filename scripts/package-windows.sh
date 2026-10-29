#!/usr/bin/env bash
# Assemble the Windows portable ZIP: the verified web build plus the PowerShell
# launcher. Does not create a Windows installer (see packaging/windows/README.md).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(node -p "require('./package.json').version")"
NAME="sourceforge-${VERSION}-windows-portable"
OUT="release/${NAME}.zip"
ABS_OUT="$(pwd)/${OUT}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/$NAME"

[ -f dist/index.html ] || { echo "error: run npm run build first" >&2; exit 1; }
mkdir -p "$PKG/app" release
cp -r dist/. "$PKG/app/"
cp packaging/windows/Start-SourceForge.ps1 packaging/windows/Start-SourceForge.cmd "$PKG/"

cat > "$PKG/README.txt" <<'TXT'
SourceForge - portable Windows build
====================================

1. Unzip this folder anywhere.
2. Double-click Start-SourceForge.cmd
3. Your browser opens http://127.0.0.1:8787/ with the app.

No installer and no admin rights are required. The launcher is a small
PowerShell HttpListener that serves the bundled files on loopback.

Data is stored in your browser profile (IndexedDB). Use "Export backup" in the
app before clearing browser data.

Status: this bundle is built by CI from the verified web build; the launcher
has NOT been executed on a Windows host yet. See packaging/windows/README.md.
TXT

rm -f "$OUT"
(cd "$STAGE" && zip -qr "$ABS_OUT" "$NAME")
echo "built $OUT ($(du -h "$OUT" | cut -f1))"

echo "==> verifying"
unzip -l "$OUT" | grep -q "$NAME/app/index.html" || { echo "FAIL: app shell missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "$NAME/app/sw.js" || { echo "FAIL: service worker missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "$NAME/app/manifest.webmanifest" || { echo "FAIL: manifest missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "Start-SourceForge.cmd" || { echo "FAIL: launcher missing" >&2; exit 1; }
unzip -l "$OUT" | grep -q "Start-SourceForge.ps1" || { echo "FAIL: PowerShell launcher missing" >&2; exit 1; }
echo "OK: portable zip contains the app shell, PWA files and both launchers"
