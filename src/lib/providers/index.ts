/**
 * Provider registry + task routing.
 *
 * A profile = credentials + endpoint + model routing + the capabilities the
 * user asserts for the chosen model. Advertised capabilities are the
 * intersection of the protocol's maximum and the profile's assertion, so a
 * text-only model can never be used for vision.
 */

import type { ProviderCapabilities, ProviderProfile, Settings, TaskCategory } from '../types';
import { DEFAULT_SETTINGS } from '../types';
import { anthropicAdapter } from './anthropic';
import { geminiAdapter } from './gemini';
import { ollamaAdapter } from './ollama';
import { openAiAdapter } from './openai';
import { ProviderError, type ChatRequest, type ChatResult, type ProviderAdapter, type StreamHandlers, type TranscriptionResult } from './types';

export * from './types';
export { ProviderError } from './types';
export { OPENAI_COMPATIBLE_PRESETS } from './types';

export const ADAPTERS: ProviderAdapter[] = [openAiAdapter, anthropicAdapter, geminiAdapter, ollamaAdapter];

export function getAdapter(protocol: ProviderProfile['protocol']): ProviderAdapter {
  const adapter = ADAPTERS.find((a) => a.id === protocol);
  if (!adapter) throw new ProviderError('unsupported', `Unknown provider protocol “${protocol}”.`);
  return adapter;
}

export const NO_CAPABILITIES: ProviderCapabilities = {
  text: false,
  vision: false,
  embeddings: false,
  tts: false,
  transcription: false,
  streaming: false,
};

export function effectiveCapabilities(profile: ProviderProfile): ProviderCapabilities {
  let max: ProviderCapabilities;
  try {
    max = getAdapter(profile.protocol).capabilities;
  } catch {
    return { ...NO_CAPABILITIES };
  }
  const declared = { ...NO_CAPABILITIES, ...profile.capabilities };
  return {
    text: max.text && declared.text,
    vision: max.vision && declared.vision,
    embeddings: max.embeddings && declared.embeddings,
    tts: max.tts && declared.tts,
    transcription: max.transcription && declared.transcription,
    streaming: max.streaming && declared.streaming,
  };
}

export function activeProvider(settings: Settings): ProviderProfile | null {
  if (!settings.activeProviderId) return null;
  return settings.providers.find((p) => p.id === settings.activeProviderId) ?? null;
}

export function providerForTask(settings: Settings, task: TaskCategory): ProviderProfile | null {
  const active = activeProvider(settings);
  if (active) {
    const caps = effectiveCapabilities(active);
    const ok =
      (task === 'vision' && caps.vision) ||
      (task === 'embedding' && caps.embeddings) ||
      (task === 'tts' && caps.tts) ||
      (task === 'transcription' && caps.transcription) ||
      (['chat', 'summary', 'study', 'audioScript'].includes(task) && caps.text);
    if (ok) return active;
  }
  // Fall back to the first provider that advertises the required capability.
  const need = capabilityFor(task);
  return (
    settings.providers.find((p) => {
      const caps = effectiveCapabilities(p);
      return caps[need] && (need !== 'text' || Boolean(p.apiKey || p.protocol === 'ollama'));
    }) ?? null
  );
}

function capabilityFor(task: TaskCategory): keyof ProviderCapabilities {
  switch (task) {
    case 'vision':
      return 'vision';
    case 'embedding':
      return 'embeddings';
    case 'tts':
      return 'tts';
    case 'transcription':
      return 'transcription';
    default:
      return 'text';
  }
}

export function resolveModel(profile: ProviderProfile, task: TaskCategory): string {
  return profile.models[task] || profile.defaultModel;
}

export function validateProfile(profile: ProviderProfile): string[] {
  const errors: string[] = [];
  if (!profile.label.trim()) errors.push('Give the provider a label.');
  if (!profile.baseUrl.trim()) errors.push('Base URL is required.');
  else {
    try {
      const url = new URL(profile.baseUrl);
      const loopback = ['localhost', '127.0.0.1', '::1'].includes(url.hostname);
      if (url.protocol !== 'https:' && !(loopback && url.protocol === 'http:')) {
        errors.push('Use HTTPS for remote endpoints (HTTP is only allowed for localhost).');
      }
    } catch {
      errors.push('Base URL is not a valid URL.');
    }
  }
  if (!profile.defaultModel.trim()) errors.push('A default model name is required.');
  if (!profile.apiKey.trim() && profile.protocol !== 'ollama') {
    errors.push('An API key is required for this protocol (it is kept in memory unless you opt in to saving it).');
  }
  for (const [task, model] of Object.entries(profile.models)) {
    if (model !== undefined && !model.trim()) errors.push(`Model for “${task}” cannot be blank.`);
  }
  return errors;
}

export function assertCapability(profile: ProviderProfile, task: TaskCategory): void {
  const caps = effectiveCapabilities(profile);
  const need = capabilityFor(task);
  if (!caps[need]) {
    throw new ProviderError(
      'unsupported',
      `“${profile.label}” is not marked as ${need}-capable. Enable that capability in Settings only if the selected model really supports it.`,
    );
  }
}

export interface RunChatOptions {
  task?: TaskCategory;
  request: ChatRequest;
  onDelta?: (delta: string) => void;
}

export async function runChat(profile: ProviderProfile, options: RunChatOptions): Promise<ChatResult> {
  const task = options.task ?? 'chat';
  assertCapability(profile, task);
  const adapter = getAdapter(profile.protocol);
  const model = options.request.model || resolveModel(profile, task);
  const request: ChatRequest = { ...options.request, model };
  if (!request.model) throw new ProviderError('bad-request', 'No model configured for this task.');
  if (options.onDelta) {
    if (!effectiveCapabilities(profile).streaming) {
      const result = await adapter.chat(profile, request);
      options.onDelta(result.text);
      return result;
    }
    const handlers: StreamHandlers = { onDelta: options.onDelta };
    return adapter.streamChat(profile, request, handlers);
  }
  return adapter.chat(profile, request);
}

export async function embedTexts(profile: ProviderProfile, texts: string[], signal?: AbortSignal): Promise<number[][]> {
  assertCapability(profile, 'embedding');
  const adapter = getAdapter(profile.protocol);
  if (!adapter.embed) throw new ProviderError('unsupported', 'This protocol does not expose an embeddings endpoint.');
  return adapter.embed(profile, resolveModel(profile, 'embedding'), texts, signal);
}

export async function transcribeAudio(profile: ProviderProfile, audio: Blob, signal?: AbortSignal): Promise<TranscriptionResult> {
  assertCapability(profile, 'transcription');
  const adapter = getAdapter(profile.protocol);
  if (!adapter.transcribe) throw new ProviderError('unsupported', 'This protocol does not expose a transcription endpoint.');
  return adapter.transcribe(profile, resolveModel(profile, 'transcription'), audio, signal);
}

export async function synthesizeSpeech(profile: ProviderProfile, text: string, signal?: AbortSignal): Promise<Blob> {
  assertCapability(profile, 'tts');
  const adapter = getAdapter(profile.protocol);
  if (!adapter.synthesize) throw new ProviderError('unsupported', 'This protocol does not expose a speech endpoint.');
  return adapter.synthesize(profile, resolveModel(profile, 'tts'), text, signal);
}

export async function testConnection(profile: ProviderProfile): Promise<{ ok: boolean; message: string; models?: string[] }> {
  const errors = validateProfile(profile);
  if (errors.length) return { ok: false, message: errors.join(' ') };
  try {
    const result = await runChat(profile, {
      task: 'chat',
      request: {
        messages: [{ role: 'user', content: 'Reply with exactly: ok' }],
        model: resolveModel(profile, 'chat'),
        maxTokens: 8,
        timeoutMs: 30000,
      },
    });
    return { ok: true, message: `Connected. Model replied: “${result.text.trim().slice(0, 60)}”` };
  } catch (err) {
    const message = err instanceof ProviderError ? `${err.code}: ${err.message}` : err instanceof Error ? err.message : String(err);
    return { ok: false, message };
  }
}

export function settingsWithProvider(settings: Settings, profile: ProviderProfile): Settings {
  const exists = settings.providers.some((p) => p.id === profile.id);
  return {
    ...settings,
    providers: exists ? settings.providers.map((p) => (p.id === profile.id ? profile : p)) : [...settings.providers, profile],
    activeProviderId: settings.activeProviderId ?? profile.id,
  };
}

export function defaultSettings(): Settings {
  return { ...DEFAULT_SETTINGS };
}
