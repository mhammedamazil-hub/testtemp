# Rust core (WebAssembly) — build output directory

This directory is populated by:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack        # or: npm i -g wasm-pack
npm run wasm                   # runs cargo test, then wasm-pack build
```

That produces `sourceforge_core.js`, `sourceforge_core_bg.wasm` and
`sourceforge_core.d.ts` here. The app then loads the module at startup,
verifies chunking parity against the TypeScript implementation, and switches
its engine badge to **Rust/WASM**.

Without those files the app runs on the TypeScript implementation (identical
behaviour, verified by shared fixtures in `core/tests/fixtures/`). The build
output is intentionally not committed; CI (`.github/workflows/ci.yml`, job
`rust`) builds and uploads it.

Note: this directory must contain a real JS glue file for the loader to accept
it — a static host's HTML fallback is rejected by a content-type check.
