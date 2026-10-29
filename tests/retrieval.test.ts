import { describe, expect, it } from 'vitest';
import { buildChunks } from '../src/lib/text';
import { makeSource } from '../src/lib/ingest';
import { retrieve, detectConflicts, cosineSimilarity } from '../src/lib/retrieval';
import { validateAnswerCitations, verifyQuote } from '../src/lib/citations';
import type { Chunk, Source } from '../src/lib/types';

const CHUNK_OPTS = { chunkSize: 400, chunkOverlap: 40 };

function corpus(): { sources: Source[]; chunks: Chunk[] } {
  const notebookId = 'nb-test';
  const specs: { title: string; text: string; kind?: Source['kind'] }[] = [
    {
      title: 'Market report',
      text: 'The market share of the product grew by 40 percent in 2023. Enterprise customers drove most of the growth. Renewal rates reached 92 percent among enterprise accounts.',
    },
    {
      title: 'Analyst note',
      text: 'A rival analyst claims the market share was 80 percent, which conflicts with other estimates. The analyst describes pricing pressure and bundling.',
    },
    {
      title: 'Gardening guide',
      text: 'Tomatoes need full sun and well drained soil. Water deeply twice a week and mulch to retain moisture. Soil pH should be between six and seven.',
    },
  ];
  const sources = specs.map((spec) =>
    makeSource({
      notebookId,
      title: spec.title,
      kind: spec.kind ?? 'text',
      text: spec.text,
      origin: { fileName: `${spec.title}.txt` },
    }),
  );
  const chunks = sources.flatMap((source) => buildChunks(source.id, notebookId, source.text, CHUNK_OPTS, source.segments));
  return { sources, chunks };
}

describe('retrieval', () => {
  const { sources, chunks } = corpus();

  it('returns relevant evidence with resolvable locators and markers', () => {
    const result = retrieve({ query: 'enterprise renewal rates', chunks, sources, topK: 5 });
    expect(result.insufficient).toBe(false);
    expect(result.hits.length).toBeGreaterThan(0);
    expect(result.evidence.length).toBe(result.hits.length);
    expect(result.evidence[0].marker).toBe(1);
    expect(result.evidence[0].sourceTitle).toBe('Market report');
    expect(result.evidence[0].locator).toMatch(/lines? \d/);
    expect(result.evidence[0].quote.toLowerCase()).toContain('renewal');
  });

  it('reports insufficient evidence for unrelated questions', () => {
    const result = retrieve({ query: 'quantum chromodynamics lattice spacing', chunks, sources, topK: 5 });
    expect(result.insufficient).toBe(true);
    expect(result.reason).toMatch(/do not contain strong enough evidence/i);
    expect(result.evidence.length).toBeLessThanOrEqual(3);
  });

  it('handles an empty notebook and empty query without throwing', () => {
    expect(retrieve({ query: 'anything', chunks: [], sources: [] }).insufficient).toBe(true);
    expect(retrieve({ query: '   ', chunks, sources }).insufficient).toBe(true);
  });

  it('respects the active source filter', () => {
    const gardening = sources.find((s) => s.title === 'Gardening guide')!;
    const filtered = retrieve({ query: 'enterprise renewal rates', chunks, sources, activeSourceIds: [gardening.id], topK: 5 });
    expect(filtered.hits.every((hit) => hit.source.id === gardening.id)).toBe(true);
    const all = retrieve({ query: 'soil pH', chunks, sources, topK: 5 });
    expect(all.hits[0].source.title).toBe('Gardening guide');
  });

  it('detects contradictory figures across sources', () => {
    const result = retrieve({ query: 'market share percent', chunks, sources, topK: 5 });
    expect(result.conflicts.length).toBeGreaterThan(0);
    expect(result.conflicts[0].sourceA).not.toBe(result.conflicts[0].sourceB);
    expect(result.conflicts[0].note).toMatch(/Sources disagree/);
  });

  it('never reports a conflict when the numbers agree', () => {
    const notebookId = 'nb-agree';
    const a = makeSource({ notebookId, title: 'A', kind: 'text', text: 'The conversion rate was 12 percent in the benchmark.' });
    const b = makeSource({ notebookId, title: 'B', kind: 'text', text: 'The conversion rate was 12 percent in the benchmark.' });
    const localChunks = [a, b].flatMap((s) => buildChunks(s.id, notebookId, s.text, CHUNK_OPTS, s.segments));
    expect(detectConflicts(retrieve({ query: 'conversion rate percent', chunks: localChunks, sources: [a, b] }).hits)).toHaveLength(0);
  });

  it('deduplicates identical passages', () => {
    const notebookId = 'nb-dupe';
    const text = 'Identical paragraph about caching strategies and invalidation rules for distributed systems.';
    const a = makeSource({ notebookId, title: 'Copy A', kind: 'text', text });
    const b = makeSource({ notebookId, title: 'Copy B', kind: 'text', text });
    const localChunks = [a, b].flatMap((s) => buildChunks(s.id, notebookId, s.text, CHUNK_OPTS, s.segments));
    const result = retrieve({ query: 'caching invalidation distributed', chunks: localChunks, sources: [a, b], topK: 5 });
    expect(result.hits).toHaveLength(1);
    expect(result.diagnostics.deduped).toBeGreaterThanOrEqual(1);
  });

  it('uses vector similarity when embeddings are available', () => {
    const notebookId = 'nb-vec';
    const text = 'Deep learning models require tokenization and embeddings for semantic retrieval.';
    const source = makeSource({ notebookId, title: 'Vectors', kind: 'text', text });
    const localChunks = buildChunks(source.id, notebookId, source.text, CHUNK_OPTS, source.segments).map((chunk) => ({
      ...chunk,
      embedding: [0.9, 0.1, 0.0],
    }));
    const result = retrieve({
      query: 'completely different words zebra',
      chunks: localChunks,
      sources: [source],
      queryEmbedding: [1, 0, 0],
      topK: 3,
      threshold: 0.1,
    });
    expect(result.diagnostics.vectorCandidates).toBe(1);
    expect(result.hits.length).toBe(1);
    expect(cosineSimilarity([1, 0], [1, 0])).toBeCloseTo(1);
    expect(cosineSimilarity([1, 0], [0, 1])).toBeCloseTo(0);
  });

  it('reports diagnostics and never exceeds topK', () => {
    const result = retrieve({ query: 'market share pricing enterprise', chunks, sources, topK: 2 });
    expect(result.hits.length).toBeLessThanOrEqual(2);
    expect(result.diagnostics.candidateChunks).toBeGreaterThan(0);
    expect(result.diagnostics.budgetLimit).toBe(2);
  });
});

describe('citation validation', () => {
  const evidence = [
    { marker: 1, chunkId: 'c1', sourceId: 's1', sourceTitle: 'S1', quote: 'Quote one', locator: 'lines 1–2', score: 0.9 },
    { marker: 2, chunkId: 'c2', sourceId: 's2', sourceTitle: 'S2', quote: 'Quote two', locator: 'p. 3', score: 0.7 },
  ];

  it('keeps valid markers and strips invented ones', () => {
    const result = validateAnswerCitations('Fact A [1]. Fact B [7]. Fact C [2].', evidence);
    expect(result.citedMarkers).toEqual([1, 2]);
    expect(result.invalidMarkers).toEqual([7]);
    expect(result.cleanedText).not.toContain('[7]');
    expect(result.citations.map((c) => c.marker)).toEqual([1, 2]);
    expect(result.hasAnyCitation).toBe(true);
  });

  it('flags uncited long sentences', () => {
    const result = validateAnswerCitations('This sentence is long enough to be considered a claim that needs support from a source. It has no marker. [1]', evidence);
    expect(result.uncitedSentences.length).toBeGreaterThanOrEqual(1);
  });

  it('verifies stored quotes against chunk text', () => {
    expect(verifyQuote('Quote one', 'prefix Quote one suffix')).toBe(true);
    expect(verifyQuote('Completely invented passage', 'some other text')).toBe(false);
  });
});
