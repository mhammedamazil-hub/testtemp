import { Viewer } from "@mkkellogg/gaussian-splats-3d";

export class ViewerUnsupportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ViewerUnsupportedError";
  }
}

export interface SplatViewerHandle {
  reset(): void;
  setAutoRotate(enabled: boolean): void;
  dispose(): Promise<void>;
}

const DEFAULT_CAMERA: [number, number, number] = [0, 0.35, 2.75];

export function supportsWebGL2(): boolean {
  try {
    const canvas = document.createElement("canvas");
    return Boolean(canvas.getContext("webgl2"));
  } catch {
    return false;
  }
}

/**
 * Mount the real 3D Gaussian splat renderer (Three.js based) for a PLY URL.
 * No point-cloud stand-in: if WebGL2 or the renderer is unavailable we fail
 * loudly and the UI shows the exact reason.
 *
 * `upright` applies a -90° X rotation because Idol's splat frame is Z-up while
 * three.js (and the built-in controls) are Y-up.
 */
export async function mountSplatViewer(
  container: HTMLElement,
  url: string,
  options: { onProgress?: (fraction: number) => void; upright?: boolean } = {},
): Promise<SplatViewerHandle> {
  if (!supportsWebGL2()) {
    throw new ViewerUnsupportedError(
      "This browser or device has no WebGL2 support, so the Gaussian splat cannot be rendered here. The PLY is still available to download.",
    );
  }

  container.replaceChildren();
  const upright = options.upright ?? true;

  const viewer = new Viewer({
    rootElement: container,
    cameraUp: [0, 1, 0],
    initialCameraPosition: DEFAULT_CAMERA,
    initialCameraLookAt: [0, 0, 0],
    useBuiltInControls: true,
    selfDrivenMode: true,
    // SharedArrayBuffer needs cross-origin isolation; we skip workers rather
    // than force COEP on every deployment.
    sharedMemoryForWorkers: false,
    gpuAcceleratedSort: true,
    dynamicScene: false,
    antialiased: true,
    showLoadingUI: false,
  });

  try {
    await viewer.addSplatScene(url, {
      showLoadingUI: false,
      progressiveLoad: true,
      splatAlphaRemovalThreshold: 5,
      rotation: upright ? [-Math.PI / 2, 0, 0] : [0, 0, 0],
      onProgress: (percent: number) => options.onProgress?.(Math.min(1, percent / 100)),
    });
  } catch (error) {
    await viewer.dispose().catch(() => undefined);
    throw error instanceof Error ? error : new Error("The splat renderer failed to start.");
  }

  viewer.start();

  const controls = viewer.controls;
  if (controls) {
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.minDistance = 0.4;
    controls.maxDistance = 40;
    controls.autoRotateSpeed = 1.4;
    controls.autoRotate = false;
  }

  return {
    reset() {
      controls?.target.set(0, 0, 0);
      const camera = viewer.camera as {
        position?: { set(x: number, y: number, z: number): void };
        lookAt?: (x: number, y: number, z: number) => void;
        updateProjectionMatrix?: () => void;
      };
      camera.position?.set(...DEFAULT_CAMERA);
      camera.lookAt?.(0, 0, 0);
      camera.updateProjectionMatrix?.();
      controls?.update();
    },
    setAutoRotate(enabled: boolean) {
      if (controls) controls.autoRotate = enabled;
    },
    async dispose() {
      try {
        viewer.stop();
        await viewer.dispose();
      } catch {
        /* already gone */
      } finally {
        container.replaceChildren();
      }
    },
  };
}
