import { useCallback, useEffect, useRef, useState } from "react";
import { Boxes, CheckCircle2, Images, Info, Maximize2, Sparkles } from "lucide-react";
import { cn } from "../lib/cn";
import type { Job } from "../lib/types";
import { formatCount } from "../lib/format";
import type { SplatViewerHandle } from "../viewer/splat";
import type { MeshViewerHandle } from "../viewer/mesh";
import {
  DetailsPanel,
  EmptyViewport,
  ErrorState,
  MeshCanvas,
  PreviewGrid,
  ProgressOverlay,
  ResultBar,
  SplatCanvas,
  ViewportToolbar,
} from "./viewport";

type ViewTab = "splat" | "mesh" | "previews" | "details";

interface ViewerStageProps {
  job: Job | null;
  hasUpload: boolean;
  onCancel: () => void;
  onRetry: () => void;
  retrying: boolean;
  onOpenHistory: () => void;
}

export function ViewerStage({ job, hasUpload, onCancel, onRetry, retrying, onOpenHistory }: ViewerStageProps) {
  const [tab, setTab] = useState<ViewTab>("splat");
  const [autoRotate, setAutoRotate] = useState(false);
  const [upright, setUpright] = useState(true);
  const [canceling, setCanceling] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [splatReady, setSplatReady] = useState(false);
  const splatRef = useRef<SplatViewerHandle | null>(null);
  const meshRef = useRef<MeshViewerHandle | null>(null);
  const sectionRef = useRef<HTMLElement>(null);

  const succeeded = job?.status === "succeeded" && Boolean(job.result);
  const hasGlb = Boolean(job?.result?.glb);

  const jobId = job?.id ?? null;
  const jobStatus = job?.status ?? null;

  useEffect(() => {
    setSplatReady(false);
    if (!jobId || jobStatus === "succeeded") setTab("splat");
  }, [jobId, jobStatus]);

  useEffect(() => {
    const onChange = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const requestFullscreen = useCallback(async () => {
    const element = sectionRef.current;
    if (!element) return;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await element.requestFullscreen();
    } catch {
      /* denied by the browser */
    }
  }, []);

  const handleCancel = useCallback(async () => {
    setCanceling(true);
    try {
      onCancel();
    } finally {
      window.setTimeout(() => setCanceling(false), 700);
    }
  }, [onCancel]);

  const resetView = useCallback(() => {
    if (tab === "mesh") meshRef.current?.reset();
    else splatRef.current?.reset();
  }, [tab]);

  const tabs: Array<{ id: ViewTab; label: string; icon: React.ReactNode; available: boolean }> = [
    { id: "splat", label: "Splat", icon: <Sparkles size={12} />, available: Boolean(job?.result?.ply) },
    { id: "mesh", label: "Mesh", icon: <Boxes size={12} />, available: hasGlb },
    { id: "previews", label: "Previews", icon: <Images size={12} />, available: Boolean(job?.preview) },
    { id: "details", label: "Details", icon: <Info size={12} />, available: Boolean(job) },
  ];

  return (
    <section
      ref={sectionRef}
      className={cn(
        "panel relative flex min-h-[62vh] flex-col overflow-hidden rounded-3xl lg:min-h-0 lg:h-[calc(100dvh-6.5rem)]",
        fullscreen && "h-dvh rounded-none",
      )}
      aria-label="3D viewport"
    >
      <div className="flex items-center gap-3 border-b border-white/[0.06] px-3 py-2.5 sm:px-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold uppercase tracking-[0.24em] text-cyan-300/80">Viewport</span>
            {succeeded && (
              <span className="hidden items-center gap-1 text-[10px] text-emerald-300/90 sm:flex">
                <CheckCircle2 size={11} /> verified PLY
              </span>
            )}
          </div>
          <p className="mt-0.5 truncate font-mono text-[10px] text-ink-500">
            {job
              ? `${job.id}${job.result ? ` · ${formatCount(job.result.gaussianCount)} gaussians` : ""}`
              : "Three.js · Gaussian splatting"}
          </p>
        </div>

        <div className="ml-auto flex items-center gap-1">
          {succeeded &&
            tabs
              .filter((item) => item.available)
              .map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setTab(item.id)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-[11px] font-medium transition sm:px-2.5",
                    tab === item.id ? "bg-white/10 text-white" : "text-ink-400 hover:text-ink-100",
                  )}
                  aria-pressed={tab === item.id}
                >
                  {item.icon}
                  <span className="hidden sm:inline">{item.label}</span>
                </button>
              ))}
          <button
            type="button"
            onClick={onOpenHistory}
            className="ml-1 hidden rounded-lg px-2 py-1.5 text-[11px] text-ink-500 transition hover:text-ink-200 sm:block"
          >
            History
          </button>
          <button
            type="button"
            onClick={() => void requestFullscreen()}
            className="grid size-8 place-items-center rounded-lg text-ink-400 transition hover:bg-white/8 hover:text-white"
            aria-label="Toggle fullscreen"
          >
            <Maximize2 size={13} />
          </button>
        </div>
      </div>

      <div className="relative min-h-[420px] flex-1 overflow-hidden bg-obsidian-950">
        {!job && <EmptyViewport hasUpload={hasUpload} />}

        {job && (job.status === "queued" || job.status === "running") && (
          <ProgressOverlay job={job} onCancel={() => void handleCancel()} canceling={canceling} />
        )}

        {job && (job.status === "failed" || job.status === "canceled") && (
          <ErrorState job={job} onRetry={onRetry} retryDisabled={retrying} />
        )}

        {job && succeeded && job.result && (
          <>
            {tab === "splat" && (
              <SplatCanvas
                url={job.result.ply.url}
                autoRotate={autoRotate}
                upright={upright}
                controllerRef={splatRef}
                onReady={setSplatReady}
              />
            )}
            {tab === "mesh" && job.result.glb && (
              <MeshCanvas url={job.result.glb.url} autoRotate={autoRotate} controllerRef={meshRef} />
            )}
            {tab === "previews" && <PreviewGrid job={job} />}
            {tab === "details" && <DetailsPanel job={job} />}
          </>
        )}

        {job && succeeded && (tab === "splat" || tab === "mesh") && (
          <ViewportToolbar
            visible={tab === "mesh" ? true : splatReady}
            autoRotate={autoRotate}
            onToggleAutoRotate={() => setAutoRotate((value) => !value)}
            onReset={resetView}
            upright={upright}
            onToggleUpright={() => setUpright((value) => !value)}
            showUpright={tab === "splat"}
            onFullscreen={() => void requestFullscreen()}
          />
        )}

        {job && succeeded && (tab === "splat" || tab === "mesh") && <ResultBar job={job} />}
      </div>

      {job && succeeded && (
        <div className="border-t border-white/[0.06] px-3 py-2 text-[10px] leading-relaxed text-ink-500 sm:px-4">
          Drag to orbit · scroll or pinch to zoom · right-drag or two-finger drag to pan. Rendered from the verified PLY
          {hasGlb ? " (Mesh tab shows the extracted GLB)" : ""}.
        </div>
      )}
    </section>
  );
}
