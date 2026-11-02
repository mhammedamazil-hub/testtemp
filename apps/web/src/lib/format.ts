import type { GenerationMode, Precision } from "./types";

export function formatBytes(bytes?: number | null, decimals = 1): string {
  if (bytes === undefined || bytes === null || Number.isNaN(bytes)) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(exponent === 0 ? 0 : decimals)} ${units[exponent]}`;
}

export function formatCount(value?: number | null): string {
  if (value === undefined || value === null || Number.isNaN(value)) return "—";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value % 1_000_000 === 0 ? 0 : 2)}M`;
  if (value >= 10_000) return `${(value / 1000).toFixed(1)}K`;
  return value.toLocaleString("en-US");
}

export function formatDuration(ms?: number | null): string {
  if (ms === undefined || ms === null || Number.isNaN(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds % 60);
  return `${minutes}m ${rest.toString().padStart(2, "0")}s`;
}

export function elapsedSince(iso?: string | null, now = Date.now()): number {
  if (!iso) return 0;
  const start = Date.parse(iso);
  if (Number.isNaN(start)) return 0;
  return Math.max(0, now - start);
}

export function relativeTime(iso?: string | null, now = Date.now()): string {
  if (!iso) return "—";
  const value = Date.parse(iso);
  if (Number.isNaN(value)) return "—";
  const diff = Math.round((now - value) / 1000);
  if (diff < 45) return "just now";
  if (diff < 90) return "1 min ago";
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 7200) return "1 hour ago";
  if (diff < 86_400) return `${Math.round(diff / 3600)} hours ago`;
  if (diff < 172_800) return "yesterday";
  return new Date(value).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export const MODE_LABELS: Record<GenerationMode, string> = {
  synthetic: "Render / generated",
  real: "Photograph",
};

export const PRECISION_LABELS: Record<Precision, string> = {
  default: "Default · fastest",
  "fp32, fp16 attention": "fp32 · fp16 attention",
  fp32: "fp32 · full precision",
};

export function estimateSeconds(gaussians: number, steps: number): string {
  // Idol's published numbers: 262,144 Gaussians / 50 steps = 28.4 s on an RTX 4090,
  // sampling scales ~linearly with steps and weakly with Gaussian count.
  const baseline = 28.4;
  const scale = (steps / 50) * Math.pow(gaussians / 262_144, 0.35);
  const seconds = baseline * scale;
  if (seconds < 12) return "~10 s on an RTX 4090";
  if (seconds < 45) return `~${Math.round(seconds / 5) * 5} s on an RTX 4090`;
  return `~${Math.round(seconds / 10) * 10} s on an RTX 4090`;
}

export function progressPercent(progress: number): number {
  return Math.max(0, Math.min(100, Math.round(progress * 100)));
}

export function fileExtension(name: string): string {
  const index = name.lastIndexOf(".");
  return index === -1 ? "" : name.slice(index + 1).toLowerCase();
}
