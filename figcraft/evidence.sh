#!/usr/bin/env bash
# Writes the headless FigCraft evidence log that lands in the repository.
# Run from the workflow after the design/export/MCP steps.
#   usage: evidence.sh <path-to-figcraft-cli> <appdir> <version>
set -euo pipefail
cli="${1:?cli path}"
appdir="${2:?appdir}"
fv="${3:?version}"

echo "# FigCraft headless run (Ubuntu runner, no display, no FUSE)"
echo
echo "The binary driven here is \`appdir/usr/bin/figcraft-cli\`, shipped inside the"
echo "published \`figcraft-${fv}-linux-x86_64.AppImage\` and unpacked with"
echo "\`--appimage-extract\` (checksum verified against the release SHA256SUMS.txt)."
echo "No window, no GPU: the engine's CPU renderer does the drawing."
echo
echo "## AppImage payload"
echo '```'
ls -l "$appdir/usr/bin"
echo '```'
echo
echo "## Version"
echo '```'
"$cli" --version
echo '```'
echo
echo "## Command surface"
echo '```'
python3 - <<'PY'
import json
cmds = json.load(open("work/commands.json"))
print(f"{len(cmds)} commands exposed")
for probe in ("node.create", "node.set", "text.set", "component.create",
              "layout.add", "inspect.code", "inspect.properties", "file.export"):
    hit = next((c for c in cmds if c["id"] == probe), None)
    print(f"  {probe:<18} {'yes' if hit else 'NO'}  {hit.get('label','') if hit else ''}")
PY
echo '```'
echo
echo "## Design script -> undo/confinement-checked run, then reopen + edit"
echo '```'
cat figcraft/out/design-run.jsonl
echo
cat figcraft/out/edit-run.jsonl
echo '```'
echo
echo "## MCP server (headless engine over stdio JSON-RPC)"
echo '```'
cat figcraft/out/mcp-smoke.log
echo '```'
echo
echo "## File stats"
echo '```'
cat figcraft/out/info.json
cat figcraft/out/info-v2.txt
echo '```'
