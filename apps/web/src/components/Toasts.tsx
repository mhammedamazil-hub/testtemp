import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { cn } from "../lib/cn";

export type ToastKind = "error" | "info" | "success";

export interface ToastInput {
  title: string;
  message?: string;
  hint?: string;
  kind?: ToastKind;
}

interface ToastItem extends ToastInput {
  id: number;
}

interface ToastApi {
  push: (toast: ToastInput) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const counter = useRef(0);

  const dismiss = useCallback((id: number) => {
    setItems((current) => current.filter((item) => item.id !== id));
  }, []);

  const push = useCallback(
    (toast: ToastInput) => {
      const id = ++counter.current;
      setItems((current) => [...current.slice(-3), { ...toast, id }]);
      window.setTimeout(() => dismiss(id), toast.kind === "error" ? 9000 : 5000);
    },
    [dismiss],
  );

  const api = useMemo(() => ({ push }), [push]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 bottom-20 z-[80] flex flex-col items-center gap-2 px-4 sm:bottom-6 sm:items-end sm:px-6">
        {items.map((item) => (
          <div
            key={item.id}
            role="status"
            className={cn(
              "pointer-events-auto flex w-full max-w-sm animate-rise items-start gap-3 rounded-2xl border p-4 shadow-2xl backdrop-blur-xl",
              item.kind === "error"
                ? "border-rose-400/25 bg-rose-950/70"
                : item.kind === "success"
                  ? "border-emerald-400/25 bg-emerald-950/60"
                  : "border-white/10 bg-obsidian-800/90",
            )}
          >
            <span
              className={cn(
                "mt-0.5 shrink-0",
                item.kind === "error" ? "text-rose-300" : item.kind === "success" ? "text-emerald-300" : "text-cyan-300",
              )}
            >
              {item.kind === "error" ? (
                <AlertTriangle size={18} />
              ) : item.kind === "success" ? (
                <CheckCircle2 size={18} />
              ) : (
                <Info size={18} />
              )}
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-white">{item.title}</p>
              {item.message && <p className="mt-0.5 text-xs leading-relaxed text-ink-300">{item.message}</p>}
              {item.hint && <p className="mt-1 text-xs leading-relaxed text-ink-400">{item.hint}</p>}
            </div>
            <button
              type="button"
              onClick={() => dismiss(item.id)}
              className="rounded-lg p-1 text-ink-400 transition hover:bg-white/5 hover:text-white"
              aria-label="Dismiss notification"
            >
              <X size={14} />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const context = useContext(ToastContext);
  if (!context) throw new Error("useToast must be used inside <ToastProvider>");
  return context;
}
