import { redactSecrets } from '../security';
import { ProviderError } from './types';

export function assertEndpoint(url: string): URL {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new ProviderError('invalid-endpoint', `“${url}” is not a valid URL.`);
  }
  const isLoopback = ['localhost', '127.0.0.1', '::1'].includes(parsed.hostname);
  if (parsed.protocol !== 'https:' && !(parsed.protocol === 'http:' && isLoopback)) {
    throw new ProviderError(
      'invalid-endpoint',
      'Remote endpoints must use HTTPS. Plain HTTP is only accepted for localhost/127.0.0.1 development servers.',
    );
  }
  return parsed;
}

export function joinUrl(base: string, path: string): string {
  return `${base.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`;
}

export function withTimeout(signal: AbortSignal | undefined, timeoutMs: number): { signal: AbortSignal; cancel: () => void; timedOut: () => boolean } {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort(new DOMException('Request timed out', 'TimeoutError'));
  }, timeoutMs);
  const onAbort = () => controller.abort(signal?.reason);
  if (signal) {
    if (signal.aborted) controller.abort(signal.reason);
    else signal.addEventListener('abort', onAbort, { once: true });
  }
  return {
    signal: controller.signal,
    cancel: () => {
      clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
    },
    timedOut: () => timedOut,
  };
}

export async function mapFetchError(err: unknown, timedOut: boolean, signal?: AbortSignal): Promise<never> {
  if (timedOut) throw new ProviderError('timeout', 'The provider did not respond in time. Check your network or raise the timeout.');
  if (signal?.aborted) throw new ProviderError('cancelled', 'Request cancelled.');
  if (err instanceof ProviderError) throw err;
  const message = err instanceof Error ? err.message : String(err);
  if (/Failed to fetch|NetworkError|Load failed|CORS/i.test(message)) {
    throw new ProviderError(
      'cors',
      'Could not reach the provider. This is usually a network failure or a CORS block: the endpoint must allow browser requests from this origin. Local proxies need their own server and are not bundled.',
    );
  }
  throw new ProviderError('network', redactSecrets(message));
}

export async function readError(res: Response): Promise<ProviderError> {
  let detail = '';
  try {
    const text = await res.text();
    try {
      const json = JSON.parse(text) as { error?: { message?: string } | string; message?: string };
      detail =
        (typeof json.error === 'string' ? json.error : json.error?.message) ?? json.message ?? text.slice(0, 400);
    } catch {
      detail = text.slice(0, 400);
    }
  } catch {
    detail = '';
  }
  const safe = redactSecrets(detail).trim();
  if (res.status === 401 || res.status === 403) {
    return new ProviderError('invalid-key', `The provider rejected the credentials (HTTP ${res.status}). ${safe}`.trim(), res.status);
  }
  if (res.status === 429) {
    const retry = Number(res.headers.get('retry-after') ?? '') * 1000;
    return new ProviderError('rate-limit', `Rate limited by the provider (HTTP 429). ${safe}`.trim(), 429, Number.isFinite(retry) ? retry : undefined);
  }
  if (res.status === 404) {
    return new ProviderError('bad-request', `Endpoint or model not found (HTTP 404). Check the base URL and model name. ${safe}`.trim(), 404);
  }
  if (res.status >= 400 && res.status < 500) {
    return new ProviderError('bad-request', `The provider rejected the request (HTTP ${res.status}). ${safe}`.trim(), res.status);
  }
  return new ProviderError('bad-response', `Provider error (HTTP ${res.status}). ${safe}`.trim(), res.status);
}

export async function postJson(url: string, body: unknown, headers: Record<string, string>, signal?: AbortSignal, timeoutMs = 60000): Promise<Response> {
  assertEndpoint(url);
  const timeout = withTimeout(signal, timeoutMs);
  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...headers },
      body: JSON.stringify(body),
      signal: timeout.signal,
    });
    if (!res.ok) throw await readError(res);
    return res;
  } catch (err) {
    return mapFetchError(err, timeout.timedOut(), signal);
  } finally {
    timeout.cancel();
  }
}

/** Parse a Server-Sent Events response into `data:` payload strings. */
export async function* iterateSse(res: Response): AsyncGenerator<string> {
  if (!res.body) throw new ProviderError('bad-response', 'Streaming response had no body.');
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const parts = buffer.split(/\r?\n\r?\n/);
      buffer = parts.pop() ?? '';
      for (const part of parts) {
        for (const line of part.split(/\r?\n/)) {
          if (line.startsWith('data:')) yield line.slice(5).trim();
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

export async function* iterateNdjson(res: Response): AsyncGenerator<unknown> {
  if (!res.body) throw new ProviderError('bad-response', 'Streaming response had no body.');
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) {
        if (!line.trim()) continue;
        try {
          yield JSON.parse(line);
        } catch {
          // ignore keep-alive noise
        }
      }
    }
    if (buffer.trim()) {
      try {
        yield JSON.parse(buffer);
      } catch {
        /* ignore */
      }
    }
  } finally {
    reader.releaseLock();
  }
}
