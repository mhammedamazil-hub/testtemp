# Idol Image-to-3D Studio — developer commands
SHELL := /bin/bash
PY := .venv/bin/python
PIP := .venv/bin/pip
COMPOSE := docker compose

.PHONY: help install venv web-deps dev dev-api dev-web sim test test-api test-web e2e typecheck lint build \
        preflight smoke docker-build up down sim-up logs clean

help:
	@echo "Idol Studio"
	@echo ""
	@echo "  make install     Create .venv, install API + web dependencies"
	@echo "  make sim         Run the ComfyUI-API simulator (no GPU) on :8288"
	@echo "  make dev-api     Run the API on :8000 (reads .env)"
	@echo "  make dev-web     Run the Vite dev server on :5173 (proxies /api)"
	@echo "  make test        API pytest + web vitest"
	@echo "  make e2e         Full HTTP/WS end-to-end run against the simulator"
	@echo "  make typecheck   tsc --noEmit for the web app"
	@echo "  make lint        ESLint"
	@echo "  make build       Production web build + API import check"
	@echo "  make preflight   Check a real ComfyUI worker for nodes, models, GPU"
	@echo "  make smoke       REAL Idol inference check (needs GPU worker)"
	@echo "  make up          docker compose up (GPU worker + studio)"
	@echo "  make sim-up      docker compose up with the no-GPU simulator"
	@echo ""

venv:
	python3 -m venv .venv
	$(PIP) install --upgrade pip

install: venv
	$(PIP) install -e "apps/api[dev]"
	$(PIP) install numpy
	cd apps/web && npm ci

web-deps:
	cd apps/web && npm ci

dev:
	@echo "Run 'make sim', 'make dev-api' and 'make dev-web' in three terminals."

sim:
	cd tools && ../$(PY) simulate_comfy.py --port 8288

dev-api:
	cd apps/api && ../../$(PY) -m uvicorn app.asgi:app --reload --host 0.0.0.0 --port 8000

dev-web:
	cd apps/web && npm run dev

test: test-api test-web

test-api:
	cd apps/api && ../../$(PY) -m pytest -q

test-web:
	cd apps/web && npm test

e2e:
	$(PY) scripts/e2e_simulated.py

typecheck:
	cd apps/web && npm run typecheck

lint:
	cd apps/web && npm run lint

build:
	cd apps/web && npm run build
	cd apps/api && ../../$(PY) -c "from app.asgi import app; print('API app OK:', app.title)"

preflight:
	$(PY) scripts/preflight.py

smoke:
	$(PY) scripts/smoke_test_gpu.py

docker-build:
	$(COMPOSE) build

up:
	$(COMPOSE) up -d --build

down:
	$(COMPOSE) down

sim-up:
	$(COMPOSE) -f docker-compose.sim.yml up -d --build

logs:
	$(COMPOSE) logs -f --tail=200

clean:
	rm -rf .tmp apps/web/dist .pytest_cache apps/api/.pytest_cache
	find . -name __pycache__ -type d -prune -exec rm -rf {} +
