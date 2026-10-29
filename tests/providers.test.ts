import { describe, expect, it, vi } from 'vitest';
import {
  ProviderError,
  effectiveCapabilities,
  embedTexts,
  providerForTask,
  resolveModel,
  runChat,
  synthesizeSpeech,
  testConnection,
  transcribeAudio,
  validateProfile,
} from '../src/lib/providers';
import { DEFAULT_SETTINGS, type ProviderProfile } from '../src/lib/types';

function profile(overrides: Partial<ProviderProfile> = {}): ProviderProfile {
  return {
    id: 'p1',
    label: 'Test OpenAI',
    protocol: 'openai',
    baseUrl: 'https://api.example.com/v1',
    apiKey: 'test-key',
    defaultModel: 'test-model',
    models: {},
    capabilities: { text: true, vision: true, embeddings: true, tts: true, transcription: true, streaming: true },
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json', ...headers } });
}

function sseResponse(chunks: string[]): Response {
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

describe('provider validation and capability model', () => {
  it('requires HTTPS for remote endpoints but allows loopback HTTP', () => {
    expect(validateProfile(profile({ baseUrl: 'http://api.example.com/v1' })).join(' ')).toMatch(/HTTPS/);
    expect(validateProfile(profile({ baseUrl: 'http://localhost:1234/v1' }))).toHaveLength(0);
  });

  it('requires a key unless the protocol is local Ollama', () => {
    expect(validateProfile(profile({ apiKey: '' })).join(' ')).toMatch(/API key is required/);
    const ollama = profile({ protocol: 'ollama', baseUrl: 'http://localhost:11434', apiKey: '', capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: true } });
    expect(validateProfile(ollama)).toHaveLength(0);
  });

  it('intersects declared capabilities with protocol maximums', () => {
    const anthropic = profile({ protocol: 'anthropic', baseUrl: 'https://api.anthropic.com/v1', capabilities: { text: true, vision: true, embeddings: true, tts: true, transcription: true, streaming: true } });
    const caps = effectiveCapabilities(anthropic);
    expect(caps.embeddings).toBe(false);
    expect(caps.tts).toBe(false);
    expect(caps.transcription).toBe(false);
    expect(caps.vision).toBe(true);
  });

  it('routes models per task with a default fallback', () => {
    const p = profile({ models: { study: 'study-model' } });
    expect(resolveModel(p, 'study')).toBe('study-model');
    expect(resolveModel(p, 'chat')).toBe('test-model');
  });

  it('selects a provider by the capability a task needs', () => {
    const textOnly = profile({ id: 'text', capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false } });
    const vision = profile({ id: 'vision', capabilities: { text: true, vision: true, embeddings: false, tts: false, transcription: false, streaming: false } });
    const settings = { ...DEFAULT_SETTINGS, providers: [textOnly, vision], activeProviderId: 'text' };
    expect(providerForTask(settings, 'chat')?.id).toBe('text');
    expect(providerForTask(settings, 'vision')?.id).toBe('vision');
    expect(providerForTask(settings, 'tts')).toBeNull();
  });

  it('rejects vision requests on a text-only profile', async () => {
    const textOnly = profile({ capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false } });
    await expect(
      runChat(textOnly, { task: 'vision', request: { messages: [{ role: 'user', content: 'hi' }], model: 'x' } }),
    ).rejects.toMatchObject({ code: 'unsupported' });
  });
});

describe('OpenAI-compatible adapter', () => {
  it('sends a chat completion and returns text, model and usage', async () => {
    const fetchMock = vi.fn(async () =>
      jsonResponse({ model: 'test-model-0613', choices: [{ message: { content: 'Grounded answer [1].' } }], usage: { prompt_tokens: 12, completion_tokens: 5 } }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const result = await runChat(profile(), { request: { messages: [{ role: 'user', content: 'question' }], model: 'test-model' } });
    expect(result.text).toBe('Grounded answer [1].');
    expect(result.model).toBe('test-model-0613');
    expect(result.usage?.promptTokens).toBe(12);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('https://api.example.com/v1/chat/completions');
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer test-key');
    const body = JSON.parse(String(init.body)) as { stream: boolean; messages: unknown[] };
    expect(body.stream).toBe(false);
    expect(body.messages).toHaveLength(1);
  });

  it('maps 401 to invalid-key, 429 to rate-limit and network failure to a CORS explanation', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: { message: 'bad key' } }, 401)));
    await expect(runChat(profile(), { request: { messages: [], model: 'm' } })).rejects.toMatchObject({ code: 'invalid-key' });

    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: { message: 'slow down' } }, 429, { 'retry-after': '2' })));
    const rateLimited = await runChat(profile(), { request: { messages: [], model: 'm' } }).catch((err: ProviderError) => err);
    expect(rateLimited).toMatchObject({ code: 'rate-limit', retryAfterMs: 2000 });

    vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('Failed to fetch'); }));
    const network = await runChat(profile(), { request: { messages: [], model: 'm' } }).catch((err: ProviderError) => err);
    expect(network).toMatchObject({ code: 'cors' });
    expect((network as ProviderError).message).toMatch(/CORS/);
  });

  it('rejects malformed responses instead of pretending', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('not json', { status: 200 })));
    await expect(runChat(profile(), { request: { messages: [], model: 'm' } })).rejects.toBeInstanceOf(Error);
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ choices: [{ message: { content: '' } }] })));
    await expect(runChat(profile(), { request: { messages: [], model: 'm' } })).rejects.toMatchObject({ code: 'bad-response' });
  });

  it('streams deltas to the caller', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        sseResponse([
          'data: {"choices":[{"delta":{"content":"Hello "}}]}\n\n',
          'data: {"choices":[{"delta":{"content":"world [1]"}}]}\n\n',
          'data: [DONE]\n\n',
        ]),
      ),
    );
    const deltas: string[] = [];
    const result = await runChat(profile(), {
      request: { messages: [{ role: 'user', content: 'hi' }], model: 'm' },
      onDelta: (d) => deltas.push(d),
    });
    expect(deltas).toEqual(['Hello ', 'world [1]']);
    expect(result.text).toBe('Hello world [1]');
  });

  it('includes images only for vision-capable profiles', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ choices: [{ message: { content: 'ok' } }] }));
    vi.stubGlobal('fetch', fetchMock);
    await runChat(profile(), {
      task: 'vision',
      request: { messages: [{ role: 'user', content: 'describe' }], model: 'm', images: ['data:image/jpeg;base64,AAA'] },
    });
    const body = JSON.parse(String((fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1].body)) as {
      messages: { content: unknown }[];
    };
    expect(Array.isArray(body.messages[0].content)).toBe(true);
  });

  it('embeds texts in order and rejects empty embeddings', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ data: [{ index: 1, embedding: [0, 1] }, { index: 0, embedding: [1, 0] }] })));
    const vectors = await embedTexts(profile(), ['a', 'b']);
    expect(vectors).toEqual([[1, 0], [0, 1]]);
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ data: [] })));
    await expect(embedTexts(profile(), ['a'])).rejects.toMatchObject({ code: 'bad-response' });
  });

  it('synthesizes speech to a blob and transcribes audio', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(new Uint8Array([1, 2, 3]).buffer, { status: 200, headers: { 'content-type': 'audio/mpeg' } })));
    const audio = await synthesizeSpeech(profile(), 'hello there');
    expect(audio.size).toBe(3);

    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse({ text: 'transcribed words', segments: [{ start: 0, end: 1.5, text: 'transcribed words' }], language: 'en' })),
    );
    const transcript = await transcribeAudio(profile(), new Blob(['audio']));
    expect(transcript.text).toBe('transcribed words');
    expect(transcript.segments?.[0]).toMatchObject({ start: 0, end: 1500 });
  });

  it('tests a connection and reports a readable failure', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ choices: [{ message: { content: 'ok' } }] })));
    const good = await testConnection(profile());
    expect(good.ok).toBe(true);
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: { message: 'nope' } }, 401)));
    const bad = await testConnection(profile());
    expect(bad.ok).toBe(false);
    expect(bad.message).toMatch(/invalid-key/);
  });
});

describe('other protocols', () => {
  it('talks to Anthropic messages with the required headers', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ content: [{ type: 'text', text: 'answer' }], model: 'claude-test', usage: { input_tokens: 3, output_tokens: 2 } }));
    vi.stubGlobal('fetch', fetchMock);
    const result = await runChat(
      profile({ protocol: 'anthropic', baseUrl: 'https://api.anthropic.com/v1', capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false } }),
      { request: { messages: [{ role: 'system', content: 'be nice' }, { role: 'user', content: 'hi' }], model: 'claude-test' } },
    );
    expect(result.text).toBe('answer');
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    const headers = init.headers as Record<string, string>;
    expect(headers['x-api-key']).toBe('test-key');
    expect(headers['anthropic-version']).toBe('2023-06-01');
    const body = JSON.parse(String(init.body)) as { system: string; messages: unknown[] };
    expect(body.system).toBe('be nice');
    expect(body.messages).toHaveLength(1);
  });

  it('talks to Gemini with the key in the query string', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ candidates: [{ content: { parts: [{ text: 'gemini answer' }] } }] }));
    vi.stubGlobal('fetch', fetchMock);
    const result = await runChat(
      profile({ protocol: 'gemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta', defaultModel: 'gemini-test', capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false } }),
      { request: { messages: [{ role: 'user', content: 'hi' }], model: 'gemini-test' } },
    );
    expect(result.text).toBe('gemini answer');
    const [url] = fetchMock.mock.calls[0] as unknown as [string];
    expect(url).toContain('models/gemini-test:generateContent');
    expect(url).toContain('key=test-key');
  });

  it('talks to native Ollama without an API key', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ message: { content: 'llama says hi' }, model: 'llama3', prompt_eval_count: 5, eval_count: 3 }));
    vi.stubGlobal('fetch', fetchMock);
    const result = await runChat(
      profile({ protocol: 'ollama', baseUrl: 'http://localhost:11434', apiKey: '', defaultModel: 'llama3', capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false } }),
      { request: { messages: [{ role: 'user', content: 'hi' }], model: 'llama3' } },
    );
    expect(result.text).toBe('llama says hi');
    const [url] = fetchMock.mock.calls[0] as unknown as [string];
    expect(url).toBe('http://localhost:11434/api/chat');
  });

  it('blocks plain HTTP for remote hosts before any request is made', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    await expect(
      runChat(profile({ baseUrl: 'http://insecure.example.com/v1' }), { request: { messages: [], model: 'm' } }),
    ).rejects.toMatchObject({ code: 'invalid-endpoint' });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
