#!/usr/bin/env bash
# Build the Rust core to WebAssembly and place it where the app loads it.
#
# Requirements: rustup, the wasm32-unknown-unknown target and wasm-pack.
# If they are missing this script fails loudly rather than pretending.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust: https://rustup.rs" >&2
  exit 1
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "error: wasm-pack not found. Install: cargo install wasm-pack (or cargo binstall wasm-pack)" >&2
  exit 1
fi

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  echo "error: wasm32-unknown-unknown target missing. Run: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

echo "==> cargo test (native)"
cargo test --manifest-path core/Cargo.toml

echo "==> wasm-pack build"
wasm-pack build core --target web --release --out-dir ../public/wasm --out-name sourceforge_core
rm -f public/wasm/.gitignore public/wasm/package.json public/wasm/*.ts public/wasm/README.md

echo "==> verifying output"
ls -la public/wasm
node --input-type=module -e "
import { readFileSync } from 'node:fs';
const js = readFileSync('public/wasm/sourceforge_core.js', 'utf8');
const wasm = readFileSync('public/wasm/sourceforge_core_bg.wasm');
if (!js.includes('chunk_text_json')) throw new Error('chunk_text_json export missing from glue');
if (wasm.length < 1000) throw new Error('wasm binary looks empty');
console.log('wasm module ready:', wasm.length, 'bytes');
"
