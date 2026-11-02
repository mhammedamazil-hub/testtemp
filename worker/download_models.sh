#!/usr/bin/env bash
# Download the exact files ComfyUI-Idol and ComfyUI's TripoSplat template need.
# Idempotent: existing files are skipped. Set DOWNLOAD_MODELS=0 to disable.
set -euo pipefail

MODELS_DIR="${COMFYUI_MODELS_DIR:-/comfyui/models}"
VARIANT="${IDOL_WEIGHTS_VARIANT:-fp32}"

if [[ "${VARIANT}" == "fp16" ]]; then
  UNET_FILE="diffusion_models/idol_fp16.safetensors"
  DECODER_FILE="vae/idol_vae_decoder_fp16.safetensors"
else
  UNET_FILE="diffusion_models/idol_fp32.safetensors"
  DECODER_FILE="vae/idol_vae_decoder_fp32.safetensors"
fi

mkdir -p "${MODELS_DIR}"/{diffusion_models,vae,clip_vision,background_removal,checkpoints,loras}

fetch() {
  local repo="$1"; shift
  local file
  for file in "$@"; do
    if [[ -f "${MODELS_DIR}/${file}" ]]; then
      echo "[models] present: ${file}"
      continue
    fi
    echo "[models] downloading ${repo}/${file}"
    for attempt in 1 2 3; do
      if hf download "${repo}" "${file}" --local-dir "${MODELS_DIR}"; then
        break
      fi
      echo "[models] attempt ${attempt} failed for ${file}, retrying…"
      sleep 5
      if [[ "${attempt}" == "3" ]]; then
        echo "[models] FATAL: could not download ${file}" >&2
        exit 1
      fi
    done
  done
}

# Idol by speridlabs — Apache-2.0 weights, converted for ComfyUI's TripoSplat nodes.
fetch PedroMarinhoDev/Idol-ComfyUI "${UNET_FILE}" "${DECODER_FILE}"

# Image encoder, FLUX.2 VAE and BiRefNet matting model used by ComfyUI's TripoSplat template.
fetch VAST-AI/TripoSplat clip_vision/dino_v3_vit_h.safetensors vae/flux2-vae.safetensors
fetch Comfy-Org/BiRefNet background_removal/birefnet.safetensors

echo "[models] done. Files:"
find "${MODELS_DIR}" -maxdepth 2 -type f -name '*.safetensors' -printf '  %-64p %s bytes\n' | sort
