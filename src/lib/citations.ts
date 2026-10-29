/**
 * Citation handling. Citations are the contract between an answer and the
 * evidence: a marker in the answer text must resolve to a real chunk, and the
 * quoted text must actually occur in that chunk (after whitespace folding).
 */

import type { Citation, Evidence, RetrievalHit } from './types';
import { locatorFor, truncate } from './text';

export const CITATION_PATTERN = /\[(\d{1,3})\]/g;

export function quoteFrom(text: string, max = 320): string {
  const clean = text.replace(/\s+/g, ' ').trim();
  if (clean.length <= max) return clean;
  // Prefer a sentence boundary inside the excerpt.
  const slice = clean.slice(0, max);
  const stop = Math.max(slice.lastIndexOf('. '), slice.lastIndexOf('! '), slice.lastIndexOf('? '));
  if (stop > max * 0.4) return slice.slice(0, stop + 1).trim();
  return `${slice.trimEnd()}…`;
}

export function evidenceFromHits(hits: RetrievalHit[], limit = 8): Evidence[] {
  return hits.slice(0, limit).map((hit, i) => ({
    marker: i + 1,
    chunkId: hit.chunk.id,
    sourceId: hit.source.id,
    sourceTitle: hit.source.title,
    quote: quoteFrom(hit.chunk.text),
    locator: hit.locator,
    score: Number(hit.score.toFixed(4)),
  }));
}

export function citationFromEvidence(item: Evidence): Citation {
  return {
    marker: item.marker,
    chunkId: item.chunkId,
    sourceId: item.sourceId,
    sourceTitle: item.sourceTitle,
    quote: item.quote,
    locator: item.locator,
    score: item.score,
  };
}

export function locatorForHit(hit: Pick<RetrievalHit, 'chunk'>): string {
  return locatorFor(hit.chunk);
}

export interface CitationValidation {
  citedMarkers: number[];
  invalidMarkers: number[];
  hasAnyCitation: boolean;
  /** Answer text with out-of-range markers removed (kept otherwise intact). */
  cleanedText: string;
  citations: Citation[];
  /** Claims in the answer with no citation marker (sentence level, advisory). */
  uncitedSentences: string[];
}

export function normalizeWhitespace(text: string): string {
  return text.replace(/\s+/g, ' ').trim().toLowerCase();
}

/**
 * Validate answer citations against the evidence list.
 * Invalid markers (out of range, or referencing empty evidence) are removed so
 * the UI can never render a dead link.
 */
export function validateAnswerCitations(answer: string, evidence: Evidence[]): CitationValidation {
  const valid = new Map<number, Evidence>();
  for (const item of evidence) valid.set(item.marker, item);
  const cited: number[] = [];
  const invalid: number[] = [];
  const seen = new Set<number>();

  CITATION_PATTERN.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = CITATION_PATTERN.exec(answer))) {
    const marker = Number(match[1]);
    if (!valid.has(marker)) {
      if (!invalid.includes(marker)) invalid.push(marker);
    } else if (!seen.has(marker)) {
      seen.add(marker);
      cited.push(marker);
    }
  }

  const cleanedText = invalid.length
    ? answer.replace(CITATION_PATTERN, (full, raw) => (valid.has(Number(raw)) ? full : ''))
    : answer;

  const sentences = cleanedText
    .split(/(?<=[.!?])\s+(?=[A-Z“"'(])/)
    .map((s) => s.trim())
    .filter((s) => s.length > 40);
  const uncitedSentences = sentences.filter((s) => {
    CITATION_PATTERN.lastIndex = 0;
    return !CITATION_PATTERN.test(s);
  });

  const citations = cited
    .map((marker) => valid.get(marker))
    .filter((e): e is Evidence => Boolean(e))
    .map(citationFromEvidence);

  return {
    citedMarkers: cited,
    invalidMarkers: invalid,
    hasAnyCitation: cited.length > 0,
    cleanedText,
    citations,
    uncitedSentences,
  };
}

/** Verify a stored quote still exists in the chunk text (guards stale backups). */
export function verifyQuote(quote: string, chunkText: string): boolean {
  const cleanQuote = normalizeWhitespace(quote.replace(/…$/, ''));
  const cleanChunk = normalizeWhitespace(chunkText);
  if (!cleanQuote) return false;
  if (cleanChunk.includes(cleanQuote)) return true;
  const head = cleanQuote.slice(0, Math.min(60, Math.floor(cleanQuote.length * 0.6)));
  return head.length > 20 && cleanChunk.includes(head);
}

export function citationLabel(citation: Citation): string {
  return `[${citation.marker}] ${truncate(citation.sourceTitle, 60)} · ${citation.locator}`;
}
