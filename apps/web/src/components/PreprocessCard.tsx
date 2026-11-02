import { useEffect, useState } from "react";
import { ScanLine, TriangleAlert } from "lucide-react";
import { cn } from "../lib/cn";
import type { ApiError } from "../lib/api";
import type { Job } from "../lib/types";
import { isTerminal } from "../hooks/studio";

interface PreprocessCardProps {
  job: Job | null;
  busy: boolean;
  error: ApiError | null;
  enabled: boolean;
}

type PreviewTab = "modelInput" | "mask" | "source";

export function PreprocessCard({ job, busy, error, enabled }: PreprocessCardProps) {
  const [tab, setTab] = useState<PreviewTab>("modelInput");
  const preview = job?.preview ?? null;

  useEffect(() => {
    if (!preview?.modelInput) setTab("source");
  }, [preview?.modelInput]);

  if (!enabled) {
    return (
      <section className="panel rounded-3xl p-4">
        <p className="label">Background removal</p>
        <p className="mt-1 text-sm text-ink-100">Preview unavailable</p>
        <p className="mt-1 text-[11px] leading-relaxed text-ink-500">
          This worker does not expose ComfyUI's background-removal nodes, so Idol will run on the raw image.
        </p>
      </section>
    );
  }

  const failed = job?.status === "failed";
  const source = job?.input?.imageUrl;

  return (
    <section className="panel rounded-3xl p-4">
      <div className="flex items-start justify-between gap-2">
        <div>
          <p className="label flex items-center gap-1.5">
            <ScanLine size={11} /> Idol input
          </p>
          <h2 className="mt-1 text-sm font-medium text-ink-100">Background & framing</h2>
        </div>
        <div className="flex gap-1 rounded-lg border border-white/8 bg-black/30 p-0.5">
          {(
            [
              { id: "source", label: "Source" },
              { id: "mask", label: "Mask" },
              { id: "modelInput", label: "Model" },
            ] as Array<{ id: PreviewTab; label: string }>
          ).map((option) => (
            <button
              key={option.id}
              type="button"
              onClick={() => setTab(option.id)}
              className={cn(
                "rounded-md px-2 py-1 text-[10px] font-medium uppercase tracking-[0.1em] transition",
                tab === option.id ? "bg-white/10 text-white" : "text-ink-500 hover:text-ink-200",
              )}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="relative mt-3 overflow-hidden rounded-2xl border border-white/10 bg-black">
        <div className="aspect-[4/3] w-full">
          {tab === "source" && source && (
            <img src={source} alt="Uploaded source with the detected mask" className="size-full object-contain" />
          )}
          {tab === "mask" && preview?.mask && (
            <img src={preview.mask.url} alt="Foreground mask" className="size-full object-contain" />
          )}
          {tab === "modelInput" && preview?.modelInput && (
            <img
              src={preview.modelInput.url}
              alt="Preprocessed image Idol receives"
              className="size-full object-contain"
            />
          )}
          {(busy || !preview) && !failed && (
            <div className="absolute inset-0 grid place-items-center">
              <div className="w-2/3 space-y-2">
                <div className="h-2 w-1/2 animate-pulse rounded-full bg-white/10" />
                <div className="h-24 animate-pulse rounded-xl bg-white/[0.06]" />
                <p className="text-center text-[10px] uppercase tracking-[0.16em] text-ink-500">
                  Removing the background on the GPU worker
                </p>
              </div>
            </div>
          )}
          {tab === "source" && source && preview?.mask && (
            <div className="pointer-events-none absolute inset-0 grid place-items-center">
              <p className="rounded-full bg-black/60 px-2 py-1 text-[10px] text-ink-300 backdrop-blur">
                see Mask tab for the detected foreground
              </p>
            </div>
          )}
        </div>
      </div>

      <p className="mt-2 text-[11px] leading-relaxed text-ink-500">
        {job?.status === "succeeded"
          ? `BiRefNet mask → ${job.params.mode === "real" ? "1.7×" : "1.2×"} crop → 1024² on black. This is exactly what Idol sees.`
          : "ComfyUI's BiRefNet removes the background, then Idol crops the object square on black."}
      </p>

      {error && (
        <p className="mt-2 flex items-start gap-1.5 rounded-xl border border-amber-400/25 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-amber-100">
          <TriangleAlert size={12} className="mt-0.5 shrink-0" />
          <span>{error.message}</span>
        </p>
      )}
      {failed && (
        <p className="mt-2 flex items-start gap-1.5 rounded-xl border border-amber-400/25 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-amber-100">
          <TriangleAlert size={12} className="mt-0.5 shrink-0" />
          <span>{job?.error?.message ?? "The preview failed."} You can still generate; Idol will remove the background again.</span>
        </p>
      )}
      {job && !isTerminal(job) && (
        <p className="mt-2 text-[10px] uppercase tracking-[0.14em] text-cyan-300/70">Preview running…</p>
      )}
    </section>
  );
}
