import { useCallback, useEffect, useRef, useState } from "react";
import { ImagePlus, Loader2, Sparkles, Trash2, UploadCloud } from "lucide-react";
import { cn } from "../lib/cn";
import { formatBytes } from "../lib/format";
import { ApiError } from "../lib/api";
import type { UploadImage } from "../lib/types";

interface UploadCardProps {
  upload: UploadImage | null;
  uploading: boolean;
  error: ApiError | null;
  demoImage?: string | null;
  maxUploadMb: number;
  onFile: (file: File) => void;
  onSample: (url: string) => void;
  onClear: () => void;
}

export function UploadCard({
  upload,
  uploading,
  error,
  demoImage,
  maxUploadMb,
  onFile,
  onSample,
  onClear,
}: UploadCardProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const dragDepth = useRef(0);

  const pick = useCallback(() => inputRef.current?.click(), []);

  const handleFiles = useCallback(
    (files: FileList | null) => {
      const file = files?.[0];
      if (file) onFile(file);
    },
    [onFile],
  );

  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      if (upload || uploading) return;
      const item = Array.from(event.clipboardData?.items ?? []).find((entry) => entry.type.startsWith("image/"));
      const file = item?.getAsFile();
      if (file) onFile(file);
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [onFile, upload, uploading]);

  return (
    <section className="panel relative overflow-hidden rounded-3xl p-4">
      <div className="mb-3 flex items-center justify-between">
        <div>
          <p className="label">Source image</p>
          <h2 className="mt-1 text-sm font-medium text-ink-100">One object, one photo</h2>
        </div>
        {upload && (
          <button
            type="button"
            onClick={onClear}
            className="btn-ghost flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-[11px] text-ink-300"
          >
            <Trash2 size={12} /> Remove
          </button>
        )}
      </div>

      <input
        ref={inputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/bmp"
        className="hidden"
        onChange={(event) => {
          handleFiles(event.target.files);
          event.target.value = "";
        }}
      />

      {upload ? (
        <div className="group relative overflow-hidden rounded-2xl border border-white/10 bg-black/40">
          <img
            src={upload.thumbUrl || upload.imageUrl}
            alt={upload.originalName}
            className="aspect-square w-full object-contain"
            loading="eager"
          />
          <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-end justify-between gap-2 bg-gradient-to-t from-black/85 via-black/25 to-transparent p-3">
            <div className="min-w-0">
              <p className="truncate text-[11px] font-medium text-white/90">{upload.originalName}</p>
              <p className="mt-0.5 font-mono text-[10px] text-ink-400">
                {upload.width}×{upload.height} · {formatBytes(upload.sizeBytes)}
              </p>
            </div>
            <button
              type="button"
              onClick={pick}
              className="pointer-events-auto flex items-center gap-1.5 rounded-lg border border-white/15 bg-black/50 px-2.5 py-1.5 text-[11px] text-white backdrop-blur transition hover:bg-black/70"
            >
              <UploadCloud size={12} /> Replace
            </button>
          </div>
        </div>
      ) : (
        <div
          role="button"
          tabIndex={0}
          aria-label="Upload an image"
          onClick={pick}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              pick();
            }
          }}
          onDragEnter={(event) => {
            event.preventDefault();
            dragDepth.current += 1;
            setDragging(true);
          }}
          onDragOver={(event) => event.preventDefault()}
          onDragLeave={(event) => {
            event.preventDefault();
            dragDepth.current -= 1;
            if (dragDepth.current <= 0) setDragging(false);
          }}
          onDrop={(event) => {
            event.preventDefault();
            dragDepth.current = 0;
            setDragging(false);
            handleFiles(event.dataTransfer.files);
          }}
          className={cn(
            "relative grid cursor-pointer place-items-center overflow-hidden rounded-2xl border border-dashed px-6 py-10 text-center transition",
            dragging
              ? "border-cyan-300/70 bg-cyan-400/[0.07]"
              : "border-white/15 bg-white/[0.015] hover:border-cyan-300/40 hover:bg-white/[0.03]",
          )}
        >
          <div className="pointer-events-none absolute inset-0 opacity-70">
            <div className="absolute left-1/2 top-0 h-32 w-56 -translate-x-1/2 rounded-full bg-cyan-400/10 blur-3xl" />
          </div>
          <div className="relative">
            <div className="mx-auto grid size-12 place-items-center rounded-2xl border border-white/10 bg-white/[0.04]">
              {uploading ? (
                <Loader2 size={20} className="animate-spin text-cyan-300" />
              ) : (
                <ImagePlus size={20} className="text-cyan-300" />
              )}
            </div>
            <p className="mt-3 text-sm font-medium text-white">
              {uploading ? "Processing image…" : dragging ? "Drop it here" : "Drag & drop an image"}
            </p>
            <p className="mt-1 text-[11px] text-ink-400">
              PNG, JPEG, WebP or BMP · up to {maxUploadMb} MB · or paste from the clipboard
            </p>
            <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
              <span className="chip">Browse files</span>
              {demoImage && (
                <button
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    onSample(demoImage);
                  }}
                  className="chip border-violet-300/25 bg-violet-400/10 text-violet-200 transition hover:border-violet-300/50"
                >
                  <Sparkles size={11} /> Try a sample
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {error && (
        <p className="mt-3 rounded-xl border border-rose-400/25 bg-rose-500/10 px-3 py-2 text-[11px] leading-relaxed text-rose-200">
          {error.message}
        </p>
      )}
    </section>
  );
}
