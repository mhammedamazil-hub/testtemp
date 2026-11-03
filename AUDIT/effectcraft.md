# EffectCraft source audit

**Status:** INVESTIGATING (source/license identified; Android integration and local builds unverified).
**Pinned source:** [`storytold/effectcraft`](https://github.com/storytold/effectcraft/tree/7870b5916ae0749d497d880989683bf2946d6c8b) · commit `7870b5916ae0749d497d880989683bf2946d6c8b` on `main` (shallow submodule checked out 2026-11-03; not a release tag).

1. **Verified repository:** https://github.com/storytold/effectcraft . Canonical EffectCraft source.
2. **Exact commit/version:** `7870b5916ae0749d497d880989683bf2946d6c8b`; no semantic release tag inferred.
3. **License:** Dual MIT OR Apache-2.0 per README and root license files; GitHub metadata selects Apache-2.0. Third-party assets, fonts and any optional model files require separate notice and redistribution review.
4. **Main languages:** Rust Cargo workspace; WebAssembly/browser frontend documented. Rust 1.95+ declared.
5. **Build instructions:** Upstream documents `cargo run --release -p effectcraft`, `cargo run --release -p effectcraft-cli -- render ...`, workspace tests and `cargo xtask ci`; web builds are described. Not executed here. No canonical Android Gradle/APK build recipe found.
6. **Android compatibility:** Desktop and web/WASM support advertised; no official Android app module or package was located. GPU compositor, video decode, animation/render queue, touch input and application lifecycle compatibility on Android are unverified.
7. **Existing editing functionality:** The web frontend exposes `window.effectcraft` command/control operations, project/file access and render status. Its browser build uses WebAssembly and web workers for heavy operations, with documented single-thread-per-instance constraints and a large raw WASM bundle. Keep this editor exclusive/lazy-loaded and verify Android WebView worker/GPU/video behavior. Upstream describes compositions, layers, keyframes, animation, effects, transforms, text/shapes/media, rendering and Lottie/project interchange. These are upstream claims and not yet Android-tested.
8. **Dependencies:** Independent workspace includes composition/model/project/time, effects, render/GPU, raster/media, export/codec, text, scripting, automation and app crates. Specific third-party dependency and model sizes/licensing still require a locked dependency and asset inventory. Rust >=1.95.
9. **Memory requirements:** No verified Android figures. Composition caches, full-resolution frames, video sources and effect graphs can consume substantial memory; need measured preview/render peaks and graceful low-memory behavior.
10. **Integration strategy:** Defer until the host, lifecycle and one editor are proven. Use a genuine upstream engine/frontend target, load only when selected, and test composition creation, layer/keyframe edit, save/reopen, deterministic render/export, pause/resume, cancellation and memory pressure. Register typed commands only if their engine adapter and undo/validation are verified.
11. **Known blockers:** Android packaging absent; GPU/window and video backend compatibility unknown; large media/effects increase RAM and APK burden; model availability/terms; no tested Android command adapter; playback/render cancellation and backgrounding unknown.
12. **Test evidence:** GitHub API verified repository, commit, root licenses, workspace and README build statements. No cargo tests/build, render/export, Android APK, emulator or device memory test was performed here. Local test status: **NOT RUN**.

**Capability registry status:** No EffectCraft operations are registered as tested.
