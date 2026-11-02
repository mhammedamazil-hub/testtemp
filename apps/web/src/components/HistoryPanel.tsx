import { useEffect, useMemo, useState } from "react";
import { Download, History, Loader2, RefreshCcw, RotateCcw, Trash2, X } from "lucide-react";
import { cn } from "../lib/cn";
import { formatBytes, formatCount, formatDuration, relativeTime } from "../lib/format";
import type { ApiError } from "../lib/api";
import type { Job } from "../lib/types";

interface HistoryPanelProps {
  open: boolean;
  onClose: () => void;
  jobs: Job[];
  loading: boolean;
  error: ApiError | null;
  activeJobId?: string | null;
  onOpenJob: (job: Job) => void;
  onRetryJob: (job: Job) => void;
  onDeleteJob: (job: Job) => Promise<void>;
  onRefresh: () => void;
}

type Filter = "all" | "done" | "failed";

function statusTone(job: Job): string {
  switch (job.status) {
    case "succeeded":
      return "text-emerald-300";
    case "failed":
      return "text-rose-300";
    case "canceled":
      return "text-amber-300";
    case "running":
      return "text-cyan-300";
    default:
      return "text-ink-400";
  }
}

export function HistoryPanel({
  open,
  onClose,
  jobs,
  loading,
  error,
  activeJobId,
  onOpenJob,
  onRetryJob,
  onDeleteJob,
  onRefresh,
}: HistoryPanelProps) {
  const [filter, setFilter] = useState<Filter>("all");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  useEffect(() => {
    if (!open) setPendingDelete(null);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const filtered = useMemo(() => {
    if (filter === "done") return jobs.filter((job) => job.status === "succeeded");
    if (filter === "failed") return jobs.filter((job) => job.status === "failed" || job.status === "canceled");
    return jobs;
  }, [jobs, filter]);

  return (
    <>
      <div
        className={cn(
          "fixed inset-0 z-50 bg-black/60 backdrop-blur-sm transition-opacity duration-300",
          open ? "opacity-100" : "pointer-events-none opacity-0",
        )}
        onClick={onClose}
        aria-hidden
      />
      <aside
        role="dialog"
        aria-modal="true"
        aria-label="Generation history"
        className={cn(
          "fixed inset-y-0 right-0 z-50 flex w-full max-w-md flex-col border-l border-white/10 bg-obsidian-900/95 shadow-2xl backdrop-blur-2xl transition-transform duration-300 ease-out",
          open ? "translate-x-0" : "translate-x-full",
        )}
      >
        <div className="flex items-center gap-2 border-b border-white/[0.06] px-4 py-3">
          <div className="grid size-8 place-items-center rounded-xl border border-white/10 bg-white/[0.04]">
            <History size={14} className="text-cyan-300" />
          </div>
          <div>
            <p className="text-sm font-medium text-white">History</p>
            <p className="text-[11px] text-ink-500">Stored on the studio server</p>
          </div>
          <div className="ml-auto flex items-center gap-1">
            <button
              type="button"
              onClick={onRefresh}
              className="grid size-8 place-items-center rounded-lg text-ink-400 transition hover:bg-white/8 hover:text-white"
              aria-label="Refresh history"
            >
              <RefreshCcw size={13} className={cn(loading && "animate-spin")} />
            </button>
            <button
              type="button"
              onClick={onClose}
              className="grid size-8 place-items-center rounded-lg text-ink-400 transition hover:bg-white/8 hover:text-white"
              aria-label="Close history"
            >
              <X size={15} />
            </button>
          </div>
        </div>

        <div className="flex gap-1 border-b border-white/[0.06] px-3 py-2">
          {(
            [
              { id: "all", label: "All" },
              { id: "done", label: "Completed" },
              { id: "failed", label: "Failed" },
            ] as Array<{ id: Filter; label: string }>
          ).map((option) => (
            <button
              key={option.id}
              type="button"
              onClick={() => setFilter(option.id)}
              className={cn(
                "rounded-lg px-2.5 py-1 text-[11px] font-medium transition",
                filter === option.id ? "bg-white/10 text-white" : "text-ink-400 hover:text-ink-100",
              )}
            >
              {option.label}
            </button>
          ))}
          <span className="ml-auto self-center text-[10px] text-ink-600">{filtered.length} jobs</span>
        </div>

        <div className="flex-1 overflow-y-auto p-3">
          {error && (
            <p className="mb-3 rounded-xl border border-rose-400/25 bg-rose-500/10 px-3 py-2 text-[11px] text-rose-200">
              {error.message}
            </p>
          )}
          {loading && jobs.length === 0 && (
            <div className="grid place-items-center py-16">
              <Loader2 className="size-5 animate-spin text-ink-500" />
            </div>
          )}
          {!loading && filtered.length === 0 && (
            <div className="grid place-items-center py-20 text-center">
              <p className="text-sm text-ink-300">Nothing here yet</p>
              <p className="mt-1 max-w-[16rem] text-[11px] leading-relaxed text-ink-500">
                Every generation is kept with its image, settings, files and verified statistics.
              </p>
            </div>
          )}

          <div className="space-y-2">
            {filtered.map((job) => {
              const thumbnail = job.preview?.reference?.url ?? job.input?.thumbUrl ?? job.input?.imageUrl;
              const active = job.id === activeJobId;
              return (
                <article
                  key={job.id}
                  className={cn(
                    "group overflow-hidden rounded-2xl border transition",
                    active
                      ? "border-cyan-300/35 bg-cyan-400/[0.06]"
                      : "border-white/8 bg-white/[0.02] hover:border-white/15",
                  )}
                >
                  <button
                    type="button"
                    onClick={() => onOpenJob(job)}
                    className="flex w-full gap-3 p-2.5 text-left"
                  >
                    <div className="relative size-16 shrink-0 overflow-hidden rounded-xl border border-white/10 bg-black/50">
                      {thumbnail ? (
                        <img src={thumbnail} alt="" className="size-full object-cover" />
                      ) : (
                        <div className="grid size-full place-items-center text-ink-600">
                          <History size={16} />
                        </div>
                      )}
                      {job.status === "running" && (
                        <div className="absolute inset-0 grid place-items-center bg-black/50">
                          <Loader2 size={14} className="animate-spin text-cyan-300" />
                        </div>
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className={cn("text-[10px] font-semibold uppercase tracking-[0.14em]", statusTone(job))}>
                          {job.status}
                        </span>
                        <span className="text-[10px] text-ink-600">{relativeTime(job.createdAt)}</span>
                        {job.simulated && (
                          <span className="rounded bg-violet-500/15 px-1 py-0.5 text-[9px] uppercase tracking-wide text-violet-200">
                            sim
                          </span>
                        )}
                      </div>
                      <p className="mt-1 truncate text-[11px] text-ink-200">
                        {job.kind === "preprocess" ? "Background preview" : `${formatCount(job.params.gaussians)} splats`}
                        <span className="text-ink-600"> · </span>
                        {job.params.mode === "real" ? "photo" : "render"}
                        {job.result ? (
                          <>
                            <span className="text-ink-600"> · </span>
                            {formatBytes(job.result.ply.sizeBytes)}
                          </>
                        ) : null}
                      </p>
                      <p className="mt-0.5 truncate font-mono text-[10px] text-ink-500">
                        {job.error?.message ??
                          (job.status === "succeeded"
                            ? `${formatDuration(job.durationMs)} · seed ${job.params.seed ?? "random"}`
                            : job.stageLabel ?? "")}
                      </p>
                    </div>
                  </button>

                  <div className="flex items-center gap-1 border-t border-white/[0.05] px-2 py-1.5">
                    {job.status === "succeeded" && job.result && (
                      <>
                        <a
                          href={`${job.result.ply.url}?download=1`}
                          download={job.result.ply.name}
                          className="flex items-center gap-1 rounded-lg px-2 py-1 text-[10px] text-ink-300 transition hover:bg-white/8 hover:text-white"
                        >
                          <Download size={11} /> PLY
                        </a>
                        {job.result.glb && (
                          <a
                            href={`${job.result.glb.url}?download=1`}
                            download={job.result.glb.name}
                            className="flex items-center gap-1 rounded-lg px-2 py-1 text-[10px] text-ink-300 transition hover:bg-white/8 hover:text-white"
                          >
                            <Download size={11} /> GLB
                          </a>
                        )}
                      </>
                    )}
                    {(job.status === "failed" || job.status === "canceled") && (
                      <button
                        type="button"
                        onClick={() => onRetryJob(job)}
                        className="flex items-center gap-1 rounded-lg px-2 py-1 text-[10px] text-ink-300 transition hover:bg-white/8 hover:text-white"
                      >
                        <RotateCcw size={11} /> Retry
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => onOpenJob(job)}
                      className="ml-auto rounded-lg px-2 py-1 text-[10px] text-ink-400 transition hover:bg-white/8 hover:text-white"
                    >
                      Open
                    </button>
                    <button
                      type="button"
                      onClick={async () => {
                        if (pendingDelete === job.id) {
                          setPendingDelete(null);
                          await onDeleteJob(job);
                        } else {
                          setPendingDelete(job.id);
                          window.setTimeout(() => {
                            setPendingDelete((current) => (current === job.id ? null : current));
                          }, 3500);
                        }
                      }}
                      disabled={job.status === "running" || job.status === "queued"}
                      className={cn(
                        "flex items-center gap-1 rounded-lg px-2 py-1 text-[10px] transition hover:bg-rose-500/15 hover:text-rose-200",
                        pendingDelete === job.id ? "text-rose-300" : "text-ink-500",
                        (job.status === "running" || job.status === "queued") && "cursor-not-allowed opacity-40",
                      )}
                      title="Delete job and its files"
                    >
                      <Trash2 size={11} />
                      {pendingDelete === job.id ? "Sure?" : ""}
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        </div>
      </aside>
    </>
  );
}
