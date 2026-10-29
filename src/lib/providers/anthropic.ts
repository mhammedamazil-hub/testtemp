import type { ProviderProfile } from '../types';
import { assertEndpoint, iterateSse, joinUrl, mapFetchError, postJson, readError, withTimeout } from './http';
import { ProviderError, type ProviderAdapter, type StreamHandlers } from './types';

function headers(profile: ProviderProfile): Record<string, string> {
  if (!profile.apiKey) throw new ProviderError('missing-key', 'No API key is configured for this provider.');
  return {
    'x-api-key': profile.apiKey,
    'anthropic-version': '2023-06-01',
    'anthropic-dangerous-direct-browser-access': 'true',
    ...(profile.headers ?? {}),
  };
}

interface AnthropicResponse {
  content?: { type: string; text?: string }[];
  model?: string;
  usage?: { input_tokens?: number; output_tokens?: number };
}

function imageBlocks(images: string[] | undefined) {
  return (images ?? [])
    .map((dataUrl) => {
      const match = /^data:(image\/[a-z+]+);base64,(.*)$/i.exec(dataUrl);
      if (!match) return null;
      return { type: 'image', source: { type: 'base64', media_type: match[1], data: match[2] } };
    })
    .filter(Boolean) as { type: string; source: { type: string; media_type: string; data: string } }[];
}

export const anthropicAdapter: ProviderAdapter = {
  id: 'anthropic',
  label: 'Anthropic Messages API',
  description: 'POST {baseUrl}/messages. Browser-accessible with the direct-browser-access header. No embeddings or speech endpoints.',
  defaultBaseUrl: 'https://api.anthropic.com/v1',
  capabilities: { text: true, vision: true, embeddings: false, tts: false, transcription: false, streaming: true },

  async chat(profile, request) {
    const url = joinUrl(profile.baseUrl, 'messages');
    const system = request.messages.filter((m) => m.role === 'system').map((m) => m.content).join('\n\n');
    const messages = request.messages
      .filter((m) => m.role !== 'system')
      .map((m) => {
        if (m.role === 'user' && profile.capabilities.vision && request.images?.length) {
          return {
            role: 'user',
            content: [{ type: 'text', text: m.content }, ...imageBlocks(request.images)],
          };
        }
        return { role: m.role, content: m.content };
      });
    const body: Record<string, unknown> = {
      model: request.model,
      max_tokens: request.maxTokens ?? 2048,
      messages,
    };
    if (system) body.system = system;
    if (request.temperature !== undefined) body.temperature = request.temperature;
    const res = await postJson(url, body, headers(profile), request.signal, request.timeoutMs ?? 90000);
    const json = (await res.json()) as AnthropicResponse;
    const text = (json.content ?? []).filter((b) => b.type === 'text').map((b) => b.text ?? '').join('');
    if (!text.trim()) throw new ProviderError('bad-response', 'Anthropic returned no text blocks.');
    return {
      text,
      model: json.model ?? request.model,
      usage: { promptTokens: json.usage?.input_tokens, completionTokens: json.usage?.output_tokens },
    };
  },

  async streamChat(profile, request, handlers: StreamHandlers) {
    const url = joinUrl(profile.baseUrl, 'messages');
    const system = request.messages.filter((m) => m.role === 'system').map((m) => m.content).join('\n\n');
    const messages = request.messages
      .filter((m) => m.role !== 'system')
      .map((m) => ({ role: m.role, content: m.content }));
    const body: Record<string, unknown> = {
      model: request.model,
      max_tokens: request.maxTokens ?? 2048,
      messages,
      stream: true,
    };
    if (system) body.system = system;
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
    try {
      for await (const data of iterateSse(res)) {
        try {
          const parsed = JSON.parse(data) as { type?: string; delta?: { type?: string; text?: string } };
          if (parsed.type === 'content_block_delta' && parsed.delta?.text) {
            full += parsed.delta.text;
            handlers.onDelta(parsed.delta.text);
          }
        } catch {
          /* ignore keepalive */
        }
      }
    } catch (err) {
      return mapFetchError(err, timeout.timedOut(), request.signal);
    } finally {
      timeout.cancel();
    }
    if (!full.trim()) throw new ProviderError('bad-response', 'The streamed response contained no text.');
    return { text: full, model: request.model };
  },
};
