import type { ProviderProfile } from '../types';
import { assertEndpoint, iterateSse, mapFetchError, readError, withTimeout } from './http';
import { ProviderError, type ChatRequest, type ProviderAdapter, type StreamHandlers } from './types';

function endpoint(profile: ProviderProfile, model: string, action: 'generateContent' | 'streamGenerateContent' | 'embedContent'): string {
  if (!profile.apiKey) throw new ProviderError('missing-key', 'No API key is configured for this Google AI Studio provider.');
  const base = (profile.baseUrl || 'https://generativelanguage.googleapis.com/v1beta').replace(/\/+$/, '');
  const query = action === 'streamGenerateContent' ? `?alt=sse&key=${encodeURIComponent(profile.apiKey)}` : `?key=${encodeURIComponent(profile.apiKey)}`;
  return `${base}/models/${encodeURIComponent(model)}:${action}${query}`;
}

function imageParts(request: ChatRequest, vision: boolean): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = [];
  if (vision && request.images?.length) {
    for (const image of request.images) {
      const match = /^data:(image\/[a-z+]+);base64,(.*)$/i.exec(image);
      if (match) out.push({ inlineData: { mimeType: match[1], data: match[2] } });
    }
  }
  return out;
}

interface GeminiResponse {
  candidates?: { content?: { parts?: { text?: string }[] } }[];
  usageMetadata?: { promptTokenCount?: number; candidatesTokenCount?: number };
}

function extractText(json: GeminiResponse): string {
  return (json.candidates?.[0]?.content?.parts ?? []).map((p) => p.text ?? '').join('');
}

export const geminiAdapter: ProviderAdapter = {
  id: 'gemini',
  label: 'Google Gemini API',
  description: 'generateContent REST endpoint. Vision and embeddings supported by compatible models.',
  defaultBaseUrl: 'https://generativelanguage.googleapis.com/v1beta',
  capabilities: { text: true, vision: true, embeddings: true, tts: false, transcription: false, streaming: true },

  async chat(profile, request) {
    const url = endpoint(profile, request.model, 'generateContent');
    assertEndpoint(url);
    const system = request.messages.filter((m) => m.role === 'system').map((m) => m.content).join('\n\n');
    const contents = request.messages
      .filter((m) => m.role !== 'system')
      .map((m) => ({
        role: m.role === 'assistant' ? 'model' : 'user',
        parts: [{ text: m.content }, ...imageParts(request, profile.capabilities.vision)],
      }));
    const body: Record<string, unknown> = { contents };
    if (system) body.systemInstruction = { parts: [{ text: system }] };
    if (request.temperature !== undefined || request.maxTokens !== undefined) {
      body.generationConfig = {
        ...(request.temperature !== undefined ? { temperature: request.temperature } : {}),
        ...(request.maxTokens !== undefined ? { maxOutputTokens: request.maxTokens } : {}),
      };
    }
    const res = await postJsonRaw(url, body, {}, request);
    const json = (await res.json()) as GeminiResponse;
    const text = extractText(json);
    if (!text.trim()) throw new ProviderError('bad-response', 'Gemini returned no text candidates (possibly blocked by a safety filter).');
    return {
      text,
      model: request.model,
      usage: { promptTokens: json.usageMetadata?.promptTokenCount, completionTokens: json.usageMetadata?.candidatesTokenCount },
    };
  },

  async streamChat(profile, request, handlers: StreamHandlers) {
    const url = endpoint(profile, request.model, 'streamGenerateContent');
    assertEndpoint(url);
    const contents = request.messages
      .filter((m) => m.role !== 'system')
      .map((m) => ({ role: m.role === 'assistant' ? 'model' : 'user', parts: [{ text: m.content }] }));
    const system = request.messages.filter((m) => m.role === 'system').map((m) => m.content).join('\n\n');
    const body: Record<string, unknown> = { contents };
    if (system) body.systemInstruction = { parts: [{ text: system }] };
    const timeout = withTimeout(request.signal, request.timeoutMs ?? 120000);
    let res: Response;
    try {
      res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
        body: JSON.stringify(body),
        signal: timeout.signal,
      });
      if (!res.ok) throw await readError(res);
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    }
    let full = '';
    try {
      for await (const data of iterateSse(res)) {
        try {
          const json = JSON.parse(data) as GeminiResponse;
          const delta = extractText(json);
          if (delta) {
            full += delta;
            handlers.onDelta(delta);
          }
        } catch {
          /* ignore */
        }
      }
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    } finally {
      timeout.cancel();
    }
    if (!full.trim()) throw new ProviderError('bad-response', 'Gemini stream produced no text.');
    return { text: full, model: request.model };
  },

  async embed(profile, model, texts, signal) {
    const results: number[][] = [];
    for (const text of texts) {
      const url = endpoint(profile, model, 'embedContent');
      assertEndpoint(url);
      const res = await postJsonRaw(url, { content: { parts: [{ text }] } }, {}, { ...(signal ? { signal } : {}) });
      const json = (await res.json()) as { embedding?: { values?: number[] } };
      if (!json.embedding?.values) throw new ProviderError('bad-response', 'Gemini returned no embedding values.');
      results.push(json.embedding.values);
    }
    return results;
  },
};

async function postJsonRaw(
  url: string,
  body: unknown,
  extraHeaders: Record<string, string>,
  request: Pick<ChatRequest, 'signal' | 'timeoutMs'>,
): Promise<Response> {
  const timeout = withTimeout(request.signal, request.timeoutMs ?? 90000);
  try {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...extraHeaders },
      body: JSON.stringify(body),
      signal: timeout.signal,
    });
    if (!res.ok) throw await readError(res);
    return res;
  } catch (err) {
    return mapFetchError(err, timeout.timedOut(), request.signal);
  } finally {
    timeout.cancel();
  }
}
