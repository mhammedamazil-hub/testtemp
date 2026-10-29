/**
 * Local, deterministic evidence retrieval.
 *
 * Pipeline: source filter -> BM25 over chunks -> optional vector search ->
 * score fusion -> near-duplicate removal -> conflict detection -> context
 * budget -> citation-ready evidence list.
 *
 * No network access is required and no API key is involved.
 */

import { buildBm25Index, queryTerms, relevanceConfidence, searchBm25 } from './bm25';
import { evidenceFromHits, locatorForHit } from './citations';
import { contentTokens, locatorFor } from './text';
import type {
  Chunk,
  Conflict,
  RetrievalHit,
  RetrievalResult,
  Source,
} from './types';

export interface RetrieveOptions {
  query: string;
  chunks: Chunk[];
  sources: Source[];
  activeSourceIds?: string[] | null;
  topK?: number;
  threshold?: number;
  queryEmbedding?: number[] | null;
  /** Weight of the vector score when both rankings are available. */
  vectorWeight?: number;
}

export function cosineSimilarity(a: number[], b: number[]): number {
  if (!a.length || a.length !== b.length) return 0;
  let dot = 0;
  let na = 0;
  let nb = 0;
  for (let i = 0; i < a.length; i += 1) {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  if (!na || !nb) return 0;
  return dot / (Math.sqrt(na) * Math.sqrt(nb));
}

function fingerprint(text: string): string {
  return contentTokens(text).slice(0, 40).join(' ');
}

function jaccard(a: Set<string>, b: Set<string>): number {
  if (!a.size || !b.size) return 0;
  let inter = 0;
  for (const item of a) if (b.has(item)) inter += 1;
  return inter / (a.size + b.size - inter);
}

export function retrieve(options: RetrieveOptions): RetrievalResult {
  const topK = options.topK ?? 8;
  const threshold = options.threshold ?? 0.18;
  const vectorWeight = options.vectorWeight ?? 0.45;
  const sourceById = new Map(options.sources.map((s) => [s.id, s]));
  const active = options.activeSourceIds && options.activeSourceIds.length ? new Set(options.activeSourceIds) : null;

  const candidates = options.chunks.filter(
    (chunk) => sourceById.has(chunk.sourceId) && (!active || active.has(chunk.sourceId)),
  );

  const base: RetrievalResult = {
    query: options.query,
    hits: [],
    evidence: [],
    insufficient: true,
    reason: 'No chunks were available for this notebook.',
    usedSources: [],
    conflicts: [],
    diagnostics: {
      candidateChunks: candidates.length,
      lexicalCandidates: 0,
      vectorCandidates: 0,
      deduped: 0,
      budgetUsed: 0,
      budgetLimit: topK,
    },
  };
  if (!candidates.length) return base;
  if (!options.query.trim()) return { ...base, reason: 'Ask a question to retrieve evidence.' };

  const index = buildBm25Index(candidates.map((c) => ({ id: c.id, text: c.text })));
  const byId = new Map(candidates.map((c) => [c.id, c]));
  const lexical = searchBm25(index, options.query, Math.max(50, topK * 6));
  const lexicalIds = new Set(lexical.map((r) => r.id));

  const vector = new Map<string, number>();
  if (options.queryEmbedding && options.queryEmbedding.length) {
    for (const chunk of candidates) {
      if (!chunk.embedding || chunk.embedding.length !== options.queryEmbedding.length) continue;
      const sim = cosineSimilarity(options.queryEmbedding, chunk.embedding);
      if (sim > 0.05) vector.set(chunk.id, sim);
    }
  }

  type Scored = { id: string; score: number; lexicalScore: number; vectorScore?: number; coverage: number };
  const fused = new Map<string, Scored>();
  const maxLexical = lexical.length ? lexical[0].score : 0;
  for (const result of lexical) {
    fused.set(result.id, {
      id: result.id,
      score: maxLexical ? result.score / maxLexical : 0,
      lexicalScore: result.score,
      coverage: result.coverage,
    });
  }
  const maxVector = vector.size ? Math.max(...vector.values()) : 0;
  for (const [id, sim] of vector) {
    const existing = fused.get(id);
    const vectorScore = maxVector ? sim / maxVector : 0;
    if (existing) {
      existing.vectorScore = vectorScore;
      existing.score = (1 - vectorWeight) * existing.score + vectorWeight * vectorScore;
    } else {
      const chunk = byId.get(id);
      fused.set(id, {
        id,
        score: vectorWeight * vectorScore,
        lexicalScore: 0,
        vectorScore,
        coverage: chunk ? coverageOf(options.query, chunk.text) : 0,
      });
    }
  }

  const ranked = [...fused.values()].sort((a, b) => b.score - a.score || a.id.localeCompare(b.id));
  const deduped: Scored[] = [];
  const seen = new Map<string, string>();
  for (const item of ranked) {
    const chunk = byId.get(item.id);
    if (!chunk) continue;
    const source = sourceById.get(chunk.sourceId);
    const provenanceBoost = source?.status === 'ready' ? 1.02 : 1;
    const fp = fingerprint(chunk.text);
    const duplicate = fp && seen.has(fp);
    if (duplicate) continue;
    if (fp) seen.set(fp, item.id);
    deduped.push({ ...item, score: item.score * provenanceBoost });
    if (deduped.length >= topK * 2) break;
  }

  // Near-duplicate suppression across different lengths.
  const kept: Scored[] = [];
  for (const item of deduped) {
    const chunk = byId.get(item.id)!;
    const tokens = new Set(contentTokens(chunk.text).slice(0, 60));
    const isNearDup = kept.some((other) => {
      const otherChunk = byId.get(other.id)!;
      if (otherChunk.sourceId !== chunk.sourceId) return false;
      return jaccard(tokens, new Set(contentTokens(otherChunk.text).slice(0, 60))) > 0.9;
    });
    if (!isNearDup) kept.push(item);
    if (kept.length >= topK) break;
  }

  const hits: RetrievalHit[] = kept
    .map((item) => {
      const chunk = byId.get(item.id)!;
      const source = sourceById.get(chunk.sourceId)!;
      return {
        chunk,
        source,
        score: item.score,
        lexicalScore: item.lexicalScore,
        ...(item.vectorScore !== undefined ? { vectorScore: item.vectorScore } : {}),
        locator: locatorForHit({ chunk }),
      };
    })
    .filter((hit) => hit.score > 0);

  const best = hits[0];
  const bestCoverage = best
    ? (() => {
        const terms = queryTerms(options.query);
        if (!terms.length) return 0;
        const text = best.chunk.text.toLowerCase();
        return terms.filter((term) => text.includes(term)).length / terms.length;
      })()
    : 0;
  const confidence = best ? relevanceConfidence(best.lexicalScore, bestCoverage) : 0;
  const insufficient = !best || confidence < threshold;

  const evidence = evidenceFromHits(hits, Math.min(topK, hits.length));
  const usedSources = [...new Set(hits.map((h) => h.source.id))];

  const result: RetrievalResult = {
    query: options.query,
    hits: insufficient ? hits.slice(0, 3) : hits,
    evidence: insufficient ? evidence.slice(0, 3) : evidence,
    insufficient,
    reason: insufficient
      ? `The selected sources do not contain strong enough evidence (confidence ${(confidence * 100).toFixed(0)}%, threshold ${(threshold * 100).toFixed(0)}%). Try rephrasing, selecting more sources, or adding material that covers this topic.`
      : `Retrieved ${hits.length} passage${hits.length === 1 ? '' : 's'} from ${usedSources.length} source${usedSources.length === 1 ? '' : 's'}.`,
    usedSources,
    conflicts: insufficient ? [] : detectConflicts(hits),
    diagnostics: {
      candidateChunks: candidates.length,
      lexicalCandidates: lexicalIds.size,
      vectorCandidates: vector.size,
      deduped: ranked.length - hits.length,
      budgetUsed: evidence.reduce((sum, e) => sum + Math.ceil(e.quote.length / 4), 0),
      budgetLimit: topK,
    },
  };
  return result;
}

function coverageOf(query: string, text: string): number {
  const terms = queryTerms(query);
  if (!terms.length) return 0;
  const lower = text.toLowerCase();
  return terms.filter((t) => lower.includes(t)).length / terms.length;
}

interface Claim {
  sourceId: string;
  sentence: string;
  numbers: number[];
  tokens: Set<string>;
}

export function extractClaims(hits: RetrievalHit[], perHit = 3): Claim[] {
  const claims: Claim[] = [];
  for (const hit of hits) {
    const sentences = hit.chunk.text
      .replace(/\s+/g, ' ')
      .split(/(?<=[.!?])\s+(?=[A-Z0-9“"'(])/)
      .map((s) => s.trim())
      .filter((s) => s.length > 12);
    for (const sentence of sentences.slice(0, perHit)) {
      const numbers = (sentence.match(/\d+(?:[.,]\d+)?%?/g) ?? [])
        .map((raw) => Number(raw.replace('%', '').replace(',', '.')))
        .filter((n) => Number.isFinite(n));
      const tokens = new Set(contentTokens(sentence).filter((t) => !/^\d+$/.test(t)));
      if (!numbers.length || tokens.size < 3) continue;
      claims.push({ sourceId: hit.source.id, sentence, numbers, tokens });
    }
  }
  return claims;
}

/**
 * Conservative contradiction detection: two sentences from different sources
 * that talk about (nearly) the same thing but state different numbers.
 */
export function detectConflicts(hits: RetrievalHit[]): Conflict[] {
  const claims = extractClaims(hits.slice(0, 6));
  const conflicts: Conflict[] = [];
  for (let i = 0; i < claims.length; i += 1) {
    for (let j = i + 1; j < claims.length; j += 1) {
      const a = claims[i];
      const b = claims[j];
      if (a.sourceId === b.sourceId) continue;
      const shared = [...a.tokens].filter((t) => b.tokens.has(t)).length;
      const containment = shared / Math.max(1, Math.min(a.tokens.size, b.tokens.size));
      if (containment < 0.6 || shared < 3) continue;
      const setA = [...new Set(a.numbers)].sort((x, y) => x - y);
      const setB = [...new Set(b.numbers)].sort((x, y) => x - y);
      if (setA.length === setB.length && setA.every((n, idx) => n === setB[idx])) continue;
      if (conflicts.some((c) => c.note.includes(b.sentence.slice(0, 40)))) continue;
      conflicts.push({
        sourceA: a.sourceId,
        sourceB: b.sourceId,
        note: `Sources disagree: “${a.sentence.slice(0, 180)}” vs “${b.sentence.slice(0, 180)}”`,
        numbersA: setA,
        numbersB: setB,
      });
      if (conflicts.length >= 3) return conflicts;
    }
  }
  return conflicts;
}

/** Whether an answer must be flagged as conflicting evidence. */
export function hasConflicts(result: RetrievalResult): boolean {
  return result.conflicts.length > 0;
}

export function sourceLocator(source: Source): string {
  return source.origin.url ?? source.origin.fileName ?? source.title;
}

export { locatorFor };
