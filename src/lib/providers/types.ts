import type { ProviderCapabilities, ProviderProfile, TaskCategory } from '../types';

export type ProviderErrorCode =
  | 'missing-key'
  | 'invalid-key'
  | 'forbidden'
  | 'rate-limit'
  | 'timeout'
  | 'network'
  | 'cors'
  | 'bad-response'
  | 'unsupported'
  | 'invalid-endpoint'
  | 'bad-request'
  | 'cancelled';

export class ProviderError extends Error {
  code: ProviderErrorCode;
  status?: number;
  retryAfterMs?: number;

  constructor(code: ProviderErrorCode, message: string, status?: number, retryAfterMs?: number) {
    super(message);
    this.name = 'ProviderError';
    this.code = code;
    if (status !== undefined) this.status = status;
    if (retryAfterMs !== undefined) this.retryAfterMs = retryAfterMs;
  }
}

export interface ChatMessageInput {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface ChatRequest {
  messages: ChatMessageInput[];
  model: string;
  temperature?: number;
  maxTokens?: number;
  /** Data URLs (data:image/...;base64,...) for vision-capable models. */
  images?: string[];
  signal?: AbortSignal;
  timeoutMs?: number;
}

export interface ChatResult {
  text: string;
  model: string;
  usage?: { promptTokens?: number; completionTokens?: number };
  raw?: unknown;
}

export interface StreamHandlers {
  onDelta: (delta: string) => void;
}

export interface TranscriptionResult {
  text: string;
  segments?: { start: number; end: number; text: string }[];
  language?: string;
}

export interface ProviderAdapter {
  id: string;
  label: string;
  description: string;
  defaultBaseUrl: string;
  /** Capabilities this protocol can support when the model supports them. */
  capabilities: ProviderCapabilities;
  chat(profile: ProviderProfile, request: ChatRequest): Promise<ChatResult>;
  streamChat(profile: ProviderProfile, request: ChatRequest, handlers: StreamHandlers): Promise<ChatResult>;
  embed?(profile: ProviderProfile, model: string, texts: string[], signal?: AbortSignal): Promise<number[][]>;
  transcribe?(profile: ProviderProfile, model: string, audio: Blob, signal?: AbortSignal): Promise<TranscriptionResult>;
  synthesize?(profile: ProviderProfile, model: string, text: string, signal?: AbortSignal): Promise<Blob>;
  listModels?(profile: ProviderProfile): Promise<string[]>;
}

export interface ResolvedModel {
  model: string;
  task: TaskCategory;
}

export const OPENAI_COMPATIBLE_PRESETS: {
  id: string;
  label: string;
  baseUrl: string;
  notes: string;
  caps: Partial<ProviderCapabilities>;
}[] = [
  {
    id: 'openai',
    label: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    notes: 'Chat, vision, embeddings, TTS and Whisper transcription. CORS-enabled.',
    caps: { vision: true, embeddings: true, tts: true, transcription: true, streaming: true },
  },
  {
    id: 'openrouter',
    label: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    notes: 'Many models behind one key. CORS-enabled. Text/vision depend on the model you pick.',
    caps: { vision: true, streaming: true },
  },
  {
    id: 'groq',
    label: 'Groq',
    baseUrl: 'https://api.groq.com/openai/v1',
    notes: 'Fast text models, Whisper transcription and TTS. CORS-enabled.',
    caps: { transcription: true, tts: true, streaming: true },
  },
  {
    id: 'mistral',
    label: 'Mistral',
    baseUrl: 'https://api.mistral.ai/v1',
    notes: 'Text and embeddings. Vision on some models.',
    caps: { vision: true, embeddings: true, streaming: true },
  },
  {
    id: 'deepseek',
    label: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    notes: 'Text chat models.',
    caps: { streaming: true },
  },
  {
    id: 'lmstudio',
    label: 'LM Studio (local)',
    baseUrl: 'http://localhost:1234/v1',
    notes: 'Local OpenAI-compatible server. HTTP allowed for loopback only.',
    caps: { embeddings: true, streaming: true },
  },
  {
    id: 'ollama-openai',
    label: 'Ollama (OpenAI-compatible)',
    baseUrl: 'http://localhost:11434/v1',
    notes: 'Local Ollama via its OpenAI-compatible endpoint.',
    caps: { embeddings: true, streaming: true },
  },
];
