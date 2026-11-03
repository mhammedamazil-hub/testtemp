# DesignCraft source audit

**Status:** INVESTIGATING (source/license identified; Android integration and local builds unverified).
**Pinned source:** [`storytold/designcraft`](https://github.com/storytold/designcraft/tree/e79d682e02dc66f14370958b193bfa46072b2f41) · commit `e79d682e02dc66f14370958b193bfa46072b2f41` on `main` (shallow submodule checked out 2026-11-03; not a release tag).

1. **Verified repository:** https://github.com/storytold/designcraft . Canonical DesignCraft source.
2. **Exact commit/version:** `e79d682e02dc66f14370958b193bfa46072b2f41`; no semantic release tag inferred.
3. **License:** Dual MIT OR Apache-2.0 per README and root license files; GitHub API metadata selects Apache-2.0. The README notes ArtCraft name/logos are trademarks with separate brand terms; fonts, bundled images and dependencies require individual review.
4. **Main languages:** Rust Cargo workspace with web/WASM frontend. Workspace's UI and shared craft-ui require Rust 1.95+ according to upstream.
5. **Build instructions:** Upstream documents `cargo run --release -p designcraft`, `cargo run --release -p designcraft-cli ...`, `cargo xtask ci`; some bundled fonts may be supplied using `CRAFT_FONTS_DIR`. These are not locally run. No Android APK/Gradle recipe found in canonical tree.
6. **Android compatibility:** Upstream advertises desktop/web, not Android. No official Android app module, Gradle project, manifest or Android package was found. Page-layout UI, text shaping, fonts, export and touch/keyboard transitions need real Android validation.
7. **Existing editing functionality:** The web frontend is a genuine Rust/egui WebAssembly editor, but upstream explicitly states that the web build has no control channel; save/export downloads files. Desktop engine/CLI/MCP automation exists, but an Android-integrated web/engine adapter will be needed for agent execution. Upstream describes multi-page/spread layout, parent/master pages, text frames and threaded stories, typography/styles, images/shapes, grids, alignment, IDML import/export, PDF/PNG and other publishing formats. Export fidelity and editing capabilities are upstream features, not verified on Android.
8. **Dependencies:** Separate Rust workspace with geometry/color/document, text composition/font shaping, render/tools/engine, PDF/IDML/PPTX/EPUB, app/CLI/web crates. Rust >=1.95; fonts and bundled assets have separate licenses. Complete dependency/asset inventory and Android compatibility remain to do.
9. **Memory requirements:** No published/verified Android peak RAM. Long multi-page documents, images, embedded fonts and typography composition need device measurement and low-memory recovery tests.
10. **Integration strategy:** Consider after document/vector integrations have a working host. Test actual page/frame model, touch selection/resize, keyboard/stylus interactions, document autosave/reopen and PDF export. Use real `.idml`/native project interchange where supported; do not flatten an editable document silently. Require tested command registry for any AI-driven layout operations.
11. **Known blockers:** No official Android app; heavy typography/font dependencies; optional font sourcing and license attribution; file format and SAF bridging; large page-document memory; automation interface has no established Android adapter.
12. **Test evidence:** GitHub API verified repo, pinned commit, licenses, Cargo workspace and build instructions/features from README. No cargo tests/build, rendered page export, APK, emulator or device test was performed here. Local test status: **NOT RUN**.

**Capability registry status:** No DesignCraft operations are registered as tested.
