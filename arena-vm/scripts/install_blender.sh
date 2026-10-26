#!/usr/bin/env bash
# Install Blender as the official bpy wheel (free, from PyPI) into the venv.
# bpy bundles the full Blender binary + Python modules; GUI needs a display,
# rendering runs headless on CPU.
set -euo pipefail
source "$(dirname "$0")/env.sh"
BPY_VERSION="${1:-4.2.0}"
"$VM_VENV/bin/pip" install "bpy==$BPY_VERSION"
log "bpy $BPY_VERSION installed"
"$VM_VENV/bin/python" -c "import bpy; print('bpy version:', bpy.app.version_string)"
