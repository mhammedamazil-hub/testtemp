// @vitest-environment jsdom
/**
 * Blank-page guard.
 *
 * The preview runs in a browser we cannot open from the build sandbox, so this
 * test mounts the real <App /> in jsdom with a stubbed API and asserts that the
 * first paint contains the studio chrome. It catches the failure modes that
 * produce a white screen: a module that touches a browser API at import time, a
 * render-time throw, or a component that disappears behind a null config.
 */
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";

// Tell React this is an act() environment (vitest's jsdom is not pre-configured).
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const HEALTH = {
  ok: true,
  app: "Idol Image-to-3D Studio",
  environment: "development",
  worker: {
    reachable: true,
    simulated: true,
    url: "http://127.0.0.1:8288",
    version: "0.43.0",
    devices: [],
    queueRemaining: 0,
    error: null,
    checkedAt: new Date().toISOString(),
  },
  capabilities: {
    idolNodes: true,
    preprocess: true,
    glb: true,
    backgroundRemoval: true,
    missingNodes: [],
    missingModels: [],
  },
  queueDepth: 0,
  allowSimulatedWorker: true,
};

const CONFIG = {
  maxUploadMb: 20,
  maxImagePixels: 40_000_000,
  gaussians: { min: 32768, max: 1048576, default: 262144, step: 32768 },
  steps: { min: 10, max: 80, default: 50 },
  presets: [{ id: "standard", label: "Standard", gaussians: 262144, steps: 50, hint: "Idol default" }],
  modes: [
    { id: "synthetic", label: "Render / generated", description: "" },
    { id: "real", label: "Photograph", description: "" },
  ],
  precisions: ["default", "fp32, fp16 attention", "fp32"],
  models: {
    diffusionModel: "idol_fp32.safetensors",
    splatDecoder: "idol_vae_decoder_fp32.safetensors",
    imageEncoder: "dino_v3_vit_h.safetensors",
    fluxVae: "flux2-vae.safetensors",
    backgroundRemoval: "birefnet.safetensors",
  },
  features: {
    idolNodes: true,
    preprocess: true,
    glb: true,
    backgroundRemoval: true,
    missingNodes: [],
    missingModels: [],
  },
  demoImage: "/demo/sample-object.png",
};

let container: HTMLDivElement;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);

  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("/api/health") ? HEALTH : url.includes("/api/config") ? CONFIG : { jobs: [] };
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }),
  );
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
  class NoopObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  }
  vi.stubGlobal("ResizeObserver", NoopObserver);
  vi.stubGlobal("IntersectionObserver", NoopObserver);
});

afterEach(() => {
  container.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("App first paint", () => {
  it("mounts the studio chrome instead of a blank page", async () => {
    const root = createRoot(container);
    await act(async () => {
      root.render(<App />);
    });

    const text = container.textContent ?? "";
    expect(text).toContain("Idol");
    expect(container.querySelector("main")).not.toBeNull();
    expect(text.length).toBeGreaterThan(200);
    expect(text.toLowerCase()).toContain("upload");

    await act(async () => {
      root.unmount();
    });
  });

  it("surfaces the simulated worker instead of pretending it is real", async () => {
    const root = createRoot(container);
    await act(async () => {
      root.render(<App />);
    });

    const text = (container.textContent ?? "").toLowerCase();
    expect(text).toContain("simulated");

    await act(async () => {
      root.unmount();
    });
  });
});
