# Development-only: ComfyUI-API simulator, no GPU and no Idol inference.
FROM python:3.11-slim
ENV PYTHONUNBUFFERED=1 PIP_NO_CACHE_DIR=1
WORKDIR /sim
RUN python3 -m pip install "numpy>=1.26" "pillow>=10.4" "fastapi>=0.115,<0.163" \
        "uvicorn[standard]>=0.30,<0.39" "python-multipart>=0.0.9,<0.1"
COPY tools/ /sim/
EXPOSE 8288
HEALTHCHECK --interval=15s --timeout=5s CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8288/system_stats', timeout=4)"
CMD ["python3", "simulate_comfy.py", "--host", "0.0.0.0", "--port", "8288", "--step-seconds", "0.05"]
