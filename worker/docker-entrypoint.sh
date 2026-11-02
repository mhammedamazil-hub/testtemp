#!/usr/bin/env bash
set -euo pipefail

if [[ "${DOWNLOAD_MODELS:-1}" == "1" ]]; then
  /usr/local/bin/download_models.sh
fi

# --listen is required for the studio container to reach the worker.
# ComfyUI's own UI is intentionally not published to the host by compose.
exec python3 main.py --listen 0.0.0.0 --port 8188 --disable-auto-launch "$@"
