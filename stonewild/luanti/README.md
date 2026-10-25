# Stonewild Realms — Luanti migration

Stonewild Realms is an original voxel-survival game overlay for the open-source **Luanti 5.17.0** engine. This directory replaces the legacy experimental renderer as the active visual direction while retaining that older implementation as a fallback.

## What this phase contains

- realistic original 512 px terrain materials for meadow, earth, slate, sand, oak, foliage, ores, and foundation;
- translucent animated water, waving vegetation, v7 mountains/ridges/caverns, three biomes, ore seams, oaks, and berry patches;
- custom sky, fog, clouds, day/night cycle, dynamic-lighting parameters, filmic tone mapping, automatic exposure, bloom, volumetric light, FXAA, mipmapping, and 1,024 px dynamic shadows;
- health, nourishment, regeneration/starvation, drowning/fall damage, starter items, 3×3 crafting, four tools, lanterns, and persistent worlds/inventories;
- Luanti's mature Android touch controls plus desktop keyboard/mouse controls;
- standalone Stonewild Android identity (`org.stonewild.realms`), original icon/menu art, and a focused local-play interface;
- a reproducible universal Android APK (arm64 phone/tablet support plus x86-64 for exact-package emulator validation) and Linux Mint `.deb` workflows.

This is an early playable survival foundation, **not** a claim of feature parity with Minecraft or a finished game. Creatures, advanced equipment progression, agriculture, weather, structures, multiplayer polish, audio, and broader content remain future phases.

## Hardware profile

| Target | Minimum for this phase | Recommended |
|---|---:|---:|
| Android | Android 5.0+, 64-bit ARM, 2 GB RAM, OpenGL ES 2 | Android 10+, 4 GB RAM, OpenGL ES 3 |
| Linux Mint | Mint 21+, x86-64, 4 GB RAM, OpenGL 2.1-class driver | 8 GB RAM, current Mesa/proprietary driver |

The default visual profile is deliberately ambitious. On slower devices, reduce viewing range and disable water reflections, volumetric lighting, or dynamic shadows in Settings.

## Controls

- **Android:** Luanti's touch joystick, swipe camera, dig/place buttons, jump, inventory, and hotbar controls.
- **Desktop:** `WASD` move, mouse look, left click dig/attack, right click place/use, `Space` jump, `I` inventory, number keys select the hotbar, `Esc` pause.

## Reproducible source overlay

`./scripts/prepare_upstream.sh DESTINATION [CACHE]` downloads the exact Luanti 5.17.0 source archive, verifies SHA-256 `52e7dd315ae0e5c3868a23231e691578cd694bcca5af627fc57c03f32bdc846f`, copies the Stonewild game, and applies small auditable branding/packaging patches. It refuses to overwrite an existing destination.

Build entry points:

```sh
# Requires JDK 17 and the Android SDK/NDK specified by upstream Luanti.
./scripts/build_android.sh /tmp/stonewild-apk

# Requires the Debian/Ubuntu dependencies listed in Luanti's compiling guide.
./scripts/build_linux_deb.sh /tmp/stonewild-deb
```

CI performs Lua syntax checks, source checksum verification, APK identity/signature/ABI/asset inspection, a real headless world launch, software-OpenGL desktop launch, ELF dependency checks, and package inspection.

## Licensing and clean-room status

The game Lua is original MIT-licensed code. Stonewild's generated and derived visuals are documented in `games/stonewild/ASSET_LICENSE.md` and dedicated under CC0. No Minecraft source, textures, models, sounds, UI, recipes, maps, or branding are used.

Luanti is an independent LGPL-2.1-or-later upstream dependency. See `ENGINE_NOTICE.md`; builds preserve the upstream license, notices, credits, About interface, and pinned corresponding-source location.
