import { cn } from "../lib/cn";
import type { ReactNode } from "react";

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  disabled,
  className,
}: {
  options: Array<{ value: T; label: string; icon?: ReactNode }>;
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <div
      role="tablist"
      className={cn(
        "grid gap-1 rounded-xl border border-white/8 bg-black/25 p-1",
        options.length === 2 ? "grid-cols-2" : "grid-cols-1",
        className,
      )}
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={disabled}
            onClick={() => onChange(option.value)}
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-xs font-medium transition",
              active
                ? "bg-gradient-to-b from-white/12 to-white/5 text-white shadow-[0_1px_0_rgba(255,255,255,0.12)_inset]"
                : "text-ink-400 hover:text-ink-200",
              disabled && "cursor-not-allowed opacity-50",
            )}
          >
            {option.icon}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

export function Slider({
  label,
  value,
  min,
  max,
  step,
  suffix,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
  onChange: (value: number) => void;
  hint?: string;
}) {
  const progress = ((value - min) / (max - min)) * 100;
  return (
    <div>
      <div className="flex items-baseline justify-between">
        <span className="label">{label}</span>
        <span className="font-mono text-xs text-ink-200">
          {value}
          {suffix ? <span className="text-ink-500"> {suffix}</span> : null}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="mt-2 w-full cursor-pointer"
        style={{ ["--range-progress" as string]: `${progress}%` }}
        aria-label={label}
      />
      {hint && <p className="mt-1 text-[11px] leading-relaxed text-ink-500">{hint}</p>}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "flex w-full items-start gap-3 rounded-xl border border-white/8 bg-white/[0.02] p-3 text-left transition hover:border-white/15",
        disabled && "cursor-not-allowed opacity-50",
      )}
    >
      <span
        className={cn(
          "mt-0.5 relative h-5 w-9 shrink-0 rounded-full border transition",
          checked ? "border-cyan-300/50 bg-cyan-400/30" : "border-white/10 bg-white/5",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 size-3.5 rounded-full transition-all",
            checked ? "left-[1.15rem] bg-cyan-200" : "left-0.5 bg-ink-400",
          )}
        />
      </span>
      <span className="min-w-0">
        <span className="block text-xs font-medium text-ink-100">{label}</span>
        {description && <span className="mt-0.5 block text-[11px] leading-relaxed text-ink-500">{description}</span>}
      </span>
    </button>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <svg className={cn("size-4 animate-spin", className)} viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="2.5" opacity="0.2" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}

export function FieldShell({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("rounded-xl border border-white/8 bg-white/[0.02] p-3", className)}>{children}</div>;
}
