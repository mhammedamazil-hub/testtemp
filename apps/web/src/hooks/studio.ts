import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, subscribeJob } from "../lib/api";
import type { AppConfig, GeneratePayload, Health, Job, UploadImage } from "../lib/types";

const MAX_CLIENT_BYTES = 25 * 1024 * 1024;
const ACCEPTED = ["image/png", "image/jpeg", "image/webp", "image/bmp"];

export function isTerminal(job: Job | null | undefined): boolean {
  return Boolean(job && ["succeeded", "failed", "canceled"].includes(job.status));
}

export function useStudioMeta() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async (withConfig = false) => {
    try {
      const [healthResult, configResult] = await Promise.all([
        api.health(),
        withConfig ? api.config() : Promise.resolve(null),
      ]);
      setHealth(healthResult);
      if (configResult) setConfig(configResult);
      setError(null);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError("The studio server is unreachable."));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(true);
    const timer = window.setInterval(() => void load(false), 20_000);
    return () => window.clearInterval(timer);
  }, [load]);

  return { config, health, error, loading, refresh: () => load(true) };
}

export function useUploader() {
  const [upload, setUpload] = useState<UploadImage | null>(null);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const controller = useRef<AbortController | null>(null);

  const clear = useCallback(() => {
    controller.current?.abort();
    controller.current = null;
    setUpload(null);
    setError(null);
    setUploading(false);
  }, []);

  const send = useCallback(async (file: File) => {
    setError(null);
    if (!ACCEPTED.includes(file.type) && !/\.(png|jpe?g|webp|bmp)$/i.test(file.name)) {
      setError(
        new ApiError("That file type is not supported. Use a PNG, JPEG, WebP or BMP image.", {
          code: "unsupported_type",
        }),
      );
      return;
    }
    if (file.size > MAX_CLIENT_BYTES) {
      setError(
        new ApiError("That image is larger than 25 MB. Resize it and try again.", {
          code: "file_too_large",
        }),
      );
      return;
    }
    controller.current?.abort();
    const ac = new AbortController();
    controller.current = ac;
    setUploading(true);
    try {
      const result = await api.upload(file, ac.signal);
      if (!ac.signal.aborted) setUpload(result);
    } catch (err) {
      if (!ac.signal.aborted) {
        setError(err instanceof ApiError ? err : new ApiError("The upload failed. Try again."));
      }
    } finally {
      if (!ac.signal.aborted) setUploading(false);
    }
  }, []);

  // Allow uploading the bundled demo image in one click.
  const sendSample = useCallback(
    async (url: string) => {
      setError(null);
      setUploading(true);
      try {
        const response = await fetch(url);
        if (!response.ok) throw new Error("sample missing");
        const blob = await response.blob();
        const file = new File([blob], "idol-sample-object.png", { type: blob.type || "image/png" });
        await send(file);
      } catch {
        setError(
          new ApiError("The demo image could not be loaded. Upload your own image instead.", {
            code: "sample_unavailable",
          }),
        );
      } finally {
        setUploading(false);
      }
    },
    [send],
  );

  useEffect(() => () => controller.current?.abort(), []);

  return { upload, uploading, error, send, sendSample, clear, setError };
}

export function usePreprocess(upload: UploadImage | null, mode: string, enabled: boolean) {
  const [job, setJob] = useState<Job | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const uploadId = upload?.uploadId ?? null;

  useEffect(() => {
    if (!uploadId || !enabled) {
      setJob(null);
      setBusy(false);
      return;
    }
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    setBusy(true);
    setError(null);
    setJob(null);

    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const created = await api.preprocess(uploadId, mode);
          if (cancelled) return;
          setJob(created);
          unsubscribe = subscribeJob(
            created.id,
            (next) => {
              if (cancelled) return;
              setJob(next);
              if (isTerminal(next)) setBusy(false);
            },
            (streamErr) => {
              if (cancelled) return;
              setError(streamErr);
              setBusy(false);
            },
          );
        } catch (err) {
          if (cancelled) return;
          setError(err instanceof ApiError ? err : new ApiError("Background removal preview failed."));
          setBusy(false);
        }
      })();
    }, 350);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      unsubscribe?.();
    };
  }, [uploadId, mode, enabled]);

  return { job, busy, error };
}

export function useGeneration(onSettled?: () => void) {
  const [job, setJob] = useState<Job | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const settledRef = useRef<string | null>(null);

  // Follow the active job until it reaches a terminal state.
  const jobId = job?.id ?? null;
  const terminal = isTerminal(job);
  useEffect(() => {
    if (!jobId || terminal) return;
    const unsubscribe = subscribeJob(
      jobId,
      (next) => {
        setJob(next);
        setError(null);
      },
      (streamError) => setError(streamError),
    );
    return unsubscribe;
  }, [jobId, terminal]);

  useEffect(() => {
    if (!job || !isTerminal(job)) return;
    if (settledRef.current === job.id) return;
    settledRef.current = job.id;
    onSettled?.();
  }, [job, onSettled]);

  const start = useCallback(async (payload: GeneratePayload) => {
    setStarting(true);
    setError(null);
    try {
      const created = await api.generate(payload);
      settledRef.current = null;
      setJob(created);
      return created;
    } catch (err) {
      const apiError = err instanceof ApiError ? err : new ApiError("Could not start the generation.");
      setError(apiError);
      throw apiError;
    } finally {
      setStarting(false);
    }
  }, []);

  const cancel = useCallback(async () => {
    if (!job) return;
    try {
      const updated = await api.cancel(job.id);
      setJob(updated);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError("Could not cancel the job."));
    }
  }, [job]);

  const retry = useCallback(async () => {
    if (!job) return null;
    setStarting(true);
    try {
      const created = await api.retry(job.id);
      settledRef.current = null;
      setJob(created);
      return created;
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError("Could not retry the job."));
      return null;
    } finally {
      setStarting(false);
    }
  }, [job]);

  const open = useCallback((next: Job) => {
    settledRef.current = isTerminal(next) ? next.id : null;
    setJob(next);
    setError(null);
  }, []);

  const reset = useCallback(() => {
    settledRef.current = null;
    setJob(null);
    setError(null);
  }, []);

  return {
    job,
    starting,
    error,
    start,
    cancel,
    retry,
    open,
    reset,
    clearError: useCallback(() => setError(null), []),
  };
}

export function useHistory(limit = 24) {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const result = await api.jobs(limit);
      setJobs(result.jobs);
      setError(null);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError("Could not load the history."));
    } finally {
      setLoading(false);
    }
  }, [limit]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const remove = useCallback(
    async (jobId: string) => {
      try {
        await api.remove(jobId);
        setJobs((current) => current.filter((item) => item.id !== jobId));
      } catch (err) {
        setError(err instanceof ApiError ? err : new ApiError("Could not delete that job."));
        throw err;
      }
    },
    [],
  );

  return { jobs, loading, error, refresh, remove };
}
