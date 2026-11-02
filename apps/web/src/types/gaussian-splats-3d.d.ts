/**
 * Minimal typings for @mkkellogg/gaussian-splats-3d (the package ships no
 * .d.ts files). Only the API surface the studio uses is declared.
 */
declare module "@mkkellogg/gaussian-splats-3d" {
  export interface ViewerOptions {
    cameraUp?: number[];
    initialCameraPosition?: number[];
    initialCameraLookAt?: number[];
    useBuiltInControls?: boolean;
    selfDrivenMode?: boolean;
    rootElement?: HTMLElement;
    sharedMemoryForWorkers?: boolean;
    gpuAcceleratedSort?: boolean;
    dynamicScene?: boolean;
    antialiased?: boolean;
    halfPrecisionCovariancesOnGPU?: boolean;
    ignoreDevicePixelRatio?: boolean;
    renderMode?: number;
    sceneRevealMode?: number;
    showLoadingUI?: boolean;
    [key: string]: unknown;
  }

  export interface SplatSceneOptions {
    progressiveLoad?: boolean;
    showLoadingUI?: boolean;
    splatAlphaRemovalThreshold?: number;
    rotation?: number[];
    position?: number[];
    scale?: number[];
    format?: number;
    headers?: Record<string, string>;
    onProgress?: (percentComplete: number, label?: string, status?: number) => void;
    [key: string]: unknown;
  }

  export interface ViewerControls {
    autoRotate: boolean;
    autoRotateSpeed: number;
    enableDamping: boolean;
    dampingFactor: number;
    minDistance: number;
    maxDistance: number;
    target: { set(x: number, y: number, z: number): void };
    update(): void;
    dispose(): void;
  }

  export class Viewer {
    constructor(options?: ViewerOptions);
    camera: unknown;
    controls: ViewerControls | null;
    splatMesh: unknown;
    addSplatScene(path: string, options?: SplatSceneOptions): Promise<void>;
    addSplatScenes(scenes: SplatSceneOptions[], showLoadingUI?: boolean): Promise<void>;
    start(): void;
    stop(): void;
    update(): void;
    dispose(): Promise<void>;
  }

  export const SceneFormat: {
    Splat: number;
    KSplat: number;
    Ply: number;
    Spz: number;
  };

  const _default: {
    Viewer: typeof Viewer;
    SceneFormat: typeof SceneFormat;
  };
  export default _default;
}
