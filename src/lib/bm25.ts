/**
 * BM25 lexical ranking. Mirrored by `bm25_scores` in the Rust core and tested
 * against the same fixture in `core/tests/fixtures/ranking.json`.
 */

import { contentTokens, tokenize } from './text';

export interface Bm25Doc {
  id: string;
  text: string;
}

export interface Bm25Result {
  id: string;
  score: number;
  matchedTerms: string[];
  coverage: number;
}

export interface Bm25Options {
  k1?: number;
  b?: number;
}

export interface Bm25Index {
  docs: { id: string; tokens: string[]; length: number }[];
  df: Map<string, number>;
  avgLength: number;
  k1: number;
  b: number;
}

export function buildBm25Index(docs: Bm25Doc[], options: Bm25Options = {}): Bm25Index {
  const df = new Map<string, number>();
  const indexed = docs.map((doc) => {
    const tokens = tokenize(doc.text);
    for (const term of new Set(tokens)) df.set(term, (df.get(term) ?? 0) + 1);
    return { id: doc.id, tokens, length: tokens.length };
  });
  const total = indexed.reduce((sum, d) => sum + d.length, 0);
  return {
    docs: indexed,
    df,
    avgLength: indexed.length ? total / indexed.length : 0,
    k1: options.k1 ?? 1.5,
    b: options.b ?? 0.75,
  };
}

function idf(index: Bm25Index, term: string): number {
  const n = index.docs.length;
  const df = index.df.get(term) ?? 0;
  return Math.log(1 + (n - df + 0.5) / (df + 0.5));
}

export function queryTerms(query: string, keepStopwords = false): string[] {
  const tokens = keepStopwords ? tokenize(query) : contentTokens(query);
  return [...new Set(tokens.filter((t) => t.length > 1))];
}

export function scoreDoc(index: Bm25Index, docIndex: number, terms: string[]): { score: number; matched: string[] } {
  const doc = index.docs[docIndex];
  const freq = new Map<string, number>();
  for (const token of doc.tokens) freq.set(token, (freq.get(token) ?? 0) + 1);
  let score = 0;
  const matched: string[] = [];
  for (const term of terms) {
    const tf = freq.get(term);
    if (!tf) {
      // Light stemming: allow a shared 5-character prefix so "collaboration"
      // matches "collaborative" without a full stemmer.
      if (term.length >= 6) {
        for (const [token, count] of freq) {
          if (token.length >= 6 && (token.startsWith(term.slice(0, 6)) || term.startsWith(token.slice(0, 6)))) {
            const norm = 1 - index.b + (index.b * doc.length) / (index.avgLength || 1);
            score += idf(index, term) * ((count * (index.k1 + 1)) / (count + index.k1 * norm));
            matched.push(term);
            break;
          }
        }
      }
      continue;
    }
    const norm = 1 - index.b + (index.b * doc.length) / (index.avgLength || 1);
    score += idf(index, term) * ((tf * (index.k1 + 1)) / (tf + index.k1 * norm));
    matched.push(term);
  }
  return { score, matched };
}

export function searchBm25(index: Bm25Index, query: string, limit = 50): Bm25Result[] {
  const terms = queryTerms(query);
  if (!terms.length) return [];
  const results: Bm25Result[] = [];
  for (let i = 0; i < index.docs.length; i += 1) {
    const { score, matched } = scoreDoc(index, i, terms);
    if (score <= 0) continue;
    results.push({
      id: index.docs[i].id,
      score,
      matchedTerms: [...new Set(matched)],
      coverage: matched.length / terms.length,
    });
  }
  return results.sort((a, b) => b.score - a.score || a.id.localeCompare(b.id)).slice(0, limit);
}

/** Bounded relevance used for the insufficient-evidence gate. */
export function relevanceConfidence(bm25Score: number, coverage: number): number {
  const normalized = bm25Score > 0 ? bm25Score / (bm25Score + 4) : 0;
  return 0.65 * Math.max(0, Math.min(1, coverage)) + 0.35 * normalized;
}
