# ARTCRAFT STUDIO

ArtCraft Studio is being rebuilt as a **native Android Rust app**. The Android process runs `eframe`/`winit` through `NativeActivity` and links the seven pinned upstream `*-ui-egui` frontends and their Rust engine crates directly into one Rust shared library. The APK is not intended to contain an editor website, WebView, HTML, JavaScript or WebAssembly.

The launcher and editor selection are Rust/egui. Each selected editor is created from its upstream Rust UI and engine API: PhotoCraft, VectorCraft, FilmCraft, LightCraft, PdfCraft, EffectCraft and DesignCraft. Upstream sources remain pinned Git submodules; they are not copied or substituted with mock editors.

## Status — native port in progress

- **Native Android host:** source scaffold is in `native-android/`; the current build attempts to cross-compile the seven upstream Rust UI/engine crates for Android ARM64 and x86_64. A successful cross-compile and emulator first-frame test have not yet been reported.
- **APK:** no working native APK is claimed yet. The earlier WebView/WASM debug APK is not the requested deliverable and is no longer built by the workflow.
- **Device/file integration:** Android SAF import/export, lifecycle recovery, media permissions, audio/video routing and touch/stylus behavior are not yet validated. Default upstream service hooks are currently used; treat editing and file round-trips as unverified until they are tested.
- **Editor functionality:** the intended integration reuses authentic upstream Rust/egui editor UIs and Rust engines. It does not convert the editors to HTML or JavaScript. Feature parity on Android is not claimed until each native editor and its platform services are verified.
- **Privacy:** editor execution is intended to be local. The native host adds no hosted editor or browser dependency; network behavior in upstream features (for example optional update/model services) still needs review. Provider-backed AI and the earlier WebView cursor agent are not part of this native build.
- **Upstream source pins:** all seven official repositories are recorded in `UPSTREAM_LOCK.json`; run `python3 scripts/verify_checkout.py` after `git submodule update --init --recursive` to verify them.

## Build and verify

The **Native Android APK** workflow cross-compiles the Rust library for Android, packages it in a single debug APK, checks for the native shared libraries and absence of web/WASM editor payloads, uploads the APK, and attempts an emulator first-frame test. The APK artifact is only a native candidate; a successful build is not proof that all seven editors are usable.

For a local source checkout:

```sh
git submodule update --init --recursive
python3 scripts/verify_checkout.py
```

The Android project is under `app/`; the Rust Android host is under `native-android/`. The workflow uses JDK 17, Android SDK/build-tools 35, NDK 27.2, Rust 1.95, and Gradle 8.11.1. Release signing is not configured. Upstream and bundled third-party licenses still apply.

## Important limitations

No native Android build, emulator run, physical-device test, or Galaxy Tab A9 test is claimed until the new workflow completes and device validation is performed. Passing a native first-frame smoke test alone would not prove file open/save, export, media editing, recovery, memory behavior, or touch usability.
