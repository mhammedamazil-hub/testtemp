# LightCraft source audit

**Status:** INVESTIGATING (source/license identified; Android integration and local builds unverified).
**Pinned source:** [`storytold/lightcraft`](https://github.com/storytold/lightcraft/tree/06c1b9ae0775464ce83ef0a9588be770337600f1) · commit `06c1b9ae0775464ce83ef0a9588be770337600f1` on `main` (shallow submodule checked out 2026-11-03; not a release tag).

1. **Verified repository:** https://github.com/storytold/lightcraft . Canonical LightCraft source.
2. **Exact commit/version:** `06c1b9ae0775464ce83ef0a9588be770337600f1`; no semantic release tag inferred.
3. **License:** Dual MIT OR Apache-2.0 per README and root license files; GitHub API chooses Apache-2.0 as metadata. Bundled profiles, fonts, sample images, models and other assets require their own attribution/redistribution review.
4. **Main languages:** Rust workspace; browser/WASM build and UI documented. Rust 1.95+ is documented for workspace builds.
5. **Build instructions:** Upstream documents `cargo run --release -p lightcraft`, CLI rendering (for example `cargo run --release -p lightcraft-cli -- render ...`) and `cargo xtask ci`; web target is also documented. Not run in this environment. No Android Gradle or APK instruction found in canonical tree.
6. **Android compatibility:** Native desktop and web/WASM are advertised; no official Android app module or Android packaging was identified. Android camera RAW/file provider, storage, GPU and background task compatibility is unverified.
7. **Existing editing functionality:** The web frontend exposes `lightcraft.command(id, params)` and uses the same UI/engine in WebAssembly. Its docs specify WebGL2 rendering, browser storage (OPFS or IndexedDB fallback), worker rendering and browser file export. It has no native Android storage integration; host consent/import/export bridging and browser storage limits need testing. Upstream describes non-destructive photo library and RAW development: exposure/color/white-balance adjustments, curves, masks, local adjustments, presets, RAW decoders, metadata/library organization and export. Format support varies by build features and must be tested; no universal RAW claim is made here.
8. **Dependencies:** Independent Cargo workspace with RAW/TIFF/raster/codecs/color/develop/pipeline/catalog/engine/UI and optional feature/model-related crates; exact dependency feature graph and redistribution requirements need inventory. Rust >=1.95. Android target support was not found.
9. **Memory requirements:** No verified Android memory figures. High-resolution RAW decode, 32-bit processing, previews, catalog caches and export need measured peak RAM; avoid claiming support for large RAWs on a 4 GB class device without testing.
10. **Integration strategy:** After Android host and SAF support are proven, test this editor as a lazy-loaded engine with Android document URIs copied or streamed safely into its storage model. Preserve original RAWs and non-destructive project/edit metadata; test a RAW import, adjustment, save/reopen and exported image against fixtures. Evaluate existing automation command surfaces before making an AI adapter.
11. **Known blockers:** No Android app/build recipe; Android storage/catalog design unknown; RAW decoding can be expensive; optional codecs/models/profile assets have separate availability/licensing concerns; memory and thermal performance unknown; AI action API is not yet validated for Android.
12. **Test evidence:** GitHub API verified repository, commit, license files, workspace and README-described commands/features. No cargo tests/build, RAW round trip, APK, emulator or device test was run here. Local test status: **NOT RUN**.

**Capability registry status:** No LightCraft operations are registered as tested.
