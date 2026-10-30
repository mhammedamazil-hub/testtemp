#!/usr/bin/env bash
# Fetch the public reverse-engineering evidence used to rebuild the NotebookLM
# wire protocol, and record exact commit hashes + sha256 so the claims in
# docs/NOTEBOOKLM_PROTOCOL.md stay auditable.
#
# Nothing here touches Google. It downloads *other people's public documentation*
# from the GitHub contents API (raw.githubusercontent is not allowlisted).
#
# Usage: bash scripts/fetch_notebooklm_evidence.sh [outdir]
set -euo pipefail
OUT="${1:-docs/rea}"
mkdir -p "$OUT"

PIN="956400aa7af6b61b4cb988a24c9a081a3abc4ee8"   # teng-lin/notebooklm-py
fetch() { # repo, path, outfile
  python3 - "$1" "$2" "$OUT/$3" <<'PY'
import sys, json, base64, pathlib, urllib.request
repo, path, out = sys.argv[1:4]
url = f"https://api.github.com/repos/{repo}/contents/{path}?ref={path.split('@')[-1]}" if '@' in path else \
      f"https://api.github.com/repos/{repo}/contents/{path}"
if '@' in path:
    path, ref = path.split('@')
    url = f"https://api.github.com/repos/{repo}/contents/{path}?ref={ref}"
req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "User-Agent": "forge-research"})
d = json.load(urllib.request.urlopen(req, timeout=90))
data = base64.b64decode(d["content"])
pathlib.Path(out).write_bytes(data)
print(f"  {path} -> {out} ({len(data)} bytes)")
PY
}

echo "fetching evidence into $OUT"
fetch teng-lin/notebooklm-py "docs/rpc-reference.md@$PIN"      rpc-reference.md
fetch teng-lin/notebooklm-py "docs/web-android-public-behavior.md@$PIN" web-android-public-behavior.md
fetch jacob-bd/gemini-notebook-mcp-cli "src/notebooklm_tools/core/utils.py" third-party-jacob-bd-utils.py
fetch jackwener/OpenCLI "clis/notebooklm/add-source.js" third-party-opencli-add-source.js

{
  echo "# Evidence manifest"
  echo
  echo "Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/fetch_notebooklm_evidence.sh"
  echo
  echo "* teng-lin/notebooklm-py pinned at \`$PIN\`"
  echo "* other repos fetched at their default branch HEAD (corroboration only)"
  echo
  echo '```'
  ( cd "$OUT" && sha256sum *.md *.py *.js 2>/dev/null )
  echo '```'
} > "$OUT/MANIFEST.md"

echo
cat "$OUT/MANIFEST.md"
