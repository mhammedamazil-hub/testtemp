import type {
  AppConfig,
  GeneratePayload,
  Health,
  Job,
  UploadImage,
} from "./types";

export class ApiError extends Error {
  code: string;
  hint?: string;
  retryable: boolean;
  status: number;
  details?: unknown;

  constructor(
    message: string,
    opts: { code?: string; hint?: string; retryable?: boolean; status?: number; details?: unknown } = {},
  ) {
    super(message);
    this.name = "ApiError";
    this.code = opts.code ?? "request_failed";
    this.hint = opts.hint;
    this.retryable = opts.retryable ?? false;
    this.status = opts.status ?? 0;
    this.details = opts.details;
  }
}

function networkError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (error instanceof DOMException && error.name === "AbortError") {
    return new ApiError("The request was canceled.", { code: "aborted", retryable: true });
  }
  return new ApiError(
    "Could not reach the studio server. Check your connection and try again.",
    { code: "network_error", retryable: true },
  );
}

async function parseError(response: Response): Promise<ApiError> {
  let payload: unknown = null;
  try {
    payload = await response.json();
  } catch {
    /* non-JSON body */
  }
  const record = payload as { error?: Record<string, unknown>; detail?: unknown } | null;
  const error = record?.error;
  if (error && typeof error === "object") {
    const message =
      typeof error.message === "string" ? error.message : `Request failed (${response.status})`;
    return new ApiError(message, {
      code: typeof error.code === "string" ? error.code : "request_failed",
      hint: typeof error.hint === "string" ? error.hint : undefined,
      retryable: Boolean(error.retryable),
      status: response.status,
      details: error.details,
    });
  }
  if (typeof record?.detail === "string") {
    return new ApiError(record.detail, { status: response.status });
  }
  return new ApiError(`Request failed (${response.status}).`, { status: response.status, retryable: response.status >= 500 });
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, {
      ...init,
      headers: {
        Accept: "application/json",
        ...(init.body && !(init.body instanceof FormData) ? { "Content-Type": "application/json" } : {}),
        ...init.headers,
      },
    });
  } catch (error) {
    throw networkError(error);
  }
  if (!response.ok) throw await parseError(response);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  health: () => request<Health>("/api/health"),
  config: () => request<AppConfig>("/api/config"),

  async upload(file: File, signal?: AbortSignal): Promise<UploadImage> {
    const form = new FormData();
    form.append("file", file, file.name);
    return request<UploadImage>("/api/uploads", { method: "POST", body: form, signal });
  },

  preprocess(uploadId: string, mode: string) {
    return request<Job>("/api/preprocess", {
      method: "POST",
      body: JSON.stringify({ uploadId, mode }),
    });
  },

  generate(payload: GeneratePayload) {
    return request<Job>("/api/jobs", { method: "POST", body: JSON.stringify(payload) });
  },

  job(jobId: string) {
    return request<Job>(`/api/jobs/${jobId}`);
  },

  jobs(limit = 30) {
    return request<{ jobs: Job[] }>(`/api/jobs?limit=${limit}`);
  },

  cancel(jobId: string) {
    return request<Job>(`/api/jobs/${jobId}/cancel`, { method: "POST" });
  },

  retry(jobId: string) {
    return request<Job>(`/api/jobs/${jobId}/retry`, { method: "POST" });
  },

  remove(jobId: string) {
    return request<void>(`/api/jobs/${jobId}`, { method: "DELETE" });
  },
};

/** Subscribe to a job via SSE, with polling as a fallback if SSE fails. */
export function subscribeJob(
  jobId: string,
  onJob: (job: Job) => void,
  onError?: (error: ApiError) => void,
): () => void {
  let closed = false;
  let source: EventSource | null = null;
  let pollTimer: number | null = null;

  const startPolling = () => {
    if (closed || pollTimer !== null) return;
    const tick = async () => {
      if (closed) return;
      try {
        const job = await api.job(jobId);
        onJob(job);
        if (["succeeded", "failed", "canceled"].includes(job.status)) return;
      } catch (error) {
        onError?.(networkError(error));
        if (closed) return;
      }
      pollTimer = window.setTimeout(tick, 1200);
    };
    pollTimer = window.setTimeout(tick, 400);
  };

  // Only poll for jobs that are still moving; terminal states arrive over REST.
  const connect = () => {
    try {
      source = new EventSource(`/api/jobs/${jobId}/events`);
    } catch {
      startPolling();
      return;
    }
    source.onmessage = (event) => {
      try {
        const job = JSON.parse(event.data) as Job;
        onJob(job);
        if (["succeeded", "failed", "canceled"].includes(job.status)) close();
      } catch {
        onError?.(new ApiError("Received an unreadable progress update.", { code: "bad_stream" }));
      }
    };
    source.onerror = () => {
      if (closed) return;
      // EventSource retries on its own, but closed connections in proxies need a fallback.
      source?.close();
      source = null;
      startPolling();
    };
  };

  const close = () => {
    if (closed) return;
    closed = true;
    source?.close();
    source = null;
    if (pollTimer !== null) window.clearTimeout(pollTimer);
    pollTimer = null;
  };

  connect();
  return close;
}
