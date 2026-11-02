export type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";
export type JobKind = "generate" | "preprocess";
export type GenerationMode = "synthetic" | "real";
export type Precision = "default" | "fp32, fp16 attention" | "fp32";

export type Stage =
  | "queued"
  | "preparing"
  | "uploading"
  | "preprocess"
  | "sampling"
  | "decoding"
  | "meshing"
  | "rendering"
  | "saving"
  | "done"
  | "failed"
  | "canceled";

export interface FileRef {
  id: string;
  name: string;
  kind: "ply" | "glb" | "png" | "webp" | "mp4" | string;
  sizeBytes: number;
  contentType: string;
  url: string;
}

export interface JobError {
  code: string;
  message: string;
  hint?: string;
  retryable: boolean;
  details?: unknown;
}

export interface JobPreview {
  modelInput?: FileRef;
  mask?: FileRef;
  reference?: FileRef;
  cutout?: FileRef;
}

export interface JobResult {
  ply: FileRef;
  glb?: FileRef;
  gaussianCount: number;
  plyFormat?: string;
  plySizeBytes?: number;
  glbVertices?: number;
  glbFaces?: number;
  glbError?: string;
}

export interface JobInput {
  uploadId: string;
  width: number;
  height: number;
  sha256?: string;
  originalName?: string;
  imageUrl?: string;
  thumbUrl?: string;
}

export interface JobParams {
  mode: GenerationMode;
  gaussians?: number;
  steps?: number;
  cfg?: number;
  seed?: number | null;
  precision?: Precision;
  exportGlb?: boolean;
  meshResolution?: number;
}

export interface Job {
  id: string;
  kind: JobKind;
  status: JobStatus;
  stage: Stage;
  stageLabel?: string;
  progress: number;
  params: JobParams;
  input: JobInput | null;
  preview: JobPreview | null;
  result: JobResult | null;
  error: JobError | null;
  simulated: boolean;
  message?: string | null;
  createdAt: string;
  startedAt?: string | null;
  finishedAt?: string | null;
  durationMs?: number | null;
  queuePosition?: number | null;
}

export interface UploadImage {
  uploadId: string;
  imageUrl: string;
  thumbUrl?: string | null;
  width: number;
  height: number;
  sizeBytes: number;
  sha256: string;
  originalName: string;
}

export interface Capabilities {
  idolNodes: boolean;
  preprocess: boolean;
  glb: boolean;
  backgroundRemoval: boolean;
  missingNodes: string[];
  missingModels: string[];
}

export interface WorkerStatus {
  reachable: boolean;
  simulated: boolean;
  url: string;
  version?: string | null;
  devices: Array<Record<string, unknown>>;
  queueRemaining?: number | null;
  error?: string | null;
  checkedAt?: string | null;
}

export interface Health {
  ok: boolean;
  app: string;
  environment: string;
  worker: WorkerStatus;
  capabilities: Capabilities;
  queueDepth: number;
  allowSimulatedWorker: boolean;
}

export interface Preset {
  id: string;
  label: string;
  gaussians: number;
  steps: number;
  hint: string;
}

export interface ModeOption {
  id: GenerationMode;
  label: string;
  description: string;
}

export interface AppConfig {
  maxUploadMb: number;
  maxImagePixels: number;
  gaussians: { min: number; max: number; default: number; step: number };
  steps: { min: number; max: number; default: number };
  presets: Preset[];
  modes: ModeOption[];
  precisions: Precision[];
  models: Record<string, string>;
  features: Capabilities;
  demoImage?: string | null;
}

export interface GeneratePayload {
  uploadId: string;
  mode: GenerationMode;
  gaussians: number;
  steps: number;
  cfg: number;
  seed: number | null;
  precision: Precision;
  exportGlb: boolean;
  meshResolution: number;
}

export interface PlyHeader {
  format: string;
  vertexCount: number;
  properties: string[];
  hasColor: boolean;
  headerBytes: number;
}
