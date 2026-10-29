import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { chunkText, tokenize, contentTokens } from '../src/lib/text';
import { buildBm25Index, searchBm25 } from '../src/lib/bm25';
import { initEngine, getEngine, resetEngineForTests } from '../src/lib/engine';

interface ChunkFixture {
  name: string;
  text: string;
  chunkSize: number;
  chunkOverlap: number;
  expected: { text: string; startLine: number; endLine: number; tokens: number }[];
}

const chunkFixtures = JSON.parse(readFileSync('core/tests/fixtures/chunking.json', 'utf8')) as ChunkFixture[];
const rankFixtures = JSON.parse(readFileSync('core/tests/fixtures/ranking.json', 'utf8')) as {
  docs: { id: string; text: string }[];
  query: string;
  expected: { id: string; score: number }[];
}[];

describe('chunking parity with the Rust fixture', () => {
  it.each(chunkFixtures.map((c) => [c.name, c] as const))('case %s', (_name, fixture) => {
    const chunks = chunkText(fixture.text, { chunkSize: fixture.chunkSize, chunkOverlap: fixture.chunkOverlap });
    expect(chunks).toHaveLength(fixture.expected.length);
    chunks.forEach((chunk, index) => {
      expect({
        text: chunk.text,
        startLine: chunk.startLine,
        endLine: chunk.endLine,
        tokens: chunk.tokens,
      }).toEqual(fixture.expected[index]);
    });
  });

  it('never emits an empty or overlap-only trailing chunk', () => {
    const text = Array.from({ length: 30 }, (_, i) => `line-${i}-${'x'.repeat(20)}`).join('\n');
    const chunks = chunkText(text, { chunkSize: 120, chunkOverlap: 40 });
    expect(chunks.length).toBeGreaterThan(3);
    for (const chunk of chunks) expect(chunk.text.trim().length).toBeGreaterThan(0);
    const unique = new Set(chunks.map((c) => c.text));
    expect(unique.size).toBe(chunks.length);
  });

  it('keeps long lines referenceable by the original line number', () => {
    const chunks = chunkText(`${'y'.repeat(300)}\nend`, { chunkSize: 100, chunkOverlap: 0 });
    expect(chunks.length).toBeGreaterThan(2);
    expect(chunks[0].startLine).toBe(1);
    expect(chunks.every((c) => c.startLine >= 1)).toBe(true);
    expect(chunks[chunks.length - 1].endLine).toBe(2);
  });
});

describe('BM25 parity with the Rust fixture', () => {
  it.each(rankFixtures.map((f, i) => [f.query, f, i] as const))('query %s', (_q, fixture) => {
    const index = buildBm25Index(fixture.docs);
    const results = searchBm25(index, fixture.query, 10);
    expect(results).toHaveLength(fixture.expected.length);
    results.forEach((result, i) => {
      expect(result.id).toBe(fixture.expected[i].id);
      expect(result.score).toBeCloseTo(fixture.expected[i].score, 5);
    });
  });

  it('tokenizes camelCase, snake_case and digits like the Rust core', () => {
    expect(tokenize('camelCase snake_case APIKey 2024')).toEqual(['camel', 'case', 'snake', 'case', 'apikey', '2024']);
    expect(contentTokens('the quick brown fox')).toEqual(['quick', 'brown', 'fox']);
  });
});

describe('engine bridge', () => {
  it('falls back to TypeScript when no WASM module is present, and still chunks', async () => {
    resetEngineForTests();
    const status = await initEngine('./wasm/does-not-exist.js');
    expect(status.engine).toBe('typescript');
    expect(status.detail).toMatch(/fallback/i);
    const engine = getEngine();
    expect(engine.id).toBe('typescript');
    expect(engine.chunk('hello world', { chunkSize: 900, chunkOverlap: 0 })[0].text).toBe('hello world');
  });

  it('ranks documents through the engine interface', () => {
    const engine = getEngine();
    const results = engine.bm25(
      [
        { id: 'a', text: 'vector databases store embeddings' },
        { id: 'b', text: 'gardening tips for tomatoes' },
      ],
      'embeddings',
      5,
    );
    expect(results[0].id).toBe('a');
  });
});
