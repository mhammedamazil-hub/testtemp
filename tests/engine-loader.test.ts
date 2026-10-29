import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { chunkText } from '../src/lib/text';
import {
  engineStatus,
  getEngine,
  initEngine,
  resetEngineForTests,
  type WasmModule,
} from '../src/lib/engine';

/**
 * These tests exercise the loader contract, not a real .wasm binary (Rust
 * cannot be built in this environment). The fake module implements the same
 * JSON surface wasm-bindgen exposes, so routing, parity checks and fallback
 * messages are all real code paths.
 */
function stubHead(ok: boolean, contentType: string) {
  return vi.fn(async () => new Response(null, { status: ok ? 200 : 404, headers: { 'content-type': contentType } }));
}

function fakeModule(overrides: Partial<WasmModule> = {}): WasmModule {
  return {
    version: () => '0.1.0-test',
    chunk_text_json: (text, size, overlap) =>
      JSON.stringify(chunkText(text, { chunkSize: size, chunkOverlap: overlap })),
    bm25_json: () => JSON.stringify([{ id: 'a', score: 1.5 }]),
    ...overrides,
  };
}

describe('engine loader', () => {
  beforeEach(() => {
    resetEngineForTests();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    resetEngineForTests();
  });

  it('starts on the TypeScript engine with an honest status', () => {
    expect(getEngine().id).toBe('typescript');
    expect(engineStatus().engine).toBe('typescript');
    expect(engineStatus().detail).toMatch(/typescript fallback/i);
  });

  it('falls back when the import URL serves HTML instead of JavaScript', async () => {
    vi.stubGlobal('fetch', stubHead(true, 'text/html'));
    const status = await initEngine('./wasm/sourceforge_core.js');
    expect(status.engine).toBe('typescript');
    expect(status.detail).toContain('content-type: text/html');
    expect(getEngine().id).toBe('typescript');
  });

  it('falls back when the module is missing (404)', async () => {
    vi.stubGlobal('fetch', stubHead(false, 'application/javascript'));
    const status = await initEngine();
    expect(status.engine).toBe('typescript');
    expect(status.detail).toMatch(/no WASM glue/);
  });

  it('falls back when the module does not export the chunker', async () => {
    vi.stubGlobal('fetch', stubHead(true, 'application/javascript'));
    const status = await initEngine('./x.js', async () => ({}) as WasmModule);
    expect(status.engine).toBe('typescript');
    expect(status.detail).toMatch(/no chunk_text_json/);
  });

  it('falls back when chunk parity with TypeScript fails', async () => {
    vi.stubGlobal('fetch', stubHead(true, 'application/javascript'));
    const broken = fakeModule({ chunk_text_json: () => JSON.stringify([{ text: 'only-one' }]) });
    const status = await initEngine('./x.js', async () => broken);
    expect(status.engine).toBe('typescript');
    expect(status.detail).toMatch(/parity failed/i);
  });

  it('activates the Rust engine when parity passes and routes calls through it', async () => {
    vi.stubGlobal('fetch', stubHead(true, 'application/javascript'));
    const calls: unknown[] = [];
    const mod = fakeModule({
      chunk_text_json: (text, size, overlap) => {
        calls.push([text.length, size, overlap]);
        return JSON.stringify(chunkText(text, { chunkSize: size, chunkOverlap: overlap }));
      },
    });
    const status = await initEngine('./wasm/sourceforge_core.js', async () => mod);
    expect(status.engine).toBe('rust-wasm');
    expect(status.version).toBe('0.1.0-test');
    expect(status.detail).toMatch(/Rust core loaded/);

    const engine = getEngine();
    expect(engine.id).toBe('rust-wasm');
    const chunks = engine.chunk('alpha beta gamma\n\ndelta epsilon zeta', { chunkSize: 20, chunkOverlap: 0 });
    expect(chunks.length).toBeGreaterThan(0);
    expect(calls.length).toBeGreaterThan(0);

    const ranked = engine.bm25([{ id: 'a', text: 'alpha' }], 'alpha', 5);
    expect(ranked).toEqual([{ id: 'a', score: 1.5 }]);
  });

  it('caches the initialisation result instead of re-probing', async () => {
    const head = stubHead(true, 'text/html');
    vi.stubGlobal('fetch', head);
    await initEngine();
    await initEngine();
    expect(head).toHaveBeenCalledTimes(1);
  });

  it('keeps the TypeScript implementation importable by the UI for the badge', async () => {
    const { engineStatus: status } = await import('../src/lib/engine');
    expect(['typescript', 'rust-wasm']).toContain(status().engine);
  });
});
