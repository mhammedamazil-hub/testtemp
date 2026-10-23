# Stonewild

Stonewild is an original, clean-room C++ voxel exploration project for Linux laptops, Android phones, and tablets. It contains no Minecraft code, decompiled implementation, names, textures, sounds, UI, or other proprietary assets. Current art is generated from original colors, geometry, and shader math.

This repository contains a tested **engine foundation and playable build/place slice**, not a claim of complete Minecraft feature parity. It currently provides:

- deterministic chunked terrain with hills, water, caves, ores, and trees;
- block breaking and placing through voxel ray casting;
- first-person collision, gravity, jumping, and sprinting;
- a nine-slot block hotbar and highlighted target block;
- persistent player state and block edits over a procedural world;
- bounded chunk streaming and revision-driven, face-culled meshes;
- procedural material variation, ambient occlusion, directional light, and distance fog;
- Linux keyboard/mouse and SDL gamepad controls;
- Android touch, keyboard, and gamepad controls;
- GitHub Actions workflows for a Linux Mint `.deb` and Android debug APK.

## Linux Mint package

The desktop build targets 64-bit Linux Mint 21 or newer. The CI-produced package is installed with:

```bash
sudo apt install ./stonewild_0.2.0_amd64.deb
```

Launch it from the application menu or run:

```bash
stonewild
```

The package depends only on standard C++/OpenGL runtime libraries and SDL2. It installs the game executable, desktop-menu entry, original Stonewild icon, and documentation. Saves are written to:

```text
~/.local/share/stonewild/
```

### Build locally

Install development dependencies on Linux Mint or Ubuntu:

```bash
sudo apt install build-essential cmake ninja-build libsdl2-dev libgl1-mesa-dev dpkg-dev
cmake -S stonewild -B build/stonewild -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build/stonewild --parallel 2
ctest --test-dir build/stonewild --output-on-failure
cpack --config build/stonewild/CPackConfig.cmake -G DEB
```

Use `--parallel 2`, rather than every CPU core, when compiling on a 4 GB machine.

## 4 GB low-memory support

At startup, Stonewild reads total system memory. Systems with 6 GB or less automatically select the **Low-memory** profile:

- quality level 0 and a one-chunk visible radius;
- only a 5×5 staging area generated at startup;
- byte-sized block storage (24 KiB of raw blocks per chunk);
- at most one new/revised chunk mesh uploaded per frame;
- distance culling before draw calls and fog matched to visible distance;
- CPU mesh memory released immediately after GPU upload;
- old world chunks and their GPU buffers removed as the player travels;
- 60 FPS VSync pacing instead of an unrestricted render loop.

The window title reports FPS, process RAM, GPU mesh memory, loaded chunks, active profile, and selected block. Runtime profiles can also be forced:

```bash
stonewild --low
stonewild --balanced
stonewild --high                 # 8 GB+ and a stronger GPU recommended
stonewild --render-distance 2 --quality 1
```

A 4 GB target does not guarantee performance on every graphics chipset, but `--low` is deliberately bounded and software-OpenGL smoke-tested in CI.

## Computer controls

| Action | Keyboard and mouse | Gamepad |
|---|---|---|
| Move | `W A S D` or arrow keys | Left stick |
| Look | Mouse | Right stick |
| Jump | `Space` | A |
| Sprint | Left/right `Shift` | Left-stick press |
| Break targeted block | Left mouse | Right shoulder |
| Place selected block | Right mouse | Left shoulder |
| Select hotbar | `1`–`9` or mouse wheel | D-pad left/right |
| Release/capture mouse | `Esc` / click | Back |
| Cycle visual detail | `F2` | — |
| Cycle render distance | `F3` | — |
| Save immediately | `F5` | — |
| Fullscreen | `F11` | — |
| Quit | `Ctrl+Q`, or `Esc` after releasing mouse | — |

The game automatically saves every 30 seconds and again during a clean exit.

## Portable engine tests

The engine core has no third-party dependencies:

```bash
g++ -std=c++20 -O2 -Wall -Wextra -Wpedantic -Werror \
  -Istonewild/engine/include \
  stonewild/engine/src/*.cpp stonewild/engine/tests/test_engine.cpp \
  -o /tmp/stonewild-tests
/tmp/stonewild-tests
```

To build tests without desktop dependencies:

```bash
cmake -S stonewild -B /tmp/stonewild-core \
  -DSTONEWILD_BUILD_DESKTOP=OFF
cmake --build /tmp/stonewild-core --parallel 2
ctest --test-dir /tmp/stonewild-core --output-on-failure
```

## Android

The Android shell is a C++ `NativeActivity` using EGL and OpenGL ES 3. It targets Android 8.0/API 26 or newer, arm64-v8a and x86_64. A local build needs JDK 17, Gradle 8.9, Android SDK 35, NDK `28.2.13676358`, and CMake 3.22.1:

```bash
cd stonewild/android
gradle :app:assembleDebug
```

The APK is produced at `stonewild/android/app/build/outputs/apk/debug/app-debug.apk`. The Android workflow compiles both native ABIs, checks the APK signature, verifies both libraries, and uploads the debug APK.

## Validation boundary

Automated tests cover deterministic terrain, coordinate math, streaming, persistent edits, ray casting, mesh output, collision, player physics, low-memory startup, and save round trips. Linux CI also launches the actual desktop renderer for 90 frames under Mesa software OpenGL before packaging and inspecting the `.deb`.

A successful build and automated smoke test do not replace hands-on testing across real Linux laptops, graphics drivers, Android devices, and controllers. Inventory, crafting, survival statistics, creatures, automation, multiplayer, and creator tools remain phased work in `ROADMAP.md`.
