# Verification log

All commands below were executed in the build environment (2 vCPU, 3 GiB RAM, Debian 12,
Node 22, no Rust toolchain, no browser, no JDK). Results are recorded verbatim in
substance; nothing is inferred.

## Passed

| # | Command | Result |
|---|---|---|
| 1 | `npx tsc -b` | exit 0, no diagnostics |
| 2 | `npx vitest run` | 12 files, **159/159 tests passed** (≈11 s) |
| 3 | `npx vite build` | 831 modules transformed, `dist/` 8.33 MiB, 19 asset files |
| 4 | `node scripts/validate-build.mjs` | passed: manifest, SW, icons, relative paths, no secrets |
| 5 | `npx vite preview` + `curl` | `/`, `/manifest.webmanifest`, `/sw.js`, `/icons/icon-192.png` all HTTP 200 |
| 6 | `bash scripts/package-web.sh` | built + verified `sourceforge-web-0.1.0.zip` (2.2 MB) |
| 7 | `bash scripts/package-linux.sh` | built `.deb`, `dpkg-deb --info/--extract` OK, launcher syntax OK, payload served on loopback (`/`, manifest, `sw.js`, entry bundle) |
| 8 | `bash scripts/package-windows.sh` | built + content-verified portable ZIP (2.3 MB) |
| 9 | `npx cap add android` | generated `android/` project and copied `dist/` into web assets |
| 10 | `scripts/make-icons.mjs` | 4 valid PNGs (192/512/maskable/apple) verified by signature + IHDR |
| 11 | `npx rea-agents@6.8.0 analyze-javascript-application dist --source-path sw.js …` | exit 0; Evidence bundle with 441 AST nodes, 6 findings, no Electron/IPC surface |

The jsdom smoke test (`tests/app-smoke.test.tsx`) exercises the real component tree:
create notebook → open dialog → paste source → chunk → ask → citation chip → passage
viewer → study generation → settings → delete.

## Blocked / not executed

| # | Attempt | Observed failure |
|---|---|---|
| A | `cargo test --manifest-path core/Cargo.toml` | no `cargo` binary; `static.rust-lang.org` and `crates.io` unreachable (HTTP 000) |
| B | `bash scripts/build-wasm.sh` | exits with an explicit "Rust toolchain not found" message |
| C | `npx playwright install chromium` | `Download failure` (browser CDN unreachable); with `--with-deps` apt also fails (non-root) |
| D | `npm run test:e2e` | not run: needs the Chromium binary from (C) |
| E | `./gradlew assembleDebug` (android) | no JDK/Android SDK; Gradle distribution unreachable |
| F | Windows launcher execution | no Windows host and no PowerShell available |
| G | `cargo fmt` / `cargo clippy` | same as (A) |
| H | GitHub Pages deploy | workflow present; deployment requires pushing to `main` with Pages enabled |

## Interpretation

* The web artifact is real and verified up to "served over HTTP with all PWA files
  present". Visual/browser interaction was verified in jsdom (DOM + event flow), not in
  Chromium.
* The Linux package is real, unpacked and functionally served; it was not installed on a
  desktop session (no X server here).
* Rust and Android are **implemented and wired but unverified by execution**; the CI
  workflows are the designated place where they get compiled.
* No AI provider was contacted at any point; no API key exists in this repository or in
  any artifact (checked by the build validator and by `grep`).
