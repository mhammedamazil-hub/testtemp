# Stonewild

Stonewild is an original, clean-room C++ voxel-survival project for phones and tablets. It does not contain Minecraft code, decompiled implementation, names, textures, sounds, UI, or other proprietary assets. Familiar genre mechanics are implemented independently and all current visuals are generated from original colors and shader math.

This repository currently contains the **engine foundation**, not a claim of complete Minecraft feature parity. The first playable slice provides:

- deterministic chunked terrain with hills, water, caves, ores, and trees;
- block breaking and placing through voxel ray casting;
- a first-person player controller with gravity, jumping, collision, and sprinting;
- a nine-slot block hotbar model;
- persistent player state and block edits over a procedural world;
- face-culled chunk meshes;
- an OpenGL ES 3 renderer with procedural material variation, directional light, fog, and three quality levels;
- Android touch controls plus keyboard/gamepad input paths;
- bounded chunk streaming suitable for memory-constrained devices;
- a GitHub Actions workflow that runs native core tests and builds a debug APK artifact.

## Build the portable engine tests

The engine core has no third-party dependencies:

```bash
g++ -std=c++20 -O2 -Wall -Wextra -Werror \
  -Istonewild/engine/include \
  stonewild/engine/src/*.cpp stonewild/engine/tests/test_engine.cpp \
  -o /tmp/stonewild-tests
/tmp/stonewild-tests
```

With CMake 3.20+:

```bash
cmake -S stonewild -B /tmp/stonewild-build -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/stonewild-build -j2
ctest --test-dir /tmp/stonewild-build --output-on-failure
```

## Android

The Android shell is a C++ `NativeActivity` using EGL and OpenGL ES 3. It targets Android 8.0/API 26 or newer, arm64-v8a and x86_64.

A local Android build needs JDK 17, Gradle 8.9, Android SDK 35, NDK `28.2.13676358`, and CMake 3.22.1:

```bash
cd stonewild/android
gradle :app:assembleDebug
```

APK output:

```text
stonewild/android/app/build/outputs/apk/debug/app-debug.apk
```

The sandbox used to create this foundation has no Android SDK, so Android compilation is delegated to `.github/workflows/stonewild-android.yml`. The workflow contains no signing secret and publishes an Android debug-key-signed APK artifact. CI compiles both native ABIs, checks the APK signature, and verifies that both native libraries were packaged.

## Validation status

- The portable core passes 5,248 assertions under `-Wall -Wextra -Wpedantic -Werror` locally and in GitHub Actions.
- The GitHub Actions Android build has completed successfully for both arm64-v8a and x86_64 and uploaded the debug APK.
- Installation and hands-on play testing still require a physical Android device or emulator; neither is available in the local sandbox, so runtime behavior is not claimed as device-tested yet.

## Controls

Landscape touch layout:

- left circle: movement;
- drag the right half: look;
- upper-right circle: jump;
- lower-right red circle: break;
- lower-right blue circle: place.

Hardware keyboard/gamepad paths are also recognized. Touch controls are drawn by the native renderer rather than depending on Android views.

## Performance strategy

- Fixed 16×96×16 chunks and a small default render radius.
- Only exposed block faces enter meshes.
- GPU meshes rebuild only when a chunk revision changes.
- Procedural fragment detail avoids large texture downloads.
- Quality 0 removes fragment detail; quality 1 adds low-cost variation; quality 2 adds finer variation and stronger fog grading.
- Old chunks are unloaded outside a bounded radius.

See `ROADMAP.md` for the explicit feature plan and non-goals.
