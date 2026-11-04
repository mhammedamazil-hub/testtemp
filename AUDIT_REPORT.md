# ARTCRAFT STUDIO — upstream audit and native Android status

**Audit date:** 2026-11-03; native host work updated 2026-11-04.
**Status:** Native Rust/Android integration is now the target. The native host and cross-build workflow are source scaffolding only; compile/runtime success is not yet established.

## Product target

One installable Android APK containing the seven authentic upstream editors—PhotoCraft, VectorCraft, FilmCraft, LightCraft, PdfCraft, EffectCraft and DesignCraft—running locally from Rust. The editor UI is native egui through eframe/winit. No WebView, editor website, HTML/JavaScript editor, or WebAssembly payload is part of the intended APK.

The earlier WebView/WASM APK and its failed smoke-test runs are not product candidates. The current workflow has been replaced with a Rust Android cross-build and NativeActivity packaging attempt.

## Upstream identities and pins

All seven official `storytold` projects are pinned as Git submodules under `upstream/`; `UPSTREAM_LOCK.json` records their source hashes. `python3 scripts/verify_checkout.py` validates the pinned origins, commits and root licenses. Each project is a substantial Rust workspace with independent engine/UI crates and native desktop applications. Their canonical trees did not contain a complete Android app for the suite when audited.

| Editor | Repository | Pinned commit | Upstream license |
|---|---|---|---|
| PhotoCraft | [storytold/photocraft](https://github.com/storytold/photocraft) | `327c4cc82e737a3e0af7c147a477126d77525495` | MIT OR Apache-2.0 |
| VectorCraft | [storytold/vectorcraft](https://github.com/storytold/vectorcraft) | `aaca80c961048161a3c7ab3ed8079727ac82b020` | MIT OR Apache-2.0 |
| FilmCraft | [storytold/filmcraft](https://github.com/storytold/filmcraft) | `5163f259b5aba0526603c81e8be53e60ec44f1ac` | MIT OR Apache-2.0 |
| LightCraft | [storytold/lightcraft](https://github.com/storytold/lightcraft) | `06c1b9ae0775464ce83ef0a9588be770337600f1` | MIT OR Apache-2.0 |
| PdfCraft | [storytold/pdfcraft](https://github.com/storytold/pdfcraft) | `3936f93a1479366a951206fc6d3094680fe2d3dc` | MIT OR Apache-2.0 |
| EffectCraft | [storytold/effectcraft](https://github.com/storytold/effectcraft) | `7870b5916ae0749d497d880989683bf2946d6c8b` | MIT OR Apache-2.0 |
| DesignCraft | [storytold/designcraft](https://github.com/storytold/designcraft) | `e79d682e02dc66f14370958b193bfa46072b2f41` | MIT OR Apache-2.0 |

The repository root licenses do not clear bundled fonts, icons, models, codecs or other third-party assets. Notices remain a release requirement.

## Native implementation attempt

- `native-android/` is a Rust `cdylib` entry point using the upstream-pinned eframe/winit Android NativeActivity support.
- It depends directly on all seven upstream `*-ui-egui` and engine crates, constructs their Rust app/UI types, and provides an egui-native editor launcher.
- `app/` now packages that Rust library as a NativeActivity APK. The old WebView/Kotlin editor host and web launcher have been removed from the Android build.
- `.github/workflows/android-apk.yml` cross-compiles for ARM64 and x86_64, packages a debug APK, checks for native libraries/no editor web payload, uploads the APK before smoke testing, then attempts a native emulator first-frame check.

**No successful native Rust cross-compile, APK install, emulator first frame, or physical-device use has yet been verified.** Adding the native source is not equivalent to a working APK. The next evidence must come from the new workflow and then an actual device test.

## Important integration gaps

1. The native app currently uses default upstream host services. Android Storage Access Framework (SAF) file picking, URI grants, save/export flows and document lifecycle/recovery are not integrated or verified.
2. Android-specific input, IME, status/navigation bars, touch/stylus pressure, GPU/backend initialization, rotation, pause/resume and low-memory behavior are unverified.
3. Video/audio codecs, hardware acceleration, camera/RAW import, font discovery and other desktop service assumptions may need Android-native adapters. No feature parity claim is made.
4. The native UI intentionally uses the upstream egui editor types. It is not a mock implementation, but each editor still needs build/startup and real editing/save/reopen/export tests on Android.
5. The prior visual agent implementation was WebView-specific and has been removed from the app build. A native agent is not included in this Rust host. Any future agent must preserve explicit screenshot consent, user-approved plans, bounded visible actions, an obvious cursor/progress indicator, stop/cancel behavior and safe diagnostics; it must not silently send project content or API keys.
6. No physical Android device was tested. No Galaxy Tab A9 result, memory/thermal measurement, release signing, store-ready APK or complete seven-editor Definition-of-Done claim is available.

## Verification status

- `python3 scripts/verify_checkout.py`: upstream pin verification is available; rerun after checkout.
- Rust syntax/type-check: **not run locally** (this environment has no Rust/Cargo toolchain).
- Android Gradle/APK build: **not run locally** (no Java/Gradle/Android SDK installed).
- Native emulator first-frame smoke: **pending the new GitHub Actions workflow**.
- SAF round-trips, real editor operations, export verification and device testing: **not run**.

The first milestone is a clean native compile and a visible first frame from the Rust NativeActivity. Passing that smoke test would still not prove the editors' real workflows. After that, integrate Android-native storage and validate each editor's real project round-trip before treating the APK as usable.
