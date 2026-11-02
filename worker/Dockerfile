# syntax=docker/dockerfile:1.7
# GPU inference worker: ComfyUI + ComfyUI-Idol + Idol/TripoSplat weights.
# Idol runs on ComfyUI's own native TripoSplat nodes; the Idol pack is GPL-3.0
# and the weights are Apache-2.0 (see docs/LICENSING.md).
FROM nvidia/cuda:12.4.1-cudnn-runtime-ubuntu22.04

ENV DEBIAN_FRONTEND=noninteractive \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    COMFYUI_MODELS_DIR=/comfyui/models

RUN apt-get update && apt-get install -y --no-install-recommends \
        python3 python3-pip python3-dev git curl ca-certificates \
        libgl1 libglib2.0-0 ffmpeg \
    && rm -rf /var/lib/apt/lists/*

ARG COMFYUI_REF=v0.43.0
ARG COMFYUI_REPO=https://github.com/comfyanonymous/ComfyUI.git
RUN git clone --depth 1 --branch "${COMFYUI_REF}" "${COMFYUI_REPO}" /comfyui

WORKDIR /comfyui

# CUDA 12.4 wheels; change the index URL for a different CUDA major version.
RUN python3 -m pip install --upgrade pip setuptools wheel \
    && python3 -m pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu124 \
    && python3 -m pip install -r requirements.txt

# ComfyUI-Idol (pinned) — adds Idol Preprocess Image, Idol Input Camera and Idol Precision.
ARG COMFYUI_IDOL_REPO=https://github.com/PedroMarinhoDev/ComfyUI-Idol.git
ARG COMFYUI_IDOL_REF=59ed4b43f0d04a2c8bac689f99e51901dd7c2329
RUN git clone "${COMFYUI_IDOL_REPO}" /comfyui/custom_nodes/ComfyUI-Idol \
    && cd /comfyui/custom_nodes/ComfyUI-Idol \
    && git checkout "${COMFYUI_IDOL_REF}" \
    && git rev-parse HEAD

RUN python3 -m pip install --no-cache-dir "huggingface_hub[cli]>=0.26"

COPY download_models.sh /usr/local/bin/download_models.sh
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/download_models.sh /usr/local/bin/docker-entrypoint.sh

EXPOSE 8188

HEALTHCHECK --interval=30s --timeout=10s --start-period=180s --retries=5 \
    CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8188/system_stats', timeout=8)"

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
