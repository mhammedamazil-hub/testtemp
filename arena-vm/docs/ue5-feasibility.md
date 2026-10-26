# Unreal Engine 5 — feasibility assessment

**Verdict: BLOCKED — the real UE5 editor cannot be installed or run in this
sandbox.** This document records the evidence, collected on 2026-10-26 from
the live environment (`tests/collect_env.sh`). No mock editor, launcher
screenshot, or substitute is counted as UE5.

## Measured environment

| Resource | Measured value | UE5 Linux requirement (Epic docs, 5.x) |
|---|---|---|
| CPU | 2 vCPU, Intel Xeon @2.60 GHz (KVM guest) | 8–12+ cores recommended |
| RAM | 3.8 GiB total | 16 GiB minimum, 32 GiB recommended |
| Disk | 21 GiB total, ~19 GiB free | ~40 GiB source checkout + 80–120 GiB build artifacts |
| GPU | none: no `/dev/dri`, no `/dev/nvidia*`, no system `libGL` | Vulkan 1.2+ capable GPU with vendor driver (or GL 4.6) |
| Compiler | gcc 12.2 only; no clang | clang 16/17/18 (UBT ships and expects a specific clang; gcc is not supported for the editor) |
| C# / .NET | none | .NET 6/8 SDK for UnrealBuildTool |
| Display | Xvfb, software-only (no render node, no glamor) | Vulkan/GL context with real device support |
| Egress | github.com, pypi.org, registry.npmjs.org only | Epic CDN/Akamai + linked EpicGames GitHub org |

## Hard blockers (each independently sufficient)

1. **No GPU and no render node.** The editor's renderer (RHI) needs Vulkan or
   GL on a real device. `/dev/dri` does not exist, so even Mesa's software
   paths (lavapipe/llvmpipe, which themselves need a render node or
   kms-swrast) have no supported setup here, and glvnd stubs without a vendor
   driver cannot create a context. Running the full editor on CPU rasterization
   is not an Epic-supported configuration and is not viable on 2 vCPUs/3.8 GiB.
2. **Memory is ~4–8x below minimum.** The editor + shader compiler (DXC/SPIR-V)
   routinely exceeds 8 GiB resident on empty projects; 3.8 GiB guarantees OOM
   kills. There is no swap.
3. **Disk cannot hold a source build.** A UE5 source tree plus compiled
   artifacts needs >100 GiB; ~19 GiB is free.
4. **Toolchain mismatch.** Epic's Linux build requires a pinned clang and the
   .NET SDK. Neither is installed, Debian mirrors are unreachable, and the
   Microsoft package host and Epic CDN are outside the egress allowlist.
5. **Licensing/distribution.** UE5 source is only available after accepting
   the EULA and linking an Epic Games account to GitHub (EpicGames org). Epic
   ships no precompiled generic Linux editor binary; the Linux editor is
   source-only. `release-assets.githubusercontent.com` is also blocked here.

## What was tested instead (free, genuinely working)

- **Blender 4.2 LTS via the official `bpy` wheel from PyPI** — installed,
  headless CPU scene creation + render + save/reopen smoke test
  (`tests/blender_smoke.py`). This is the strongest 3D tool this environment
  supports. Blender's GUI also runs on Xvfb with llvmpipe-independent 2D
  drawing; GPU features (CUDA/OptiX/HIP) are unavailable and CPU rendering is
  used.
- **Software-rasterizer route for UE5 (not pursued, documented as
  cost-free-but-infeasible):** Mesa lavapipe could in principle expose a
  software Vulkan ICD, but it still needs far more RAM/CPU than available and
  Epic does not support it for the editor; it would demonstrate a context, not
  a usable editor.

## Re-evaluation trigger

If a future sandbox exposes `/dev/dri/renderD*` with a Vulkan driver,
≥16 GiB RAM, ≥120 GiB free disk, and egress to the Epic CDN, this assessment
should be redone starting from the official "Linux Build Instructions" page on
dev.epicgames.com.
