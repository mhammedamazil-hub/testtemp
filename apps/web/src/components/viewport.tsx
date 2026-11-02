import { useEffect, useRef, useState, type MutableRefObject } from "react";
import {
  AlertOctagon,
  ArrowUpRight,
  Box,
  Download,
  Loader2,
  RefreshCcw,
  RotateCcw,
  RotateCw,
  ScanEye,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import { cn } from "../lib/cn";
import { formatBytes, formatCount, formatDuration, progressPercent } from "../lib/format";
import { useNow } from "../hooks/useNow";
import type { Job } from "../lib/types";
import { mountSplatViewer, type SplatViewerHandle } from "../viewer/splat";
import { mountMeshViewer, type MeshViewerHandle } from "../viewer/mesh";
import { Spinner } from "./primitives";

/* ------------------------------------------------------------------ */
/* 3D canvases                                                        */
/* ------------------------------------------------------------------ */

export function SplatCanvas({
  url,
  autoRotate,
  upright,
  controllerRef,
  onReady,
}: {
  url: string;
  autoRotate: boolean;
  upright: boolean;
  controllerRef: MutableRefObject<SplatViewerHandle | null>;
  onReady?: (ready: boolean) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const element = host.current;
    if (!element) return;
    setState("loading");
    setProgress(0);
    controllerRef.current = null;

    void (async () => {
      try {
        const handle = await mountSplatViewer(element, url, {
          upright,
          onProgress: (fraction) => {
            if (!cancelled) setProgress(fraction);
          },
        });
        if (cancelled) {
          await handle.dispose();
          return;
        }
        controllerRef.current = handle;
        setState("ready");
        onReady?.(true);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "The splat viewer could not start.");
        setState("error");
        onReady?.(false);
      }
    })();

    return () => {
      cancelled = true;
      const handle = controllerRef.current;
      controllerRef.current = null;
      void handle?.dispose();
    };
  }, [url, upright, controllerRef, onReady]);

  useEffect(() => {
    controllerRef.current?.setAutoRotate(autoRotate);
  }, [autoRotate, controllerRef, state]);

  return (
    <div className="relative size-full">
      <div ref={host} className="size-full" />
      {state === "loading" && (
        <div className="absolute inset-0 grid place-items-center bg-obsidian-950/60 backdrop-blur-sm">
          <div className="w-56 text-center">
            <Loader2 className="mx-auto size-6 animate-spin text-cyan-300" />
            <p className="mt-3 text-xs font-medium text-ink-200">Loading Gaussian splats</p>
            <div className="mt-2 h-1 overflow-hidden rounded-full bg-white/10">
              <div
                className="h-full rounded-full bg-gradient-to-r from-cyan-400 to-violet-400 transition-all"
                style={{ width: `${Math.max(6, progress * 100)}%` }}
              />
            </div>
            <p className="mt-2 font-mono text-[10px] text-ink-500">{Math.round(progress * 100)}%</p>
          </div>
        </div>
      )}
      {state === "error" && (
        <div className="absolute inset-0 grid place-items-center p-6 text-center">
          <div className="max-w-sm">
            <TriangleAlert className="mx-auto size-6 text-amber-300" />
            <p className="mt-3 text-sm font-medium text-white">The 3D viewer could not load this splat</p>
            <p className="mt-1 text-xs leading-relaxed text-ink-400">{error}</p>
            <p className="mt-2 text-[11px] text-ink-500">
              The PLY file itself is intact — download it and open it in any 3DGS viewer.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

export function MeshCanvas({
  url,
  autoRotate,
  controllerRef,
  onReady,
}: {
  url: string;
  autoRotate: boolean;
  controllerRef: MutableRefObject<MeshViewerHandle | null>;
  onReady?: (ready: boolean) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const element = host.current;
    if (!element) return;
    setError(null);
    void (async () => {
      try {
        const handle = await mountMeshViewer(element, url);
        if (cancelled) {
          handle.dispose();
          return;
        }
        controllerRef.current = handle;
        onReady?.(true);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "The GLB could not be displayed.");
          onReady?.(false);
        }
      }
    })();
    return () => {
      cancelled = true;
      controllerRef.current?.dispose();
      controllerRef.current = null;
    };
  }, [url, controllerRef, onReady]);

  useEffect(() => {
    controllerRef.current?.setAutoRotate(autoRotate);
  }, [autoRotate, controllerRef, error]);

  return (
    <div className="relative size-full">
      <div ref={host} className="size-full" />
      {error && (
        <div className="absolute inset-0 grid place-items-center p-6 text-center">
          <div className="max-w-sm">
            <TriangleAlert className="mx-auto size-6 text-amber-300" />
            <p className="mt-3 text-sm font-medium text-white">The GLB could not be displayed</p>
            <p className="mt-1 text-xs text-ink-400">{error}</p>
          </div>
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* States                                                             */
/* ------------------------------------------------------------------ */

export function EmptyViewport({ hasUpload }: { hasUpload: boolean }) {
  return (
    <div className="relative grid size-full place-items-center overflow-hidden bg-obsidian-950">
      <div className="absolute inset-0 viewport-grid opacity-60" />
      <div className="absolute left-1/2 top-1/2 size-[420px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-cyan-500/10 blur-[110px]" />
      <div className="relative flex flex-col items-center px-6 text-center">
        <div className="relative mb-8 grid size-28 place-items-center [perspective:600px]">
          <div className="wire-cube relative size-16">
            {[0, 90, 180, 270].map((deg) => (
              <div key={`y${deg}`} style={{ transform: `rotateY(${deg}deg) translateZ(2rem)` }} />
            ))}
            {[90, 270].map((deg) => (
              <div key={`x${deg}`} style={{ transform: `rotateX(${deg}deg) translateZ(2rem)` }} />
            ))}
          </div>
        </div>
        <p className="text-[10px] font-semibold uppercase tracking-[0.3em] text-cyan-300/70">Viewport</p>
        <h2 className="mt-2 max-w-sm text-balance text-lg font-medium text-white">
          {hasUpload ? "Ready when you are" : "Your 3D object will appear here"}
        </h2>
        <p className="mt-2 max-w-sm text-balance text-xs leading-relaxed text-ink-400">
          {hasUpload
            ? "Generate to run Idol on the GPU worker. The finished splat arrives as a real 3DGS PLY and is rendered here with Three.js."
            : "Upload one image of an object on the left. Idol infers the full shape — including the sides the photo never showed — as 3D Gaussians."}
        </p>
        <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
          <span className="chip">
            <ScanEye size={11} /> Orbit · zoom · pan
          </span>
          <span className="chip">
            <Download size={11} /> PLY export
          </span>
          <span className="chip">
            <Box size={11} /> Optional GLB
          </span>
        </div>
      </div>
      <p className="absolute bottom-3 left-1/2 -translate-x-1/2 text-[10px] text-ink-600">
        Placeholder animation — not a generated model
      </p>
    </div>
  );
}

export function ProgressOverlay({ job, onCancel, canceling }: { job: Job; onCancel: () => void; canceling: boolean }) {
  const now = useNow(1000);
  const percent = progressPercent(job.progress);
  const startedMs = job.createdAt ? Date.parse(job.createdAt) : now;
  const elapsed = Math.max(0, now - startedMs);
  const radius = 52;
  const circumference = 2 * Math.PI * radius;
  const waiting = job.status === "queued";

  return (
    <div className="relative grid size-full place-items-center overflow-hidden bg-obsidian-950">
      <div className="absolute inset-0 viewport-grid opacity-40" />
      <div className="absolute left-1/2 top-1/2 size-[380px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-violet-500/10 blur-[100px]" />
      <div className="relative w-full max-w-sm px-6 text-center">
        <div className="relative mx-auto size-32">
          <svg viewBox="0 0 128 128" className="size-full -rotate-90">
            <circle cx="64" cy="64" r={radius} fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="5" />
            <circle
              cx="64"
              cy="64"
              r={radius}
              fill="none"
              stroke="url(#progressGradient)"
              strokeWidth="5"
              strokeLinecap="round"
              strokeDasharray={circumference}
              strokeDashoffset={circumference * (1 - Math.max(0.02, job.progress))}
              className="transition-[stroke-dashoffset] duration-700 ease-out"
            />
            <defs>
              <linearGradient id="progressGradient" x1="0" y1="0" x2="1" y2="1">
                <stop stopColor="#22d3ee" />
                <stop offset="0.6" stopColor="#818cf8" />
                <stop offset="1" stopColor="#c084fc" />
              </linearGradient>
            </defs>
          </svg>
          <div className="absolute inset-0 grid place-items-center">
            <div>
              <p className="font-mono text-2xl font-semibold text-white">{percent}%</p>
              <p className="mt-0.5 text-[10px] uppercase tracking-[0.16em] text-ink-500">
                {waiting ? "In queue" : "Running"}
              </p>
            </div>
          </div>
        </div>

        <p className="mt-5 text-sm font-medium text-white">{job.stageLabel ?? "Working"}</p>
        <p className="mt-1 min-h-[2.5rem] text-xs leading-relaxed text-ink-400">
          {job.message ||
            (waiting
              ? job.queuePosition && job.queuePosition > 0
                ? `Position ${job.queuePosition + 1} in the studio queue`
                : "Waiting for the GPU worker to pick this up"
              : "Real inference is running on the GPU worker. This is not a UI animation.")}
        </p>

        <div className="mt-5 flex items-center justify-center gap-2 text-[11px] text-ink-500">
          <span className="font-mono">{formatDuration(elapsed)}</span>
          <span className="text-ink-600">·</span>
          <span className="font-mono">{formatCount(job.params.gaussians)} splats</span>
          <span className="text-ink-600">·</span>
          <span className="font-mono">{job.params.steps ?? 50} steps</span>
        </div>

        <div className="mt-4 flex items-center justify-center gap-2">
          {job.input?.thumbUrl && (
            <img
              src={job.input.thumbUrl}
              alt="Input"
              className="size-10 rounded-lg border border-white/10 object-cover opacity-60"
            />
          )}
          <button
            type="button"
            onClick={onCancel}
            disabled={canceling}
            className="btn-ghost rounded-lg px-3 py-1.5 text-[11px] font-medium text-ink-200"
          >
            {canceling ? <Spinner className="size-3" /> : "Cancel"}
          </button>
        </div>
      </div>
    </div>
  );
}

export function ErrorState({ job, onRetry, retryDisabled }: { job: Job; onRetry: () => void; retryDisabled: boolean }) {
  const error = job.error;
  return (
    <div className="relative grid size-full place-items-center overflow-hidden bg-obsidian-950 px-6">
      <div className="absolute inset-0 viewport-grid opacity-30" />
      <div className="relative w-full max-w-md text-center">
        <div className="mx-auto grid size-12 place-items-center rounded-2xl border border-rose-400/25 bg-rose-500/10">
          <AlertOctagon size={20} className="text-rose-300" />
        </div>
        <p className="mt-4 text-[10px] font-semibold uppercase tracking-[0.24em] text-rose-300/80">
          {job.status === "canceled" ? "Canceled" : "Generation failed"}
        </p>
        <h2 className="mt-2 text-base font-medium text-white">
          {error?.message ?? "The worker could not finish this job."}
        </h2>
        {error?.hint && <p className="mt-2 text-xs leading-relaxed text-ink-400">{error.hint}</p>}
        {error?.details != null && (
          <details className="mt-4 text-left">
            <summary className="cursor-pointer text-[11px] text-ink-500 hover:text-ink-300">Worker details</summary>
            <pre className="mt-2 max-h-40 overflow-auto rounded-xl border border-white/10 bg-black/50 p-3 font-mono text-[10px] leading-relaxed text-ink-400">
              {typeof error.details === "string" ? error.details : JSON.stringify(error.details, null, 2)}
            </pre>
          </details>
        )}
        <div className="mt-5 flex items-center justify-center gap-2">
          <button
            type="button"
            onClick={onRetry}
            disabled={retryDisabled || error?.retryable === false}
            className="btn-primary flex items-center gap-2 rounded-xl px-4 py-2 text-xs font-semibold text-white"
          >
            <RefreshCcw size={13} /> Retry
          </button>
          {error?.code === "canceled" && (
            <span className="text-[11px] text-ink-500">Canceled by you.</span>
          )}
        </div>
        <p className="mt-4 font-mono text-[10px] text-ink-600">job {job.id}</p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Result chrome                                                      */
/* ------------------------------------------------------------------ */

export function ResultBar({ job }: { job: Job }) {
  const result = job.result;
  if (!result) return null;
  const ply = result.ply;
  const download = (url: string) => `${url}?download=1`;
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 z-20 bg-gradient-to-t from-black/90 via-black/40 to-transparent p-3 sm:p-4">
      <div className="pointer-events-auto flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="chip border-cyan-300/25 bg-cyan-400/10 text-cyan-100">
            <Sparkles size={11} /> {formatCount(result.gaussianCount)} Gaussians
          </span>
          {result.plyFormat && <span className="chip font-mono normal-case">{result.plyFormat}</span>}
          <span className="chip">{formatBytes(ply.sizeBytes)}</span>
          {result.glb && <span className="chip">{formatCount(result.glbVertices)} verts</span>}
          {typeof job.durationMs === "number" && <span className="chip">{formatDuration(job.durationMs)}</span>}
          {job.simulated && (
            <span className="chip border-violet-300/30 bg-violet-500/10 text-violet-200">simulated</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <a
            href={download(ply.url)}
            className="btn-primary flex items-center gap-2 rounded-xl px-3.5 py-2 text-xs font-semibold text-white"
            download={ply.name}
          >
            <Download size={14} /> PLY
          </a>
          {result.glb && (
            <a
              href={download(result.glb.url)}
              className="btn-ghost flex items-center gap-2 rounded-xl px-3.5 py-2 text-xs font-semibold text-ink-100"
              download={result.glb.name}
            >
              <Download size={14} /> GLB
            </a>
          )}
          <a
            href={`/api/jobs/${job.id}`}
            target="_blank"
            rel="noreferrer"
            className="btn-ghost hidden size-9 place-items-center rounded-xl text-ink-300 sm:grid"
            aria-label="Open this job's JSON record"
            title="Job JSON record"
          >
            <ArrowUpRight size={14} />
          </a>
        </div>
      </div>
    </div>
  );
}

export function PreviewGrid({ job }: { job: Job }) {
  const items = [
    { key: "source", label: "Source", note: "Your upload", url: job.input?.imageUrl },
    { key: "mask", label: "Foreground mask", note: "BiRefNet on the worker", url: job.preview?.mask?.url },
    { key: "modelInput", label: "Idol input", note: "1024² square crop on black", url: job.preview?.modelInput?.url },
    { key: "reference", label: "Predicted viewpoint", note: "Render Splat at Idol's camera", url: job.preview?.reference?.url },
  ].filter((item) => Boolean(item.url));

  if (items.length === 0) {
    return (
      <div className="grid size-full place-items-center p-6 text-center text-xs text-ink-500">
        This job has no 2D previews.
      </div>
    );
  }

  return (
    <div className="size-full overflow-auto bg-obsidian-950 p-4">
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
        {items.map((item) => (
          <a
            key={item.key}
            href={item.url}
            target="_blank"
            rel="noreferrer"
            className="group panel-flat overflow-hidden rounded-2xl"
          >
            <img src={item.url} alt={item.label} className="aspect-square w-full bg-black object-contain" />
            <div className="flex items-center justify-between px-2.5 py-2">
              <div>
                <p className="text-[11px] font-medium text-ink-200">{item.label}</p>
                <p className="text-[10px] text-ink-500">{item.note}</p>
              </div>
              <ArrowUpRight size={12} className="text-ink-600 transition group-hover:text-cyan-300" />
            </div>
          </a>
        ))}
      </div>
    </div>
  );
}

export function DetailsPanel({ job }: { job: Job }) {
  const rows: Array<[string, string]> = [
    ["Job", job.id],
    ["Status", job.status],
    ["Mode", job.params.mode],
    ["Gaussians", formatCount(job.params.gaussians)],
    ["Steps", String(job.params.steps ?? "—")],
    ["CFG", String(job.params.cfg ?? "—")],
    ["Seed", job.params.seed == null ? "random" : String(job.params.seed)],
    ["Precision", job.params.precision ?? "—"],
    ["PLY", job.result ? `${job.result.plyFormat ?? "ply"} · ${formatBytes(job.result.ply.sizeBytes)}` : "—"],
    ["GLB", job.result?.glb ? `${formatCount(job.result.glbVertices)} verts / ${formatCount(job.result.glbFaces)} tris` : "not requested"],
    ["Duration", formatDuration(job.durationMs)],
    ["Worker", job.simulated ? "simulated (development)" : "ComfyUI GPU worker"],
  ];
  return (
    <div className="size-full overflow-auto bg-obsidian-950 p-4 sm:p-6">
      <div className="mx-auto max-w-2xl">
        <p className="label">Inference record</p>
        <h3 className="mt-1 text-sm font-medium text-white">What produced this file</h3>
        <dl className="mt-4 divide-y divide-white/5 overflow-hidden rounded-2xl border border-white/8">
          {rows.map(([label, value]) => (
            <div key={label} className="flex items-start gap-4 px-3 py-2.5">
              <dt className="w-28 shrink-0 text-[11px] text-ink-500">{label}</dt>
              <dd className="min-w-0 break-words font-mono text-[11px] text-ink-200">{value}</dd>
            </div>
          ))}
        </dl>
        {job.result?.glbError && (
          <p className="mt-3 rounded-xl border border-amber-400/25 bg-amber-500/10 px-3 py-2 text-[11px] text-amber-100">
            GLB conversion failed: {job.result.glbError}
          </p>
        )}
        <p className="mt-4 text-[11px] leading-relaxed text-ink-500">
          Files are verified on the server before they are published: the PLY header and vertex count are parsed,
          and the optional GLB is read as a glTF container. Downloads always come from that verified copy.
        </p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Toolbar                                                            */
/* ------------------------------------------------------------------ */

export function ViewportToolbar({
  visible,
  autoRotate,
  onToggleAutoRotate,
  onReset,
  upright,
  onToggleUpright,
  showUpright,
  onFullscreen,
}: {
  visible: boolean;
  autoRotate: boolean;
  onToggleAutoRotate: () => void;
  onReset: () => void;
  upright: boolean;
  onToggleUpright: () => void;
  showUpright: boolean;
  onFullscreen: () => void;
}) {
  if (!visible) return null;
  return (
    <div className="absolute right-3 top-3 z-20 flex items-center gap-1 rounded-xl border border-white/10 bg-black/45 p-1 backdrop-blur-md">
      <ToolbarButton active={autoRotate} onClick={onToggleAutoRotate} label="Auto-rotate">
        <RotateCw size={14} />
      </ToolbarButton>
      <ToolbarButton onClick={onReset} label="Reset camera">
        <RotateCcw size={14} />
      </ToolbarButton>
      {showUpright && (
        <ToolbarButton active={!upright} onClick={onToggleUpright} label="Rotate model 90°">
          <RotateCw size={14} className="rotate-90" />
        </ToolbarButton>
      )}
      <ToolbarButton onClick={onFullscreen} label="Fullscreen">
        <svg viewBox="0 0 24 24" className="size-3.5" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" strokeLinecap="round" />
        </svg>
      </ToolbarButton>
    </div>
  );
}

function ToolbarButton({
  children,
  onClick,
  label,
  active,
}: {
  children: React.ReactNode;
  onClick: () => void;
  label: string;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-pressed={active}
      className={cn(
        "grid size-8 place-items-center rounded-lg transition",
        active ? "bg-cyan-400/20 text-cyan-200" : "text-ink-300 hover:bg-white/8 hover:text-white",
      )}
    >
      {children}
    </button>
  );
}
