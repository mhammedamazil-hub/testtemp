/**
 * Rust core bridge.
 *
 * The Rust crate in `core/` compiles to WebAssembly (`wasm-pack build`). The
 * generated package is placed in `public/wasm/` at build time by CI or
 * `npm run wasm`. When it is present and passes a parity smoke test, the app
 * routes chunking and ranking through Rust. Otherwise it uses the TypeScript
 * implementation and reports the fallback honestly in the UI.
 */

import { buildBm25Index, searchBm25 } from './bm25';
import { chunkText, type ChunkOptions, type RawChunk } from './text';

export interface CoreEngine {
  id: 'rust-wasm' | 'typescript';
  chunk(text: string, options: ChunkOptions): RawChunk[];
  bm25(docs: { id: string; text: string }[], query: string, limit: number): { id: string; score: number }[];
}

function tsEngine(): CoreEngine {
  return {
    id: 'typescript',
    chunk: (text, options) => chunkText(text, options),
    bm25: (docs, query, limit) => {
      const index = buildBm25Index(docs);
      return searchBm25(index, query, limit).map((r) => ({ id: r.id, score: r.score }));
    },
  };
}

export interface EngineStatus {
  engine: 'rust-wasm' | 'typescript';
  detail: string;
  version?: string;
}

let active: CoreEngine = tsEngine();
let status: EngineStatus = { engine: 'typescript', detail: 'TypeScript fallback (Rust/WASM module not loaded).' };

export function getEngine(): CoreEngine {
  return active;
}

export function engineStatus(): EngineStatus {
  return status;
}

export interface WasmModule {
  version?: () => string;
  chunk_text_json?: (text: string, size: number, overlap: number) => string;
  bm25_json?: (docsJson: string, query: string, limit: number) => string;
}

let initialised: Promise<EngineStatus> | null = null;

/**
 * Try to load the wasm-pack output. `importUrl` must be reachable as a module
 * (the build copies it to ./wasm/sourceforge_core.js).
 */
export type ModuleLoader = (url: string) => Promise<WasmModule & { default?: () => Promise<void> }>;

export async function initEngine(
  importUrl = './wasm/sourceforge_core.js',
  loadModule?: ModuleLoader,
): Promise<EngineStatus> {
  if (initialised) return initialised;
  initialised = (async () => {
    try {
      // Static hosts often serve index.html for missing paths; verify we are
      // really getting JavaScript before importing it.
      const head = await fetch(importUrl, { method: 'HEAD' });
      const contentType = head.headers.get('content-type') ?? '';
      if (!head.ok || !/javascript|ecmascript/.test(contentType)) {
        throw new Error(`no WASM glue at ${importUrl} (content-type: ${contentType || 'none'})`);
      }
      const mod = loadModule
        ? await loadModule(importUrl)
        : ((await import(/* @vite-ignore */ importUrl)) as WasmModule & { default?: () => Promise<void> });
      if (mod.default) await mod.default();
      if (typeof mod.chunk_text_json !== 'function') throw new Error('WASM module has no chunk_text_json export');
      const probeText = 'alpha beta gamma\n\ndelta epsilon zeta\n\neta theta iota kappa';
      const probeOptions = { chunkSize: 24, chunkOverlap: 6 };
      const probe = mod.chunk_text_json!(probeText, probeOptions.chunkSize, probeOptions.chunkOverlap);
      const parsed = JSON.parse(probe) as { text: string; startLine?: number; endLine?: number }[];
      if (!Array.isArray(parsed) || !parsed.length || !parsed[0].text) {
        throw new Error('WASM chunker returned an unexpected shape');
      }
      const jsProbe = chunkText(probeText, probeOptions);
      const wasmShape = parsed.map((chunk) => `${chunk.text}@${chunk.startLine ?? '?'}-${chunk.endLine ?? '?'}`);
      const jsShape = jsProbe.map((chunk) => `${chunk.text}@${chunk.startLine}-${chunk.endLine}`);
      const mismatch = wasmShape.findIndex((value, index) => value !== jsShape[index]);
      if (wasmShape.length !== jsShape.length || mismatch !== -1) {
        throw new Error(
          `WASM/JS chunk parity failed (${parsed.length} vs ${jsProbe.length} chunks${
            mismatch === -1 ? '' : `, first difference at index ${mismatch}`
          })`,
        );
      }
      active = {
        id: 'rust-wasm',
        chunk: (text, options) => {
          const json = mod.chunk_text_json!(text, options.chunkSize, options.chunkOverlap);
          return JSON.parse(json) as RawChunk[];
        },
        bm25: (docs, query, limit) => {
          const json = mod.bm25_json!(JSON.stringify(docs), query, limit);
          return JSON.parse(json) as { id: string; score: number }[];
        },
      };
      status = {
        engine: 'rust-wasm',
        detail: 'Rust core loaded via WebAssembly; chunking and BM25 ranking run in Rust.',
        ...(typeof mod.version === 'function' ? { version: mod.version() } : {}),
      };
    } catch (err) {
      status = {
        engine: 'typescript',
        detail: `TypeScript fallback: ${err instanceof Error ? err.message : String(err)}`,
      };
      active = tsEngine();
    }
    return status;
  })();
  return initialised;
}

export function resetEngineForTests(): void {
  initialised = null;
  active = tsEngine();
  status = { engine: 'typescript', detail: 'TypeScript fallback (reset).' };
}
