import { useState } from "react";
import { Boxes, ChevronDown, Dices, Info, Sparkles, TriangleAlert } from "lucide-react";
import { cn } from "../lib/cn";
import { estimateSeconds } from "../lib/format";
import type { AppConfig, GenerationMode, Precision } from "../lib/types";
import { Segmented, Slider, Toggle } from "./primitives";

export interface GenerationSettings {
  mode: GenerationMode;
  gaussians: number;
  steps: number;
  cfg: number;
  seed: number | null;
  precision: Precision;
  exportGlb: boolean;
  meshResolution: number;
}

interface SettingsCardProps {
  config: AppConfig | null;
  settings: GenerationSettings;
  onChange: (patch: Partial<GenerationSettings>) => void;
  onGenerate: () => void;
  canGenerate: boolean;
  simulated: boolean;
  generating: boolean;
  blockedReason: string | null;
}

export function SettingsCard({
  config,
  settings,
  onChange,
  onGenerate,
  canGenerate,
  simulated,
  generating,
  blockedReason,
}: SettingsCardProps) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const presets = config?.presets ?? [];
  const modes = config?.modes ?? [];
  const activePreset = presets.find(
    (preset) => preset.gaussians === settings.gaussians && preset.steps === settings.steps,
  );
  const glbAvailable = config?.features.glb ?? false;

  return (
    <section className="panel rounded-3xl p-4">
      <div className="flex items-center justify-between">
        <div>
          <p className="label">Inference settings</p>
          <h2 className="mt-1 text-sm font-medium text-ink-100">Idol · TripoSplat architecture</h2>
        </div>
        <span className="chip border-cyan-300/20 text-cyan-200/90">ComfyUI</span>
      </div>

      <div className="mt-4 space-y-4">
        <div>
          <span className="label">Image type</span>
          <Segmented
            className="mt-2"
            options={modes.map((mode) => ({ value: mode.id, label: mode.label }))}
            value={settings.mode}
            onChange={(mode) => onChange({ mode })}
          />
          <p className="mt-1.5 text-[11px] leading-relaxed text-ink-500">
            {modes.find((mode) => mode.id === settings.mode)?.description}
          </p>
        </div>

        <div>
          <span className="label">Quality</span>
          <div className="mt-2 grid grid-cols-2 gap-1.5">
            {presets.map((preset) => {
              const active = activePreset?.id === preset.id;
              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => onChange({ gaussians: preset.gaussians, steps: preset.steps })}
                  className={cn(
                    "group relative overflow-hidden rounded-xl border px-3 py-2 text-left transition",
                    active
                      ? "border-cyan-300/45 bg-gradient-to-br from-cyan-400/15 to-violet-500/10"
                      : "border-white/8 bg-white/[0.02] hover:border-white/20",
                  )}
                >
                  <span className={cn("block text-xs font-semibold", active ? "text-white" : "text-ink-200")}>
                    {preset.label}
                  </span>
                  <span className="mt-0.5 block font-mono text-[10px] text-ink-500">
                    {(preset.gaussians / 1000).toFixed(0)}k splats
                  </span>
                  {active && <span className="absolute inset-x-0 bottom-0 h-px accent-line" />}
                </button>
              );
            })}
          </div>
          <p className="mt-1.5 text-[11px] text-ink-500">
            {activePreset?.hint ?? "Custom"} · {estimateSeconds(settings.gaussians, settings.steps)}
          </p>
        </div>

        <button
          type="button"
          onClick={() => setAdvancedOpen((open) => !open)}
          className="flex w-full items-center justify-between rounded-xl border border-white/8 bg-white/[0.02] px-3 py-2 text-[11px] font-medium text-ink-300 transition hover:border-white/15 hover:text-white"
          aria-expanded={advancedOpen}
        >
          <span className="flex items-center gap-1.5">
            <Sparkles size={12} className="text-violet-300" /> Advanced
          </span>
          <ChevronDown size={14} className={cn("transition", advancedOpen && "rotate-180")} />
        </button>

        {advancedOpen && (
          <div className="animate-fade space-y-4 rounded-xl border border-white/8 bg-black/20 p-3">
            <Slider
              label="Sampling steps"
              value={settings.steps}
              min={config?.steps.min ?? 10}
              max={config?.steps.max ?? 80}
              step={1}
              onChange={(steps) => onChange({ steps })}
              hint="Idol's workflow uses 50 steps (euler, simple)."
            />
            <Slider
              label="CFG"
              value={settings.cfg}
              min={1}
              max={8}
              step={0.5}
              onChange={(cfg) => onChange({ cfg })}
              hint="4 is Idol's guidance of 3 in ComfyUI's convention."
            />

            <div>
              <span className="label">Seed</span>
              <div className="mt-2 flex items-center gap-2">
                <input
                  type="number"
                  min={0}
                  max={2 ** 31 - 1}
                  value={settings.seed ?? ""}
                  placeholder="Random"
                  onChange={(event) =>
                    onChange({ seed: event.target.value === "" ? null : Number(event.target.value) })
                  }
                  className="min-w-0 flex-1 rounded-lg border border-white/10 bg-black/40 px-2.5 py-1.5 font-mono text-xs text-ink-100 outline-none transition focus:border-cyan-300/50"
                />
                <button
                  type="button"
                  onClick={() => onChange({ seed: Math.floor(Math.random() * 2 ** 31) })}
                  className="btn-ghost flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-[11px]"
                  title="Use a random seed"
                >
                  <Dices size={12} /> Random
                </button>
                {settings.seed !== null && (
                  <button
                    type="button"
                    onClick={() => onChange({ seed: null })}
                    className="text-[11px] text-ink-500 underline-offset-2 hover:text-ink-200 hover:underline"
                  >
                    reset
                  </button>
                )}
              </div>
            </div>

            <div>
              <span className="label">Denoiser precision</span>
              <select
                value={settings.precision}
                onChange={(event) => onChange({ precision: event.target.value as Precision })}
                className="mt-2 w-full appearance-none rounded-lg border border-white/10 bg-black/40 px-2.5 py-2 text-xs text-ink-100 outline-none transition focus:border-cyan-300/50"
              >
                {(config?.precisions ?? ["default"]).map((precision) => (
                  <option key={precision} value={precision} className="bg-obsidian-900">
                    {precision}
                  </option>
                ))}
              </select>
              <p className="mt-1 text-[11px] leading-relaxed text-ink-500">
                fp32 presets need idol_fp32.safetensors and are slower. Kept in the graph at default too.
              </p>
            </div>

            <Toggle
              checked={settings.exportGlb && glbAvailable}
              disabled={!glbAvailable}
              onChange={(exportGlb) => onChange({ exportGlb })}
              label="Also export GLB (mesh)"
              description={
                glbAvailable
                  ? "ComfyUI extracts a surface mesh from the splat (SplatToMesh → GLB). Adds a few seconds."
                  : "This worker's ComfyUI build has no SplatToMesh/MeshToFile3D nodes."
              }
            />
            {settings.exportGlb && glbAvailable && (
              <div>
                <span className="label">Mesh resolution</span>
                <select
                  value={settings.meshResolution}
                  onChange={(event) => onChange({ meshResolution: Number(event.target.value) })}
                  className="mt-2 w-full rounded-lg border border-white/10 bg-black/40 px-2.5 py-2 text-xs text-ink-100 outline-none focus:border-cyan-300/50"
                >
                  {[128, 256, 384, 512].map((resolution) => (
                    <option key={resolution} value={resolution} className="bg-obsidian-900">
                      {resolution}³ voxels{resolution === 384 ? " (ComfyUI default)" : ""}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="mt-4">
        <button
          type="button"
          onClick={onGenerate}
          disabled={!canGenerate || generating}
          className="btn-primary flex w-full items-center justify-center gap-2 rounded-xl px-4 py-3 text-sm font-semibold text-white"
        >
          <Boxes size={16} />
          {generating ? "Starting…" : "Generate 3D splat"}
        </button>
        {simulated && (
          <p className="mt-2 flex items-start gap-1.5 rounded-lg border border-amber-300/25 bg-amber-500/10 px-2.5 py-2 text-[11px] leading-relaxed text-amber-200/90">
            <TriangleAlert size={12} className="mt-0.5 shrink-0" />
            <span>
              Development simulator connected. This runs a procedural stand-in on the worker, <strong>not Idol
              inference</strong>; every job is labelled <span className="font-mono">simulated</span>.
            </span>
          </p>
        )}
        <p className="mt-2 flex items-start gap-1.5 text-[11px] leading-relaxed text-ink-500">
          {blockedReason ? (
            <>
              <TriangleAlert size={12} className="mt-0.5 shrink-0 text-amber-300" />
              <span>{blockedReason}</span>
            </>
          ) : (
            <>
              <Info size={12} className="mt-0.5 shrink-0" />
              <span>
                Runs real Idol inference on the GPU worker. You get a downloadable 3DGS <span className="font-mono">.ply</span>
                {glbAvailable ? " plus an optional GLB" : ""} — never a fake preview.
              </span>
            </>
          )}
        </p>
      </div>
    </section>
  );
}
