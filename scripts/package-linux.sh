#!/usr/bin/env bash
# Build a real .deb from the production web build, then verify it.
#
# The package installs the static app into /usr/share/sourceforge and a tiny
# launcher that serves it on 127.0.0.1 via python3's http.server, then opens the
# default browser. No background daemon, no root service, no network exposure.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(node -p "require('./package.json').version")"
OUT="release/sourceforge_${VERSION}_all.deb"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

if [ ! -f dist/index.html ]; then
  echo "error: dist/index.html missing — run 'npm run build' first" >&2
  exit 1
fi
if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "error: dpkg-deb is required (apt install dpkg)" >&2
  exit 1
fi

mkdir -p "$STAGE/DEBIAN" \
         "$STAGE/usr/share/sourceforge" \
         "$STAGE/usr/bin" \
         "$STAGE/usr/share/applications" \
         "$STAGE/usr/share/icons/hicolor/512x512/apps" \
         "$STAGE/usr/share/doc/sourceforge"

cp -r dist/. "$STAGE/usr/share/sourceforge/"
cp public/icons/icon-512.png "$STAGE/usr/share/icons/hicolor/512x512/apps/sourceforge.png"
cp README.md "$STAGE/usr/share/doc/sourceforge/README.md" 2>/dev/null || true

cat > "$STAGE/usr/bin/sourceforge" <<'LAUNCHER'
#!/bin/sh
# SourceForge launcher: serve the installed static app on loopback and open a browser.
set -eu
APP_DIR="${SOURCEFORGE_HOME:-/usr/share/sourceforge}"
PORT="${SOURCEFORGE_PORT:-8787}"
PYTHON="$(command -v python3 || command -v python || true)"
if [ -z "$PYTHON" ]; then
  echo "SourceForge needs python3 to serve its files locally (apt install python3)." >&2
  exit 1
fi
if [ ! -f "$APP_DIR/index.html" ]; then
  echo "SourceForge files are missing from $APP_DIR" >&2
  exit 1
fi
URL="http://127.0.0.1:${PORT}/"
"$PYTHON" -m http.server "$PORT" --bind 127.0.0.1 --directory "$APP_DIR" >/dev/null 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT INT TERM
i=0
while [ $i -lt 40 ]; do
  if command -v curl >/dev/null 2>&1 && curl -fsS "$URL" >/dev/null 2>&1; then break; fi
  sleep 0.1
  i=$((i + 1))
done
echo "SourceForge is running at $URL (Ctrl+C to stop)"
if command -v xdg-open >/dev/null 2>&1; then
  xdg-open "$URL" >/dev/null 2>&1 || true
fi
wait "$SERVER_PID"
LAUNCHER
chmod 0755 "$STAGE/usr/bin/sourceforge"

cat > "$STAGE/usr/share/applications/sourceforge.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=SourceForge
GenericName=Research workspace
Comment=Local-first research notebooks with grounded citations
Exec=sourceforge
Icon=sourceforge
Terminal=true
Categories=Office;Education;Utility;
Keywords=research;notebook;citations;study;offline;
StartupNotify=false
DESKTOP

INSTALLED_SIZE="$(du -sk "$STAGE/usr" | awk '{print $1}')"
cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: sourceforge
Version: $VERSION
Section: education
Priority: optional
Architecture: all
Installed-Size: $INSTALLED_SIZE
Depends: python3, xdg-utils
Maintainer: SourceForge <noreply@localhost>
Description: Local-first research workspace with grounded citations
 SourceForge is a static progressive web app for research notebooks: import
 PDFs, DOCX, PPTX, Markdown, CSV, GitHub files, web pages and transcripts;
 ask questions answered only from retrieved evidence with inline citations;
 generate study guides, flashcards, quizzes and audio scripts; export real
 Markdown, HTML, PDF, PPTX, CSV and JSON backup files.
 .
 All data stays in the browser's IndexedDB. AI features are optional and use
 keys you provide. This package serves the files from /usr/share/sourceforge
 on loopback with python3 and opens the default browser.
CONTROL

mkdir -p release
rm -f "$OUT"
dpkg-deb --root-owner-group --build "$STAGE" "$OUT" >/dev/null
echo "built $OUT ($(du -h "$OUT" | cut -f1))"

echo "==> inspecting"
dpkg-deb --info "$OUT" | sed -n '1,20p'
dpkg-deb --contents "$OUT" | awk '{print $6}' | sed -n '1,12p'

echo "==> verifying payload and launcher"
EXTRACT="$(mktemp -d)"
dpkg-deb --extract "$OUT" "$EXTRACT"
test -f "$EXTRACT/usr/share/sourceforge/index.html" || { echo "FAIL: index.html missing" >&2; exit 1; }
test -f "$EXTRACT/usr/share/sourceforge/sw.js" || { echo "FAIL: service worker missing" >&2; exit 1; }
test -f "$EXTRACT/usr/share/sourceforge/manifest.webmanifest" || { echo "FAIL: manifest missing" >&2; exit 1; }
test -f "$EXTRACT/usr/share/sourceforge/icons/icon-512.png" || { echo "FAIL: icons missing" >&2; exit 1; }
test -x "$EXTRACT/usr/bin/sourceforge" || { echo "FAIL: launcher not executable" >&2; exit 1; }
sh -n "$EXTRACT/usr/bin/sourceforge" || { echo "FAIL: launcher has a syntax error" >&2; exit 1; }
test -f "$EXTRACT/usr/share/applications/sourceforge.desktop" || { echo "FAIL: desktop entry missing" >&2; exit 1; }

echo "==> serving the packaged payload on loopback"
PORT=4191
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$EXTRACT/usr/share/sourceforge" >/dev/null 2>&1 &
HTTP_PID=$!
cleanup() { kill "$HTTP_PID" 2>/dev/null || true; rm -rf "$EXTRACT"; }
trap cleanup EXIT
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:$PORT/" | grep -q "SourceForge" || { echo "FAIL: index.html not served" >&2; exit 1; }
curl -fsS "http://127.0.0.1:$PORT/manifest.webmanifest" | grep -q '"name"' || { echo "FAIL: manifest not served" >&2; exit 1; }
curl -fsS "http://127.0.0.1:$PORT/sw.js" | grep -q "addEventListener" || { echo "FAIL: service worker not served" >&2; exit 1; }
ASSET="$(curl -fsS "http://127.0.0.1:$PORT/" | grep -o 'assets/index-[A-Za-z0-9_-]*\.js' | head -1)"
[ -n "$ASSET" ] || { echo "FAIL: no entry bundle referenced" >&2; exit 1; }
ASSET_BYTES="$(curl -fsS "http://127.0.0.1:$PORT/$ASSET" | wc -c)"
[ "$ASSET_BYTES" -gt 1000 ] || { echo "FAIL: entry bundle not served (${ASSET_BYTES} bytes)" >&2; exit 1; }
echo "OK: served /, manifest, sw.js and $ASSET from the packaged payload"
echo "package verification passed: $OUT"
