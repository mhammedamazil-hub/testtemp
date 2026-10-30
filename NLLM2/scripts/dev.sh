#!/usr/bin/env bash
# Build and run Forge locally.
#
#   bash scripts/dev.sh              # debug build, port 8080, data in ./data
#   FORGE_ADDR=0.0.0.0:8099 bash scripts/dev.sh
#
# Toolchain resolution order: $PATH, /tmp/forge-rust, $HOME/.cargo, /usr/local.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  for cand in /tmp/forge-rust/bin "$HOME/.cargo/bin" /usr/local/bin; do
    if [ -x "$cand/cargo" ]; then export PATH="$cand:$PATH"; break; fi
  done
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust (see docs/FROM-ZERO-GUIDE.md) or run scripts/install-rust.sh." >&2
  exit 1
fi

cargo build --offline
export FORGE_ADDR="${FORGE_ADDR:-0.0.0.0:8080}"
export FORGE_DATA_DIR="${FORGE_DATA_DIR:-data}"
export FORGE_WEB_DIR="${FORGE_WEB_DIR:-apps/web}"
echo "Forge: http://${FORGE_ADDR}"
exec ./target/debug/forge-server
