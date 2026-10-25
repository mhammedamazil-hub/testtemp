#!/usr/bin/env bash
# stage-static.sh — assemble a self-contained static bundle (no backend).
#
# The Blender wasm runtime is ~285 MB and is NOT committed to Git. This script
# copies it from a local kit directory so the bundle stays reproducible without
# bloating the repository. Run this, then serve the output directory with any
# static host.
#
#   ./deploy/stage-static.sh /path/to/blender-web-local /tmp/bw-static
#
set -euo pipefail

KIT="${1:?usage: stage-static.sh <blender-web-local dir> [out dir]}"
OUT="${2:-$(mktemp -d /tmp/bw-static.XXXXXX)}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$HERE/.." && pwd)"

for f in bin/blender_browser.js bin/blender_browser.wasm bin/blender_browser.data; do
  [[ -f "$KIT/$f" ]] || { echo "missing $KIT/$f" >&2; exit 1; }
done

mkdir -p "$OUT/bin" "$OUT/static"
cp "$KIT/bin/blender_browser.js" "$OUT/bin/"
# wasm + data are large; link them instead of copying when possible so staging
# is fast and does not double disk usage.
ln -sf "$(cd "$KIT" && pwd)/bin/blender_browser.wasm" "$OUT/bin/blender_browser.wasm" 2>/dev/null \
  || cp "$KIT/bin/blender_browser.wasm" "$OUT/bin/"
ln -sf "$(cd "$KIT" && pwd)/bin/blender_browser.data" "$OUT/bin/blender_browser.data" 2>/dev/null \
  || cp "$KIT/bin/blender_browser.data" "$OUT/bin/"

# Runtime support files that Blender expects under BLENDER_SYSTEM_* paths.
for d in python scripts datafiles; do
  [[ -d "$KIT/$d" ]] && ln -sfn "$(cd "$KIT" && pwd)/$d" "$OUT/bw/$d" || true
done
[[ -d "$OUT/bw" ]] || mkdir -p "$OUT/bw"

cp "$APP/deploy/static/index.html" "$OUT/"
cp "$APP/deploy/static/static-app.js" "$OUT/static/"
cp "$APP/deploy/static/coi-serviceworker.min.js" "$OUT/" 2>/dev/null || true
cp "$APP/app/web/glb.js" "$OUT/static/glb.js"
cp "$APP/deploy/static/static.css" "$OUT/static/static.css"
cp "$APP/harness/bw_ops.py" "$OUT/bw_ops.py"

# Host config files for the two supported targets. Neither reads them, but they
# document intent for anyone deploying the bundle.
cp "$APP/deploy/static/_headers" "$OUT/_headers" 2>/dev/null || true
cp "$APP/deploy/static/_redirects" "$OUT/_redirects" 2>/dev/null || true

echo "$OUT"
du -sh "$OUT" 2>/dev/null || true
