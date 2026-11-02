# GPU & model requirements

Everything below is required for **real** image-to-3D inference. The studio itself runs happily on CPU; the
worker is the only part that needs a GPU.

## Verified reference hardware (upstream ComfyUI-Idol)

| | ComfyUI-Idol's tested configuration |
|---|---|
| ComfyUI | 0.43.0 (native TripoSplat nodes are required) |
| GPU | RTX 4090 and RTX 3090, 24 GB |
| VRAM | **5.8 GB peak** for the whole ComfyUI server at 262,144 Gaussians (two 72-frame turntables), RTX 3090 |
| Time | 28.4 s per object on an RTX 4090 (24.3 s of that is 50 sampling steps); ~52 s of sampling on an RTX 3090 |
| Python packages | none beyond ComfyUI |

Practical guidance:

* **24 GB** is comfortable at every Gaussian count. 12 GB works at 262k; 1M Gaussians and `SplatToMesh` at
  512³ want 24 GB.
* First run on a cold worker includes loading ~5 GB of weights from disk (tens of seconds).
* `fp32` precision is ~4.6× slower than the default and needs the fp32 denoiser file.

## Model files

Downloaded automatically by `worker/download_models.sh` into the ComfyUI models volume.

| File | Folder | Size | Source | Licence |
|---|---|---|---|---|
| `idol_fp32.safetensors` | `models/diffusion_models` | 1.48 GB | [PedroMarinhoDev/Idol-ComfyUI](https://huggingface.co/PedroMarinhoDev/Idol-ComfyUI) | Apache-2.0 (Idol weights) |
| `idol_vae_decoder_fp32.safetensors` | `models/vae` | 1.15 GB | PedroMarinhoDev/Idol-ComfyUI | Apache-2.0 |
| `dino_v3_vit_h.safetensors` | `models/clip_vision` | 1.68 GB | [VAST-AI/TripoSplat](https://huggingface.co/VAST-AI/TripoSplat) (bf16 DINOv3 ViT-H) | DINOv3 License (Meta) |
| `flux2-vae.safetensors` | `models/vae` | 0.34 GB | VAST-AI/TripoSplat | FLUX.2 VAE licence (Black Forest Labs) |
| `birefnet.safetensors` | `models/background_removal` | 0.44 GB | [Comfy-Org/BiRefNet](https://huggingface.co/Comfy-Org/BiRefNet) | BiRefNet MIT |

Optional / advanced:

| File | Folder | Size | Use |
|---|---|---|---|
| `idol_fp16.safetensors` | `models/diffusion_models` | 0.74 GB | same speed and result at default precision; not for fp32 presets |
| `idol_vae_decoder_fp16.safetensors` | `models/vae` | 0.58 GB | decodes in fp16 |
| Meta's fp32 DINOv3 `model.safetensors` (gated) | `models/clip_vision` | 3.36 GB | only for an all-fp32 pipeline |

Set `IDOL_WEIGHTS_VARIANT=fp16` in `.env` to download the fp16 pair instead of fp32.

### Manual download

```bash
cd /path/to/ComfyUI/models
pip install "huggingface_hub[cli]"

hf download PedroMarinhoDev/Idol-ComfyUI \
  diffusion_models/idol_fp32.safetensors \
  vae/idol_vae_decoder_fp32.safetensors --local-dir .

hf download VAST-AI/TripoSplat \
  clip_vision/dino_v3_vit_h.safetensors \
  vae/flux2-vae.safetensors --local-dir .

hf download Comfy-Org/BiRefNet background_removal/birefnet.safetensors --local-dir .
```

If you place the files in sub-folders (for example with ComfyUI-Idol's `tools/convert_idol.py`, which writes
`diffusion_models/idol/idol_fp32.safetensors`), point the studio at the actual names:

```
IDOL_MODEL_UNET=idol/idol_fp32.safetensors
IDOL_MODEL_VAE_DECODER=idol_vae_decoder_fp32.safetensors
```

`GET /api/health` and `scripts/preflight.py` compare the configured names with what ComfyUI actually lists,
so a mismatch is reported before a job is wasted.

## ComfyUI nodes

Installed by the worker image, pinned:

* **ComfyUI `v0.43.0`** — provides native TripoSplat support: `TripoSplatConditioning`,
  `VAEDecodeTripoSplat`, `SplatToFile3D`, `SaveGaussianSplat` (Save Splat), `RenderSplat`, and the mesh path
  `SplatToMesh` → `MeshToFile3D`/`Save3DAdvanced`.
* **ComfyUI-Idol** at commit `59ed4b4` — adds `IdolPreprocessImage` (Idol's 1.2×/1.7× square crop),
  `IdolInputCamera` (Idol's predicted viewpoint) and `IdolPrecision` (fp16 / fp32 with fp16 attention / fp32,
  plus the TripoSplat dtype patch).
* Core: `LoadImage`, `LoadBackgroundRemovalModel`, `RemoveBackground`, `CLIPVisionLoader`, `VAELoader`,
  `UNETLoader`, `KSampler`, `SaveImage`, `MaskToImage`.

No extra `pip install` is needed for ComfyUI-Idol itself.

## Inference parameters used by the studio

| Setting | Value | Notes |
|---|---|---|
| sampler / scheduler | `euler` / `simple` | Idol's workflow |
| steps | 50 (10–80) | fewer steps = faster, rougher |
| cfg | 4.0 (1–8) | Idol's guidance 3 in ComfyUI's convention |
| denoise | 1.0 | |
| time shift | 3 | built into the model |
| Gaussians | 262,144 default; 32,768–1,048,576 in multiples of 32,768 | set on `VAEDecodeTripoSplat` |
| mode | `synthetic` (1.2× crop) / `real` (1.7×) | same value on both Idol nodes |
| precision | `default` / `fp32, fp16 attention` / `fp32` | fp32 needs `idol_fp32.safetensors` |
| mesh export | off by default; `SplatToMesh` 128–512³ voxels | GLB only if ComfyUI exposes the nodes |

## Licensing summary

Idol's **code** (speridlabs/idol-sdk) and **weights** are Apache-2.0; the ComfyUI integration pack is GPL-3.0;
ComfyUI itself is GPL-3.0. DINOv3, the FLUX.2 VAE and BiRefNet keep their own licences — review them before
commercial deployment. Full detail: [LICENSING.md](LICENSING.md).
