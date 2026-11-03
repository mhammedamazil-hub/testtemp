# ARTCRAFT STUDIO — Phase 1 source and integration audit

**Audit date:** 2026-11-03 (UTC; source checkouts refreshed)
**Status:** INVESTIGATING — source identity and license checks completed; Android build/integration feasibility remains unverified.
**Scope:** Phase 1 source audit completed; an initial Phase 2 Android host/build scaffold has been added. All seven canonical upstream repositories are shallow Git submodules under `upstream/`, pinned to the commits below. Upstream files remain unmodified. The Android APK workflow and host have not yet been built or runtime-tested.

## Executive summary

The requested product names correspond to seven distinct repositories in the `storytold` GitHub organization: PhotoCraft, VectorCraft, FilmCraft, LightCraft, PdfCraft, EffectCraft and DesignCraft. Each repository is a substantial Rust workspace with its own crates, editor frontend and command/automation components. Their upstream READMEs advertise native desktop applications and, for most/all of the projects, a browser/WebAssembly build. Their root trees do **not** provide an official Android application/package for the seven editors. PdfCraft contains vendored `winit` Android platform source, which is not itself an Android app or evidence that PdfCraft builds and runs on Android.

All seven are open source under the **dual license MIT OR Apache-2.0** according to their README and root license files. GitHub's repository metadata selects Apache-2.0 as the single displayed SPDX value; that metadata should not be mistaken for the only offered license. Bundled fonts, icons, model files, and other assets may have separate terms and need to be inventoried before redistribution.

**A single APK is not yet demonstrated.** An initial Android host now loads the actual upstream web/WASM frontend bundles from local assets, one editor at a time, using AndroidX WebViewAssetLoader; the GitHub Actions workflow is designed to build each upstream bundle and package one debug APK. This is a candidate route, not evidence that it builds or runs. It is not the native desktop frontend, and seven-engine compatibility, phone/tablet usability, memory, input, GPU/video behavior, APK size and project/file handoff remain unverified. No approach should be called integrated until the workflow succeeds and actual edit/save/export round trips pass on target Android hardware.

## Verified upstreams and pinned audit points

The following GitHub API lookups verified repository identity, `main` branch head, primary GitHub license metadata, README licensing statements, and root license files. The commit hashes pin the source snapshots inspected for this audit; they are not release tags and will move as upstream changes.

| Editor | Verified repository | Commit inspected | GitHub license metadata | License in README/files |
|---|---|---|---|---|
| PhotoCraft | [storytold/photocraft](https://github.com/storytold/photocraft) | `327c4cc82e737a3e0af7c147a477126d77525495` | Apache-2.0 | MIT OR Apache-2.0 |
| VectorCraft | [storytold/vectorcraft](https://github.com/storytold/vectorcraft) | `aaca80c961048161a3c7ab3ed8079727ac82b020` | Apache-2.0 | MIT OR Apache-2.0 |
| FilmCraft | [storytold/filmcraft](https://github.com/storytold/filmcraft) | `5163f259b5aba0526603c81e8be53e60ec44f1ac` | Apache-2.0 | MIT OR Apache-2.0 |
| LightCraft | [storytold/lightcraft](https://github.com/storytold/lightcraft) | `06c1b9ae0775464ce83ef0a9588be770337600f1` | Apache-2.0 | MIT OR Apache-2.0 |
| PDFCraft (upstream spelling: PdfCraft) | [storytold/pdfcraft](https://github.com/storytold/pdfcraft) | `3936f93a1479366a951206fc6d3094680fe2d3dc` | Apache-2.0 | MIT OR Apache-2.0 |
| EffectCraft | [storytold/effectcraft](https://github.com/storytold/effectcraft) | `7870b5916ae0749d497d880989683bf2946d6c8b` | Apache-2.0 | MIT OR Apache-2.0 |
| DesignCraft | [storytold/designcraft](https://github.com/storytold/designcraft) | `e79d682e02dc66f14370958b193bfa46072b2f41` | Apache-2.0 | MIT OR Apache-2.0 |

## Source checkout and agent-automation findings (2026-11-03)

All seven official repositories were shallow-cloned as Git submodules under `upstream/<editor>`. The parent repository records each exact submodule commit. To reproduce the checkout after cloning this project, use `git submodule update --init --recursive`. Current working-tree checkout sizes total about 342 MiB (excluding each submodule's `.git` history). The source remains independently licensed upstream code; submodule registration does not relicense it.

The new source review identifies useful engine-level automation that the initial audit did not record:

- All seven upstreams document command-based or MCP/CLI automation for their desktop/engine workflow. This makes them stronger candidates for typed adapters than projects requiring generic UI scripting.
- Existing web-facing automation is uneven. VectorCraft, FilmCraft, LightCraft and EffectCraft expose JavaScript control APIs in their web frontends (documented/API source exists); LightCraft documents `lightcraft.command(id, params)`. PdfCraft's web host bridge handles file handoff/save, not the full agent command catalog. DesignCraft explicitly says its web build has no control channel. PhotoCraft's browser build has no TCP control channel; it uses the same Rust UI and browser file dialogs/downloads, but a web-agent command bridge was not found in the inspected entry point.
- Therefore the shared AI can only safely register operations after each editor has a verified in-process web/engine adapter. MCP availability in a desktop CLI does not mean an Android WebView can directly call those tools. For PhotoCraft, PdfCraft and DesignCraft, adapter work is a real integration requirement, not a cosmetic UI task.
- Current source-documented web/WASM sizes vary. For example, upstream reports PhotoCraft around 23.9 MiB raw WASM, VectorCraft around 17.5 MB raw, LightCraft around 13.5 MB plus separately bundled fonts, and EffectCraft around 58 MB raw before optimization. These figures come from the upstream docs and are not our own build measurements; several other bundle sizes were not established in this checkout. Loading only one engine at a time is important, but APK size and Android runtime memory still need measurement.
- Several web builds rely on browser APIs such as WebGPU/WebGL2, workers, OPFS/IndexedDB and downloads. Android WebView support for each combination, including fallback behavior, needs device testing. Web bundles are actual editor engines/UIs rather than mockups, but their existence alone does not establish usable phone/tablet UX.

## Android and one-APK feasibility

### What source inspection establishes

- The repositories (now checked out as submodules under `upstream/`) are separate Rust Cargo workspaces (`crates/*`, `apps/*`, and often `xtask`) with distinct editor engines, frontends, file formats, native dependencies and test suites. They are not existing Gradle modules that can simply be added to one Android app.
- The upstream desktop frontends use Rust/egui and native window/GPU stacks. The web frontends target WebAssembly and can be useful as an integration route, but compiling to WebAssembly alone does not supply Android SAF, lifecycle, secure settings, system back navigation, memory-pressure handling, or reliable touch/stylus behavior.
- Tree inspection found no official Android app module, Android manifest, Gradle wrapper, or Android packaging directory in the seven canonical repositories. PdfCraft's vendored `winit` tree includes Android platform code, but that is a dependency implementation detail, not a tested PdfCraft Android target.
- Other GitHub forks advertise Android ports for some products. They are not the canonical `storytold` upstreams and were not established as maintained, license-clean, reproducible integrations for all seven editors. They are leads for further investigation only, not a basis for claiming the suite is integrated.

### Feasibility conclusion

**One APK: INVESTIGATING, not VERIFIED.** The repo now contains a minimal native host, seven-editor responsive launcher, SAF file-open/save bridge, provider settings screen, and a GitHub Actions pipeline intended to bundle the actual source WebAssembly apps. These are implementation artifacts only; the build has not run. The current prototype must prove one editor and a real project round trip before product claims. The host must not silently flatten native projects or substitute a web mock. Before scaling this design, validate:

1. Android-compatible build of the exact pinned source and dependency graph (including GPU/window and codec backends).
2. Touch, rotation, process death, SAF import/export and autosave in the actual editor.
3. Real editing, save, reopen and export against upstream tests/fixtures.
4. Per-editor peak memory and APK growth; only one editor engine loaded at a time.
5. Whether AI automation commands exist at the engine level, their supported typed parameter schema and safe Android access to those commands.
6. License and attribution inventory for transitive dependencies and bundled assets/models.

A desktop executable, generic WebView screenshot, static preview, or standalone upstream web demo is not sufficient evidence of an Android editor integration.

## Largest technical blockers

1. **No verified Android baseline.** None of the canonical sources provides an official Android APK/build recipe for the full suite. No Android SDK, Java/JDK, Gradle, Rust compiler or Cargo is installed in this workspace, and no emulator or physical Galaxy Tab A9 is attached. Therefore no Gradle/Rust build, APK install, ABI validation, or device test can be run here today.
2. **Seven independent large Rust workspaces.** Toolchain/dependency versioning, duplicate shared crates, linker settings, native libraries, resources and per-app build scripts need reconciliation. Combining package/module graphs or JNI entry points is not a no-op.
3. **GPU, windowing and media platform assumptions.** Editor UI/rendering stacks are designed/tested for desktop or browser targets. Android input, WebGPU/WebGL availability, backgrounding, codec availability and renderer recovery need direct tests. FilmCraft and EffectCraft in particular have resource-heavy playback/render pipelines.
4. **Memory and package size.** The primary 4 GB tablet target has limited headroom. No verified per-editor peak-RAM figures or combined APK-size measurements were found in this audit. Several editors ship extensive codecs, filters, fonts or model assets. Lazy loading may help but must be measured.
5. **Capability/API evidence.** Some upstreams advertise command registries, CLI automation or MCP. The suite still needs a supported, typed adapter per actual editor, capability discovery, validation, cancellation, checkpoints/undo and explicit approval for destructive operations. An MCP server existing on desktop does not automatically mean a suitable in-process Android API exists.
6. **Provider security and consent.** The Android scaffold now stores user-entered API keys with an Android Keystore-backed AES-GCM key, has provider/endpoint/model fields, and performs an explicit generic connection test. This is unbuilt and untested, and is not yet the shared AI workflow agent. The AI execution planner, per-editor typed adapters, token/usage handling, destructive-action approval, and per-project-data consent remain to be implemented.
7. **Licensing is not just root license files.** The core code is MIT OR Apache-2.0, but each asset/dependency/model needs its own notice and redistribution review. Product names/logos are not necessarily granted under the code license.

## Prioritized implementation plan

1. **Keep the seven official shallow submodules pinned.** Done for this iteration. `python3 scripts/verify_checkout.py` checks origin, commit, and root licenses. Next, inventory all transitive dependencies/assets and notices before release redistribution.
2. **Build the Android host/workflow.** A first native host, local WebViewAssetLoader shell, file-picker/export bridge, provider settings, and GitHub Actions workflow now exist. Trigger the workflow and fix actual build errors; none of these Android artifacts have yet been compiled in this environment.
3. **Run a one-editor device proof.** Use PhotoCraft first to verify Android WebView WASM startup, SAF image open, a real layer/edit operation, save/reopen, export, process/rotation behavior and memory on emulator/device. The workflow currently attempts all seven builds; do not treat a successful multi-bundle APK build as successful editing.
4. **Prove each editor's real web/mobile behavior.** Test phone portrait, phone landscape, tablet portrait/landscape, stylus/mouse, import/export, lifecycle and recovery. The launcher is responsive; the embedded upstream editors have not been redesigned or device-tested for mobile.
5. **Build adapters against genuine engine command surfaces.** VectorCraft, FilmCraft, LightCraft and EffectCraft have web-facing command APIs in current upstream; PhotoCraft, PdfCraft and DesignCraft need safe web/engine adapters. Inspect typed schemas and tests, then expose only tested capabilities in the registry.
6. **Complete the AI workflow layer.** Provider key configuration and a generic connection test are scaffolded. Add planning/tool calling, capability discovery, validation, checkpoints/undo, cancellation, logs, destructive-operation approval, usage reporting and explicit consent before project data leaves the device.
7. **Measure and validate.** Run import/export and editor integration suites, ABI/install tests, phone/tablet UI tests, GPU/media checks, startup, RAM and package-size checks; use a Galaxy Tab A9 if available.
8. **Release only verified behavior.** The debug APK, if produced by Actions, is a test artifact. No seven-editor Definition-of-Done claim until every editor passes real editing and save/reopen/export tests within one installed app.

## What can actually be built and tested in this environment

- **Completed now:** all seven upstream repositories shallow-cloned as exact pinned submodules; repository/license/build and web automation research; per-editor audits; initial native Android host and responsive launcher; Android SAF picker/export bridge source; provider settings with Keystore-backed encrypted key storage and explicit generic connection test source; workflow that builds the seven upstream WebAssembly frontends and packages a debug APK.
- **Executed checks here:** `python3 scripts/verify_checkout.py` passed for all seven origins, commits, and root license files; AndroidManifest XML parsed successfully. These do not compile or execute the app.
- **Workflow not yet run:** the GitHub Actions workflow has been authored but no hosted run/artifact is verified. This environment has no `rustc`/Cargo, Java/JDK, Gradle or Android SDK, so Android or upstream builds cannot run locally.
- **Not claimed:** no APK exists yet; no editor has passed a real Android edit/save/reopen/export test; provider connection test has not been run live; no shared AI editing agent exists; no Android/physical-device performance or mobile usability result is claimed.

## Audit files

- [PhotoCraft](AUDIT/photocraft.md)
- [VectorCraft](AUDIT/vectorcraft.md)
- [FilmCraft](AUDIT/filmcraft.md)
- [LightCraft](AUDIT/lightcraft.md)
- [PDFCraft / PdfCraft](AUDIT/pdfcraft.md)
- [EffectCraft](AUDIT/effectcraft.md)
- [DesignCraft](AUDIT/designcraft.md)
- [AI capability and provider status](AI_INTEGRATION.md)
- [Reproducible upstream pins](UPSTREAM_LOCK.json)
- [Android APK workflow](.github/workflows/android-apk.yml)
- [Project/build status](README.md)
