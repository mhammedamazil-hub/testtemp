/**
 * Caption/transcript parsing (WebVTT, SRT, YouTube JSON3, plain text).
 * Pure functions with no DOM dependency so they run in tests and workers.
 */

import type { Segment } from '../types';

export type CaptionFormat = 'vtt' | 'srt' | 'json3' | 'text';

export function detectCaptionFormat(content: string, fileName = ''): CaptionFormat {
  const lower = fileName.toLowerCase();
  if (lower.endsWith('.vtt')) return 'vtt';
  if (lower.endsWith('.srt')) return 'srt';
  if (lower.endsWith('.json') || lower.endsWith('.json3')) return 'json3';
  const head = content.trimStart().slice(0, 200);
  if (head.startsWith('WEBVTT')) return 'vtt';
  if (/^\d+\s*\n\d{2}:\d{2}:\d{2}[,.]\d{3}\s*-->/.test(head)) return 'srt';
  if (head.startsWith('{') && head.includes('"events"')) return 'json3';
  return 'text';
}

function parseTimestamp(value: string): number {
  const parts = value.trim().replace(',', '.').split(':').map(Number);
  if (parts.length === 3) return ((parts[0] * 3600 + parts[1] * 60 + parts[2]) * 1000) | 0;
  if (parts.length === 2) return ((parts[0] * 60 + parts[1]) * 1000) | 0;
  return 0;
}

/** Parse VTT or SRT into timestamped segments. */
export function parseTimedCaptions(content: string): Segment[] {
  const blocks = content.replace(/\r\n?/g, '\n').split(/\n{2,}/);
  const segments: Segment[] = [];
  for (const block of blocks) {
    const lines = block.split('\n').filter((l) => l.trim() && !/^WEBVTT/i.test(l) && !/^NOTE\b/i.test(l));
    if (!lines.length) continue;
    const timeIndex = lines.findIndex((l) => l.includes('-->'));
    if (timeIndex === -1) continue;
    const [startRaw, endRaw] = lines[timeIndex].split('-->');
    const text = lines
      .slice(timeIndex + 1)
      .join(' ')
      .replace(/<[^>]+>/g, '')
      .replace(/\s+/g, ' ')
      .trim();
    if (!text) continue;
    const startMs = parseTimestamp(startRaw.replace(/(\d+)\s*$/, '$1'));
    const endMs = parseTimestamp((endRaw ?? '').split(' ')[1] ?? endRaw ?? '');
    segments.push({
      index: segments.length,
      text,
      startLine: 0,
      endLine: 0,
      startMs,
      endMs: endMs || startMs + 2000,
    });
  }
  // Fill line numbers so citations always have a locator.
  let line = 1;
  for (const seg of segments) {
    seg.startLine = line;
    line += Math.max(1, seg.text.split('\n').length);
    seg.endLine = line - 1;
    line += 1;
  }
  return segments;
}

interface Json3 {
  events?: { tStartMs?: number; dDurationMs?: number; segs?: { utf8?: string }[] }[];
}

export function parseJson3Captions(content: string): Segment[] {
  let json: Json3;
  try {
    json = JSON.parse(content) as Json3;
  } catch {
    return [];
  }
  const segments: Segment[] = [];
  let line = 1;
  for (const event of json.events ?? []) {
    const text = (event.segs ?? []).map((s) => s.utf8 ?? '').join('').replace(/\s+/g, ' ').trim();
    if (!text || text === '\n') continue;
    const startMs = event.tStartMs ?? 0;
    const endMs = startMs + (event.dDurationMs ?? 2000);
    segments.push({
      index: segments.length,
      text,
      startMs,
      endMs,
      startLine: line,
      endLine: line,
    });
    line += 1;
  }
  return segments;
}

/**
 * Parse a transcript of any supported shape. Plain text becomes one segment
 * per paragraph so retrieval still has line-level locators.
 */
export function parseTranscript(content: string, fileName = ''): { segments: Segment[]; format: CaptionFormat; text: string } {
  const format = detectCaptionFormat(content, fileName);
  let segments: Segment[] = [];
  if (format === 'vtt' || format === 'srt') segments = parseTimedCaptions(content);
  else if (format === 'json3') segments = parseJson3Captions(content);
  if (!segments.length) {
    const lines = content.replace(/\r\n?/g, '\n').split('\n');
    segments = [];
    let buffer: string[] = [];
    let start = 1;
    const flush = (end: number) => {
      const text = buffer.join('\n').trim();
      if (text) {
        segments.push({ index: segments.length, text, startLine: start, endLine: end });
      }
      buffer = [];
    };
    lines.forEach((line, i) => {
      if (!buffer.length) start = i + 1;
      buffer.push(line);
      if (!line.trim() || buffer.length > 40) flush(i + 1);
    });
    flush(lines.length);
    return { segments, format: 'text', text: content };
  }
  return { segments, format, text: segments.map((s) => s.text).join('\n') };
}

/** Merge caption segments into readable paragraphs while keeping timestamps. */
export function mergeCaptionSegments(segments: Segment[], maxChars = 420): Segment[] {
  const merged: Segment[] = [];
  let current: Segment | null = null;
  for (const segment of segments) {
    if (!current) {
      current = { ...segment, index: merged.length };
      continue;
    }
    const combined: string = `${current.text} ${segment.text}`;
    if (combined.length <= maxChars) {
      current = {
        ...current,
        text: combined,
        endLine: segment.endLine,
        endMs: segment.endMs,
      };
    } else {
      merged.push(current);
      current = { ...segment, index: merged.length };
    }
  }
  if (current) merged.push(current);
  return merged;
}
