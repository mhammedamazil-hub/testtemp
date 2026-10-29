import type { ProviderProfile } from '../types';
import { assertEndpoint, iterateNdjson, joinUrl, mapFetchError, readError, withTimeout } from './http';
import { ProviderError, type ProviderAdapter, type StreamHandlers } from './types';

function headers(profile: ProviderProfile): Record<string, string> {
  const h: Record<string, string> = { ...(profile.headers ?? {}) };
  if (profile.apiKey) h.Authorization = `Bearer ${profile.apiKey}`;
  return h;
}

interface OllamaChunk {
  message?: { content?: string };
  model?: string;
  done?: boolean;
  prompt_eval_count?: number;
  eval_count?: number;
}

export const ollamaAdapter: ProviderAdapter = {
  id: 'ollama',
  label: 'Ollama native API',
  description: 'POST {baseUrl}/api/chat (NDJSON stream). Fully local when Ollama runs in your browser-accessible network; images supported by vision models.',
  defaultBaseUrl: 'http://localhost:11434',
  capabilities: { text: true, vision: true, embeddings: true, tts: false, transcription: false, streaming: true },

  async chat(profile, request) {
    const url = joinUrl(profile.baseUrl, 'api/chat');
    assertEndpoint(url);
    const timeout = withTimeout(request.signal, request.timeoutMs ?? 120000);
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...headers(profile) },
        body: JSON.stringify({
          model: request.model,
          messages: request.messages,
          stream: false,
          ...(request.images?.length && profile.capabilities.vision ? { images: request.images.map(stripDataUrl) } : {}),
          options: {
            ...(request.temperature !== undefined ? { temperature: request.temperature } : {}),
            ...(request.maxTokens !== undefined ? { num_predict: request.maxTokens } : {}),
          },
        }),
        signal: timeout.signal,
      });
      if (!res.ok) throw await readError(res);
      const json = (await res.json()) as OllamaChunk;
      const text = json.message?.content ?? '';
      if (!text.trim()) throw new ProviderError('bad-response', 'Ollama returned an empty response. Is the model pulled?');
      return {
        text,
        model: json.model ?? request.model,
        usage: { promptTokens: json.prompt_eval_count, completionTokens: json.eval_count },
      };
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    } finally {
      timeout.cancel();
    }
  },

  async streamChat(profile, request, handlers: StreamHandlers) {
    const url = joinUrl(profile.baseUrl, 'api/chat');
    assertEndpoint(url);
    const timeout = withTimeout(request.signal, request.timeoutMs ?? 180000);
    let res: Response;
    try {
      res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...headers(profile) },
        body: JSON.stringify({ model: request.model, messages: request.messages, stream: true }),
        signal: timeout.signal,
      });
      if (!res.ok) throw await readError(res);
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    }
    let full = '';
    try {
      for await (const chunk of iterateNdjson(res)) {
        const json = chunk as OllamaChunk;
        const delta = json.message?.content ?? '';
        if (delta) {
          full += delta;
          handlers.onDelta(delta);
        }
        if (json.done) break;
      }
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    } finally {
      timeout.cancel();
    }
    if (!full.trim()) throw new ProviderError('bad-response', 'Ollama stream produced no text.');
    return { text: full, model: request.model };
  },

  async listModels(profile) {
    const url = joinUrl(profile.baseUrl, 'api/tags');
    assertEndpoint(url);
    const res = await fetch(url, { headers: headers(profile) });
    if (!res.ok) throw await readError(res);
    const json = (await res.json()) as { models?: { name: string }[] };
    return (json.models ?? []).map((m) => m.name).sort();
  },
};

function stripDataUrl(dataUrl: string): string {
  const idx = dataUrl.indexOf(',');
  return idx >= 0 ? dataUrl.slice(idx + 1) : dataUrl;
}
