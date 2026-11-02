import { History, RefreshCw, TriangleAlert } from "lucide-react";
import { cn } from "../lib/cn";
import type { Health } from "../lib/types";

interface HeaderProps {
  health: Health | null;
  checking: boolean;
  onRefresh: () => void;
  onOpenHistory: () => void;
  historyCount: number;
}

function workerState(health: Health | null): {
  label: string;
  tone: "ok" | "warn" | "bad" | "sim";
  detail: string;
} {
  if (!health) {
    return { label: "Connecting", tone: "warn", detail: "Checking the GPU worker…" };
  }
  if (!health.worker.reachable) {
    return {
      label: "Worker offline",
      tone: "bad",
      detail: health.worker.error ?? "The GPU worker is not reachable.",
    };
  }
  if (health.worker.simulated) {
    return {
      label: "Simulated worker",
      tone: "sim",
      detail:
        "Connected to the development simulator: it produces placeholder geometry, not Idol inference.",
    };
  }
  if (health.capabilities.missingModels.length > 0) {
    return {
      label: "Models missing",
      tone: "warn",
      detail: health.capabilities.missingModels.join(" · "),
    };
  }
  if (health.capabilities.missingNodes.length > 0) {
    return {
      label: "Nodes missing",
      tone: "warn",
      detail: health.capabilities.missingNodes.join(", "),
    };
  }
  if (!health.ok) {
    return { label: "Worker not ready", tone: "warn", detail: "The worker is reachable but not ready." };
  }
  return { label: "GPU worker ready", tone: "ok", detail: "Idol is loaded and ready for inference." };
}

export function Header({ health, checking, onRefresh, onOpenHistory, historyCount }: HeaderProps) {
  const state = workerState(health);
  const gpu = health?.worker.devices?.[0]?.name as string | undefined;

  return (
    <header className="sticky top-0 z-40 border-b border-white/[0.06] bg-obsidian-950/70 backdrop-blur-xl">
      <div className="mx-auto flex h-16 max-w-[1600px] items-center gap-3 px-4 sm:px-6">
        <div className="flex min-w-0 items-center gap-3">
          <div className="relative grid size-9 shrink-0 place-items-center rounded-xl border border-white/10 bg-gradient-to-br from-cyan-400/20 via-transparent to-violet-500/25">
            <svg viewBox="0 0 24 24" className="size-5" fill="none" aria-hidden>
              <path d="M12 2.6 20 7v10l-8 4.4L4 17V7l8-4.4Z" stroke="url(#g)" strokeWidth="1.4" />
              <path d="M4.4 7.2 12 11.5l7.6-4.3M12 11.5V21" stroke="url(#g)" strokeWidth="1.2" opacity=".85" />
              <defs>
                <linearGradient id="g" x1="4" y1="3" x2="20" y2="21">
                  <stop stopColor="#67e8f9" />
                  <stop offset="1" stopColor="#c084fc" />
                </linearGradient>
              </defs>
            </svg>
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-[15px] font-semibold tracking-[0.22em] text-white">IDOL</span>
              <span className="text-[10px] font-medium uppercase tracking-[0.28em] text-cyan-300/80">Studio</span>
            </div>
            <p className="hidden truncate text-[11px] text-ink-400 sm:block">
              One image → 3D Gaussian splat · real inference on ComfyUI
            </p>
          </div>
        </div>

        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            onClick={onRefresh}
            className="group hidden items-center gap-2 rounded-full border border-white/8 bg-white/[0.03] px-3 py-1.5 text-[11px] font-medium text-ink-300 transition hover:border-white/15 hover:text-white sm:flex"
            title={state.detail}
          >
            <span className="relative flex size-2">
              <span
                className={cn(
                  "absolute inline-flex size-full rounded-full opacity-60",
                  state.tone === "ok" && "animate-ping bg-emerald-400",
                )}
              />
              <span
                className={cn(
                  "relative inline-flex size-2 rounded-full",
                  state.tone === "ok" && "bg-emerald-400",
                  state.tone === "warn" && "bg-amber-400",
                  state.tone === "bad" && "bg-rose-400",
                  state.tone === "sim" && "bg-violet-400",
                )}
              />
            </span>
            <span className="max-w-[190px] truncate">{state.label}</span>
            {gpu && state.tone === "ok" && (
              <span className="hidden max-w-[150px] truncate text-ink-500 lg:inline">{gpu}</span>
            )}
            <RefreshCw size={11} className={cn("text-ink-500 transition group-hover:rotate-90", checking && "animate-spin")} />
          </button>

          <button
            type="button"
            onClick={onOpenHistory}
            className="btn-ghost flex items-center gap-2 rounded-full px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-ink-200 sm:px-4"
          >
            <History size={14} />
            <span className="hidden sm:inline">History</span>
            {historyCount > 0 && (
              <span className="rounded-full bg-white/10 px-1.5 py-0.5 text-[10px] text-white">{historyCount}</span>
            )}
          </button>
        </div>
      </div>

      {(state.tone === "sim" || state.tone === "bad" || state.tone === "warn") && (
        <div
          className={cn(
            "border-t px-4 py-2 text-[11px] sm:px-6",
            state.tone === "bad"
              ? "border-rose-400/20 bg-rose-500/8 text-rose-200"
              : state.tone === "sim"
                ? "border-violet-400/20 bg-violet-500/8 text-violet-200"
                : "border-amber-400/20 bg-amber-500/8 text-amber-100",
          )}
        >
          <div className="mx-auto flex max-w-[1600px] items-start gap-2">
            <TriangleAlert size={13} className="mt-0.5 shrink-0" />
            <p className="min-w-0 flex-1">
              <span className="font-semibold">{state.label}. </span>
              {state.detail}
            </p>
          </div>
        </div>
      )}
    </header>
  );
}
