# syntax=docker/dockerfile:1.7
# Studio: FastAPI backend + prebuilt React/Three.js frontend in one image.

FROM node:22-bookworm-slim AS web
WORKDIR /src
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY apps/web/ ./
RUN npm run build

FROM python:3.11-slim AS studio
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1

RUN groupadd --system --gid 10001 studio \
    && useradd --system --uid 10001 --gid studio --home /app studio \
    && mkdir -p /app /data && chown -R studio:studio /app /data

WORKDIR /app
COPY apps/api/requirements.txt ./
RUN python -m pip install -r requirements.txt
COPY apps/api/app ./app
COPY --from=web /src/dist ./frontend_dist

ENV IDOL_ENVIRONMENT=production \
    IDOL_DATA_DIR=/data \
    IDOL_FRONTEND_DIST=/app/frontend_dist \
    IDOL_COMFYUI_URL=http://comfyui:8188

USER studio
EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=6s --start-period=20s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8000/api/health', timeout=5)"

CMD ["uvicorn", "app.asgi:app", "--host", "0.0.0.0", "--port", "8000", "--proxy-headers", "--forwarded-allow-ips=*"]
