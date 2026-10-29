import type { ProviderProfile } from '../types';
import { iterateSse, joinUrl, postJson, readError, withTimeout, mapFetchError, assertEndpoint } from './http';
import { ProviderError, type ChatRequest, type ChatResult, type ProviderAdapter, type StreamHandlers, type TranscriptionResult } from './types';

interface OpenAiChunk {
  choices?: { delta?: { content?: string }; message?: { content?: string } }[];
  model?: string;
  usage?: { prompt_tokens?: number; completion_tokens?: number };
}

function headers(profile: ProviderProfile): Record<string, string> {
  if (!profile.apiKey) throw new ProviderError('missing-key', 'No API key is configured for this provider. Add one in Settings (it stays in memory unless you opt in to saving it).');
  const h: Record<string, string> = { Authorization: `Bearer ${profile.apiKey}`, ...(profile.headers ?? {}) };
  if (profile.headers?.Authorization) h.Authorization = profile.headers.Authorization;
  return h;
}

function buildMessages(request: ChatRequest, supportsVision: boolean) {
  return request.messages.map((message) => {
    const useImages = supportsVision && message.role === 'user' && request.images?.length;
    if (!useImages) return { role: message.role, content: message.content };
    return {
      role: message.role,
      content: [
        { type: 'text', text: message.content },
        ...(request.images ?? []).map((url) => ({ type: 'image_url', image_url: { url } })),
      ],
    };
  });
}

export const openAiAdapter: ProviderAdapter = {
  id: 'openai',
  label: 'OpenAI-compatible chat completions',
  description: 'POST {baseUrl}/chat/completions with Bearer auth. Works with OpenAI, OpenRouter, Groq, Mistral, DeepSeek, LM Studio, Ollama and others.',
  defaultBaseUrl: 'https://api.openai.com/v1',
  capabilities: { text: true, vision: true, embeddings: true, tts: true, transcription: true, streaming: true },

  async chat(profile, request) {
    const url = joinUrl(profile.baseUrl, 'chat/completions');
    const body: Record<string, unknown> = {
      model: request.model,
      messages: buildMessages(request, profile.capabilities.vision),
      stream: false,
    };
    if (request.temperature !== undefined) body.temperature = request.temperature;
    if (request.maxTokens !== undefined) body.max_tokens = request.maxTokens;
    const res = await postJson(url, body, headers(profile), request.signal, request.timeoutMs ?? 90000);
    const json = (await res.json()) as OpenAiChunk;
    const text = json.choices?.[0]?.message?.content ?? '';
    if (!text) throw new ProviderError('bad-response', 'The provider returned an empty completion.');
    return {
      text,
      model: json.model ?? request.model,
      usage: { promptTokens: json.usage?.prompt_tokens, completionTokens: json.usage?.completion_tokens },
      raw: json,
    };
  },

  async streamChat(profile, request, handlers: StreamHandlers) {
    const url = joinUrl(profile.baseUrl, 'chat/completions');
    const body: Record<string, unknown> = {
      model: request.model,
      messages: buildMessages(request, profile.capabilities.vision),
      stream: true,
    };
    if (request.temperature !== undefined) body.temperature = request.temperature;
    if (request.maxTokens !== undefined) body.max_tokens = request.maxTokens;
    assertEndpoint(url);
    const timeout = withTimeout(request.signal, request.timeoutMs ?? 120000);
    let res: Response;
    try {
      res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream', ...headers(profile) },
        body: JSON.stringify(body),
        signal: timeout.signal,
      });
      if (!res.ok) throw await readError(res);
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    }
    let full = '';
    let model = request.model;
    try {
      for await (const data of iterateSse(res)) {
        if (data === '[DONE]') break;
        let parsed: OpenAiChunk;
        try {
          parsed = JSON.parse(data) as OpenAiChunk;
        } catch {
          continue;
        }
        if (parsed.model) model = parsed.model;
        const delta = parsed.choices?.[0]?.delta?.content ?? '';
        if (delta) {
          full += delta;
          handlers.onDelta(delta);
        }
      }
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    } finally {
      timeout.cancel();
    }
    if (!full.trim()) throw new ProviderError('bad-response', 'The streamed response contained no text.');
    return { text: full, model } satisfies ChatResult;
  },

  async embed(profile, model, texts, signal) {
    const url = joinUrl(profile.baseUrl, 'embeddings');
    const res = await postJson(url, { model, input: texts }, headers(profile), signal, 60000);
    const json = (await res.json()) as { data?: { embedding: number[]; index: number }[] };
    if (!json.data?.length) throw new ProviderError('bad-response', 'The provider returned no embeddings.');
    return json.data.sort((a, b) => a.index - b.index).map((row) => row.embedding);
  },

  async transcribe(profile, model, audio, signal) {
    const url = joinUrl(profile.baseUrl, 'audio/transcriptions');
    assertEndpoint(url);
    const form = new FormData();
    form.append('file', audio, audio instanceof File ? audio.name : 'audio.webm');
    form.append('model', model);
    form.append('response_format', 'verbose_json');
    const timeout = withTimeout(signal, 180000);
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: headers(profile),
        body: form,
        signal: timeout.signal,
      });
      if (!res.ok) throw await readError(res);
      const json = (await res.json()) as { text?: string; segments?: { start: number; end: number; text: string }[]; language?: string };
      if (!json.text) throw new ProviderError('bad-response', 'The transcription response was empty.');
      const result: TranscriptionResult = { text: json.text };
      if (json.segments) result.segments = json.segments.map((s) => ({ start: s.start * 1000, end: s.end * 1000, text: s.text }));
      if (json.language) result.language = json.language;
      return result;
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), signal);
    } finally {
      timeout.cancel();
    }
  },

  async synthesize(profile, model, text, signal) {
    const url = joinUrl(profile.baseUrl, 'audio/speech');
    const res = await postJson(
      url,
      { model, input: text, voice: (profile.headers?.['x-voice'] as string) || 'alloy', response_format: 'mp3' },
      headers(profile),
      signal,
      180000,
    );
    const blob = await res.blob();
    if (!blob.size) throw new ProviderError('bad-response', 'Speech synthesis returned an empty file.');
    return blob;
  },

  async listModels(profile) {
    const url = joinUrl(profile.baseUrl, 'models');
    assertEndpoint(url);
    const res = await fetch(url, { headers: headers(profile) });
    if (!res.ok) throw await readError(res);
    const json = (await res.json()) as { data?: { id: string }[] };
    return (json.data ?? []).map((m) => m.id).sort();
  },
};
