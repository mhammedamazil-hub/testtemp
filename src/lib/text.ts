/**
 * Deterministic text utilities: normalization, tokenization, chunking.
 *
 * IMPORTANT: `chunkText` is mirrored 1:1 by the Rust implementation in
 * `core/src/lib.rs`. Both are validated against the same fixture file
 * (`core/tests/fixtures/chunking.json`). Change one, change the other.
 */

import type { Chunk, Segment } from './types';

export interface ChunkOptions {
  chunkSize: number;
  chunkOverlap: number;
}

export interface RawChunk {
  text: string;
  startLine: number;
  endLine: number;
  page?: number;
  startMs?: number;
  endMs?: number;
  section?: string;
  tokens: number;
}

export function normalizeText(input: string): string {
  return input.replace(/\r\n?/g, '\n').replace(/\u0000/g, '').replace(/[ \t]+$/gm, '');
}

const STOPWORDS = new Set(
  `a an and are as at be but by for from has have how i if in into is it its of on or our that the their then there these they this to was were what when where which who why will with you your about over than them we can may not no do does did`.split(
    /\s+/,
  ),
);

export interface Token {
  raw: string;
}

/** Lowercased alphanumeric tokens; keeps numbers, splits snake_case and camelCase. */
export function tokenize(text: string): string[] {
  const spaced = text
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_/\\-]+/g, ' ')
    .toLowerCase();
  const matches = spaced.match(/[a-z0-9]+/g);
  return matches ? matches : [];
}

export function contentTokens(text: string): string[] {
  return tokenize(text).filter((t) => t.length > 1 && !STOPWORDS.has(t));
}

export function isStopword(token: string): boolean {
  return STOPWORDS.has(token);
}

interface Unit {
  text: string;
  line: number;
}

/**
 * Split a text into line units. Lines longer than `limit` are hard-split into
 * consecutive pieces that all keep the same 1-based line number so citations
 * never point at a line that does not contain the quote.
 */
export function toUnits(text: string, limit: number): Unit[] {
  const units: Unit[] = [];
  const lines = normalizeText(text).split('\n');
  const safeLimit = Math.max(40, limit);
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.length <= safeLimit) {
      units.push({ text: line, line: i + 1 });
      continue;
    }
    for (let offset = 0; offset < line.length; offset += safeLimit) {
      units.push({ text: line.slice(offset, offset + safeLimit), line: i + 1 });
    }
  }
  return units;
}

function segmentFor(segments: Segment[], startLine: number, endLine: number): Segment | undefined {
  return segments.find((s) => s.endLine >= startLine && s.startLine <= endLine);
}

/**
 * Greedy line-aware chunker with overlap.
 *
 * - Never emits an empty chunk.
 * - Chunks respect `chunkSize` characters where possible.
 * - Consecutive chunks overlap by whole trailing lines up to `chunkOverlap`
 *   characters (overlap is skipped when it would duplicate the whole chunk).
 */
export function chunkText(text: string, opts: ChunkOptions, segments: Segment[] = []): RawChunk[] {
  const chunkSize = Math.max(80, Math.floor(opts.chunkSize) || 900);
  const overlap = Math.max(0, Math.min(Math.floor(opts.chunkOverlap) || 0, Math.floor(chunkSize / 2)));
  const units = toUnits(text, chunkSize);
  const chunks: RawChunk[] = [];
  if (!units.length) return chunks;

  let index = 0;
  while (index < units.length) {
    const group: Unit[] = [];
    let size = 0;
    let cursor = index;
    while (cursor < units.length) {
      const addition = units[cursor].text.length + (group.length ? 1 : 0);
      if (group.length && size + addition > chunkSize) break;
      group.push(units[cursor]);
      size += addition;
      cursor += 1;
      if (size >= chunkSize) break;
    }
    if (!group.length) {
      group.push(units[index]);
      cursor = index + 1;
    }
    const body = group.map((u) => u.text).join('\n').trim();
    const startLine = group[0].line;
    const endLine = group[group.length - 1].line;
    if (body.length) {
      const seg = segmentFor(segments, startLine, endLine);
      const chunk: RawChunk = {
        text: body,
        startLine,
        endLine,
        tokens: Math.max(1, Math.ceil(body.length / 4)),
      };
      if (seg?.page !== undefined) chunk.page = seg.page;
      if (seg?.section) chunk.section = seg.section;
      if (seg?.startMs !== undefined && seg?.endMs !== undefined) {
        chunk.startMs = seg.startMs;
        chunk.endMs = seg.endMs;
      }
      chunks.push(chunk);
    }
    if (cursor <= index) {
      index += 1;
      continue;
    }
    // Everything has been consumed: no overlap-only tail chunk may be emitted.
    if (cursor >= units.length) break;
    // Compute overlap by walking backwards through already-consumed units.
    let back = cursor;
    let overlapChars = 0;
    while (back - 1 > index && overlapChars + units[back - 1].text.length + 1 <= overlap) {
      back -= 1;
      overlapChars += units[back].text.length + 1;
    }
    index = overlapChars > 0 ? back : cursor;
  }
  return chunks;
}

export function buildChunks(sourceId: string, notebookId: string, text: string, opts: ChunkOptions, segments: Segment[] = []): Chunk[] {
  const raw = chunkText(text, opts, segments);
  return raw.map((rc, ordinal) => ({
    id: `${sourceId}#${ordinal}`,
    notebookId,
    sourceId,
    ordinal,
    text: rc.text,
    startLine: rc.startLine,
    endLine: rc.endLine,
    ...(rc.page !== undefined ? { page: rc.page } : {}),
    ...(rc.startMs !== undefined ? { startMs: rc.startMs, endMs: rc.endMs } : {}),
    ...(rc.section ? { section: rc.section } : {}),
    tokens: rc.tokens,
  }));
}

/** Human readable citation locator, e.g. "p. 4", "lines 12–18", "12:04–12:41". */
export function locatorFor(input: {
  page?: number;
  startLine?: number;
  endLine?: number;
  startMs?: number;
  endMs?: number;
  section?: string;
}): string {
  const parts: string[] = [];
  if (input.section) parts.push(input.section);
  if (input.page !== undefined) parts.push(`p. ${input.page}`);
  if (input.startMs !== undefined) parts.push(formatRange(input.startMs, input.endMs ?? input.startMs, formatClock));
  if (input.startLine !== undefined) {
    const end = input.endLine !== undefined && input.endLine !== input.startLine ? `–${input.endLine}` : '';
    parts.push(`lines ${input.startLine}${end}`);
  }
  return parts.length ? parts.join(' · ') : 'excerpt';
}

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h ? String(m).padStart(2, '0') : String(m);
  return `${h ? `${h}:` : ''}${mm}:${String(s).padStart(2, '0')}`;
}

function formatRange(start: number, end: number, fmt: (ms: number) => string): string {
  const a = fmt(start);
  const b = fmt(end);
  return a === b ? a : `${a}–${b}`;
}

export function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  const slice = text.slice(0, Math.max(0, max - 1));
  const boundary = Math.max(slice.lastIndexOf(' '), slice.lastIndexOf('\n'));
  return `${slice.slice(0, boundary > max * 0.6 ? boundary : slice.length).trimEnd()}…`;
}

export function countWords(text: string): number {
  const tokens = tokenize(text);
  return tokens.length;
}

/** Stable, dependency-free 53-bit string hash (cyrb53). */
export function hashString(input: string, seed = 0): string {
  let h1 = 0xdeadbeef ^ seed;
  let h2 = 0x41c6ce57 ^ seed;
  for (let i = 0; i < input.length; i += 1) {
    const ch = input.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}

export function approxTokenCount(text: string): number {
  return Math.max(1, Math.ceil(text.length / 4));
}
