# Licensing and attribution

**Short version:** Idol's architecture and weights are Apache-2.0; the ComfyUI integration code is GPL-3.0;
this repository is GPL-3.0-or-later. The image encoder, VAE and matting model each carry their own licence and
must be reviewed before commercial use. The studio talks to ComfyUI over its public HTTP/WebSocket API and
does not link against GPL code in the API/frontend processes, but the distributed worker image contains
GPL-3.0 software, so source availability obligations apply to that image.

## This repository

* Code: **GPL-3.0-or-later** — see [`../LICENSE`](../LICENSE) and [`../NOTICE`](../NOTICE).
* Why GPL: the GPU worker image redistributes ComfyUI and ComfyUI-Idol (both GPL-3.0). Choosing GPL for the
  studio keeps the combined deployment unambiguously compliant. If you only ever deploy the studio against a
  pre-existing ComfyUI installation, the studio code alone could be relicensed — ask before doing that in a
  proprietary product.

## Upstream components

| Component | Role | Licence | Notes |
|---|---|---|---|
| [speridlabs/idol-sdk](https://github.com/speridlabs/idol-sdk) | Idol architecture + weights (TripoSplat-based) | Apache-2.0 | Attribution required; keep the licence and NOTICE with redistributed weights |
| [PedroMarinhoDev/ComfyUI-Idol](https://github.com/PedroMarinhoDev/ComfyUI-Idol) @ `59ed4b4` | ComfyUI nodes: Idol preprocess, input camera, precision | GPL-3.0 | Installed unmodified inside the worker image |
| [ComfyUI](https://github.com/comfyanonymous/ComfyUI) @ `v0.43.0` | Inference runtime, TripoSplat + 3D/splat nodes | GPL-3.0 | Unmodified |
| [VAST-AI/TripoSplat](https://huggingface.co/VAST-AI/TripoSplat) | DINOv3 ViT-H encoder (bf16), FLUX.2 VAE | DINOv3 License / BFL model licence | Check the model cards for commercial terms |
| [Comfy-Org/BiRefNet](https://huggingface.co/Comfy-Org/BiRefNet) | Background removal | MIT (BiRefNet) | Repack of the BiRefNet weights |
| [PedroMarinhoDev/Idol-ComfyUI](https://huggingface.co/PedroMarinhoDev/Idol-ComfyUI) | Converted Idol weights | Apache-2.0 | Conversion, not new weights |

We do **not** vendor or modify any upstream source. The worker `git clone`s the pinned revisions at build
time; the studio's only integration surface is ComfyUI's documented API (`/upload/image`, `/prompt`,
`/history`, `/view`, `/queue`, `/ws`, `/object_info`).

## Frontend / backend dependencies

| Package | Licence |
|---|---|
| Three.js | MIT |
| @mkkellogg/gaussian-splats-3d | MIT |
| React, React DOM | MIT |
| Vite, Tailwind CSS | MIT |
| FastAPI, Starlette, Uvicorn, httpx | MIT / BSD-3-Clause |
| Pillow | MIT-CMU |
| Pydantic, pydantic-settings | MIT |
| lucide-react | ISC |

## Model weight redistribution

If you publish the worker image or a volume containing the weights:

1. Keep `NOTICE` and the Idol Apache-2.0 licence text alongside the weights.
2. Keep the DINOv3, FLUX.2 VAE and BiRefNet licences/attributions as published on their model cards.
3. Do not present modified weights as the original Idol release.
4. The studio's simulator writes its own procedural geometry and includes `comment SIMULATED` in the PLY
   header — it must never be passed off as Idol output.

## Trademarks

"Idol" and "speridlabs" belong to their owners. This project is an independent integration; it is not
endorsed by speridlabs, VAST, Black Forest Labs, Meta or the ComfyUI maintainers.
