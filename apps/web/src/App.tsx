import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Boxes, Image as ImageIcon, Plus, Sparkles } from "lucide-react";
import { cn } from "./lib/cn";
import { ApiError, api } from "./lib/api";
import type { Job } from "./lib/types";
import { isTerminal, useGeneration, useHistory, usePreprocess, useStudioMeta, useUploader } from "./hooks/studio";
import { Header } from "./components/Header";
import { StudioPanel } from "./components/StudioPanel";
import { ViewerStage } from "./components/ViewerStage";
import { HistoryPanel } from "./components/HistoryPanel";
import { useToast, ToastProvider } from "./components/Toasts";
import type { GenerationSettings } from "./components/SettingsCard";

const STORAGE_KEY = "idol-studio:settings:v1";

const DEFAULT_SETTINGS: GenerationSettings = {
  mode: "synthetic",
  gaussians: 262_144,
  steps: 50,
  cfg: 4,
  seed: null,
  precision: "default",
  exportGlb: false,
  meshResolution: 384,
};

function loadSettings(): GenerationSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed = JSON.parse(raw) as Partial<GenerationSettings>;
    return { ...DEFAULT_SETTINGS, ...parsed, seed: null };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

type MobileTab = "create" | "view";

function Studio() {
  const toast = useToast();
  const meta = useStudioMeta();
  const uploader = useUploader();
  const history = useHistory();
  const [settings, setSettings] = useState<GenerationSettings>(loadSettings);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [mobileTab, setMobileTab] = useState<MobileTab>("create");
  const [retrying, setRetrying] = useState(false);
  const notified = useRef<string | null>(null);

  const onSettled = useCallback(() => {
    void history.refresh();
    void meta.refresh();
  }, [history, meta]);

  const generation = useGeneration(onSettled);
  const preprocess = usePreprocess(
    uploader.upload,
    settings.mode,
    Boolean(meta.config?.features.preprocess),
  );

  // Keep the form inside the server's bounds once config arrives.
  useEffect(() => {
    if (!meta.config) return;
    setSettings((current) => ({
      ...current,
      gaussians: Math.min(Math.max(current.gaussians, meta.config!.gaussians.min), meta.config!.gaussians.max),
      exportGlb: current.exportGlb && meta.config!.features.glb,
    }));
  }, [meta.config]);

  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
      /* storage may be unavailable */
    }
  }, [settings]);

  const changeSettings = useCallback((patch: Partial<GenerationSettings>) => {
    setSettings((current) => ({ ...current, ...patch }));
  }, []);

  const job = generation.job;
  const running = Boolean(job && (job.status === "queued" || job.status === "running"));

  const blockedReason = useMemo(() => {
    if (!meta.health) return "Checking the GPU worker…";
    if (!meta.health.worker.reachable) {
      return `The GPU worker is offline${meta.health.worker.error ? `: ${meta.health.worker.error}` : "."}`;
    }
    if (meta.health.worker.simulated && !meta.health.allowSimulatedWorker) {
      return "The studio is connected to the development simulator, which cannot produce real Idol inference.";
    }
    if (meta.health.capabilities.missingModels.length > 0) {
      return "Idol model files are missing on the worker. See docs/MODEL_REQUIREMENTS.md.";
    }
    if (meta.health.capabilities.missingNodes.length > 0) {
      return "The worker is missing ComfyUI nodes required by Idol (install ComfyUI 0.43+ and ComfyUI-Idol).";
    }
    if (!uploader.upload) return "Add an image to begin.";
    return null;
  }, [meta.health, uploader.upload]);

  // A simulated worker is only ever reachable in development (the API refuses
  // to boot with it in production). It is allowed here so the dev environment
  // stays usable, but every job it produces is labelled "simulated".
  const simulated = Boolean(meta.health?.worker.simulated);
  const simulationAllowed = simulated && Boolean(meta.health?.allowSimulatedWorker);
  const canGenerate = Boolean(
    uploader.upload && meta.health?.ok && (!simulated || simulationAllowed) && !running && !generation.starting,
  );

  const handleGenerate = useCallback(async () => {
    if (!uploader.upload || !meta.config) return;
    setMobileTab("view");
    try {
      await generation.start({
        uploadId: uploader.upload.uploadId,
        mode: settings.mode,
        gaussians: settings.gaussians,
        steps: settings.steps,
        cfg: settings.cfg,
        seed: settings.seed,
        precision: settings.precision,
        exportGlb: settings.exportGlb && Boolean(meta.config.features.glb),
        meshResolution: settings.meshResolution,
      });
    } catch (error) {
      const apiError = error instanceof ApiError ? error : new ApiError("Could not start the generation.");
      toast.push({
        kind: "error",
        title: "Generation could not start",
        message: apiError.message,
        hint: apiError.hint,
      });
    }
  }, [generation, meta.config, settings, toast, uploader.upload]);

  const handleCancel = useCallback(() => {
    void generation.cancel();
  }, [generation]);

  const handleRetryActive = useCallback(async () => {
    setRetrying(true);
    try {
      const created = await generation.retry();
      if (created) {
        setMobileTab("view");
        toast.push({ kind: "info", title: "Retrying on the GPU worker", message: "A new job was queued." });
      }
    } finally {
      setRetrying(false);
    }
  }, [generation, toast]);

  const handleOpenJob = useCallback(
    (next: Job) => {
      generation.open(next);
      setHistoryOpen(false);
      setMobileTab("view");
    },
    [generation],
  );

  const handleRetryJob = useCallback(
    async (source: Job) => {
      try {
        const created = await api.retry(source.id);
        generation.open(created);
        setHistoryOpen(false);
        setMobileTab("view");
        toast.push({ kind: "info", title: "Retrying", message: "A new job was queued on the GPU worker." });
      } catch (error) {
        const apiError = error instanceof ApiError ? error : new ApiError("Could not retry that job.");
        toast.push({ kind: "error", title: "Retry failed", message: apiError.message, hint: apiError.hint });
      }
    },
    [generation, toast],
  );

  const handleDeleteJob = useCallback(
    async (target: Job) => {
      try {
        await history.remove(target.id);
        if (generation.job?.id === target.id) generation.reset();
        toast.push({ kind: "success", title: "Job deleted", message: "Its files were removed from the studio." });
      } catch (error) {
        const apiError = error instanceof ApiError ? error : new ApiError("Could not delete that job.");
        toast.push({ kind: "error", title: "Delete failed", message: apiError.message });
      }
    },
    [generation, history, toast],
  );

  const handleClear = useCallback(() => {
    uploader.clear();
    generation.reset();
  }, [generation, uploader]);

  // Notify once per terminal job.
  useEffect(() => {
    if (!job || !isTerminal(job) || notified.current === job.id) return;
    notified.current = job.id;
    if (job.status === "succeeded") {
      toast.push({
        kind: "success",
        title: "3D splat ready",
        message: job.result
          ? `${job.result.gaussianCount.toLocaleString("en-US")} Gaussians verified · ${(job.result.ply.sizeBytes / 1_048_576).toFixed(1)} MB`
          : "The PLY is ready to download.",
      });
    } else if (job.status === "failed") {
      toast.push({
        kind: "error",
        title: "Generation failed",
        message: job.error?.message ?? "The worker could not finish this job.",
        hint: job.error?.hint,
      });
    }
  }, [job, toast]);

  const historyCount = history.jobs.filter((item) => item.status === "succeeded").length;

  return (
    <div className="app-bg grain relative min-h-dvh">
      <Header
        health={meta.health}
        checking={meta.loading}
        onRefresh={() => void meta.refresh()}
        onOpenHistory={() => setHistoryOpen(true)}
        historyCount={historyCount}
      />

      <main className="relative mx-auto max-w-[1640px] px-3 pb-24 pt-4 sm:px-5 lg:pb-6">
        <div className="grid items-start gap-4 lg:grid-cols-[minmax(340px,380px)_minmax(0,1fr)] xl:grid-cols-[minmax(380px,420px)_minmax(0,1fr)]">
          <div className={cn("space-y-4", mobileTab !== "create" && "hidden lg:block")}>
            <StudioPanel
              config={meta.config}
              upload={uploader.upload}
              uploading={uploader.uploading}
              uploadError={uploader.error}
              preprocessJob={preprocess.job}
              preprocessBusy={preprocess.busy}
              preprocessError={preprocess.error}
              settings={settings}
              onSettingsChange={changeSettings}
              onFile={(file) => void uploader.send(file)}
              onSample={(url) => void uploader.sendSample(url)}
              onClear={handleClear}
              onGenerate={() => void handleGenerate()}
              canGenerate={canGenerate}
              simulated={simulated}
              generating={generation.starting}
              blockedReason={blockedReason}
            />
            <p className="px-1 text-[10px] leading-relaxed text-ink-600">
              Images and generated files are stored on this studio's server. Idol weights and architecture remain
              Apache-2.0 / GPL-3.0 as published by speridlabs and PedroMarinhoDev.
            </p>
          </div>

          <div className={cn(mobileTab !== "view" && "hidden lg:block")}>
            <ViewerStage
              job={job}
              hasUpload={Boolean(uploader.upload)}
              onCancel={handleCancel}
              onRetry={() => void handleRetryActive()}
              retrying={retrying}
              onOpenHistory={() => setHistoryOpen(true)}
            />
          </div>
        </div>
      </main>

      <nav className="fixed inset-x-0 bottom-0 z-40 border-t border-white/10 bg-obsidian-950/90 px-4 py-2 backdrop-blur-xl lg:hidden">
        <div className="mx-auto flex max-w-md items-center justify-around">
          <MobileTabButton
            active={mobileTab === "create"}
            onClick={() => setMobileTab("create")}
            icon={uploader.upload ? <ImageIcon size={16} /> : <Plus size={16} />}
            label="Create"
          />
          <MobileTabButton
            active={mobileTab === "view"}
            onClick={() => setMobileTab("view")}
            icon={job?.status === "succeeded" ? <Sparkles size={16} /> : <Boxes size={16} />}
            label="Viewport"
            badge={running ? "live" : undefined}
          />
          <MobileTabButton
            active={historyOpen}
            onClick={() => setHistoryOpen(true)}
            icon={<Boxes size={16} className="rotate-45" />}
            label="History"
          />
        </div>
      </nav>

      <HistoryPanel
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
        jobs={history.jobs}
        loading={history.loading}
        error={history.error}
        activeJobId={job?.id ?? null}
        onOpenJob={handleOpenJob}
        onRetryJob={(next) => void handleRetryJob(next)}
        onDeleteJob={handleDeleteJob}
        onRefresh={() => void history.refresh()}
      />
    </div>
  );
}

function MobileTabButton({
  active,
  onClick,
  icon,
  label,
  badge,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  badge?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "relative flex min-w-20 flex-col items-center gap-0.5 rounded-xl px-3 py-1.5 text-[10px] font-medium transition",
        active ? "text-cyan-200" : "text-ink-500 hover:text-ink-200",
      )}
    >
      {icon}
      {label}
      {badge && (
        <span className="absolute right-2 top-0 rounded-full bg-cyan-400/20 px-1 text-[8px] uppercase tracking-wide text-cyan-200">
          {badge}
        </span>
      )}
    </button>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <Studio />
    </ToastProvider>
  );
}

