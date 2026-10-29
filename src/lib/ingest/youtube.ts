/**
 * YouTube ingestion.
 *
 * Reality: a browser cannot read YouTube's player response or download audio
 * (CORS + ToS). What IS feasible:
 *  - Fetch the public watch page through an optional reader service, which
 *    yields the title and description (real, useful provenance).
 *  - Use caption/transcript files the user supplies (.vtt/.srt/.json3 or
 *    pasted text) — parsed with timestamps.
 *  - Transcribe a user-supplied audio/video file with a configured provider.
 *
 * We never claim to "download YouTube audio" or bypass restrictions.
 */

import type { Segment } from '../types';
import { IngestionError } from './files';
import { mergeCaptionSegments, parseTranscript } from './captions';

export interface YouTubeTarget {
  videoId?: string;
  playlistId?: string;
  url: string;
}

export function parseYouTubeUrl(input: string): YouTubeTarget {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    throw new IngestionError('invalid-input', `“${input}” is not a valid URL.`);
  }
  const host = url.hostname.toLowerCase().replace(/^www\./, '').replace(/^m\./, '');
  const isYouTube = ['youtube.com', 'youtu.be', 'music.youtube.com', 'youtube-nocookie.com'].includes(host);
  if (!isYouTube) throw new IngestionError('invalid-input', 'This is not a YouTube URL.');
  let videoId: string | undefined;
  let playlistId: string | undefined;
  if (host === 'youtu.be') {
    videoId = url.pathname.split('/').filter(Boolean)[0];
  } else if (url.pathname === '/watch') {
    videoId = url.searchParams.get('v') ?? undefined;
    playlistId = url.searchParams.get('list') ?? undefined;
  } else if (url.pathname.startsWith('/shorts/') || url.pathname.startsWith('/embed/') || url.pathname.startsWith('/live/')) {
    videoId = url.pathname.split('/').filter(Boolean)[1];
  } else if (url.pathname === '/playlist') {
    playlistId = url.searchParams.get('list') ?? undefined;
  }
  if (videoId && !/^[A-Za-z0-9_-]{6,20}$/.test(videoId)) {
    throw new IngestionError('invalid-input', `“${videoId}” is not a valid YouTube video id.`);
  }
  if (playlistId && !/^[A-Za-z0-9_-]{10,60}$/.test(playlistId)) {
    throw new IngestionError('invalid-input', 'That playlist id looks malformed.');
  }
  if (!videoId && !playlistId) {
    throw new IngestionError('invalid-input', 'No video or playlist id found in this YouTube URL.');
  }
  const result: YouTubeTarget = { url: url.toString() };
  if (videoId) result.videoId = videoId;
  if (playlistId) result.playlistId = playlistId;
  return result;
}

export function youtubeWatchUrl(videoId: string, atMs?: number): string {
  const base = `https://www.youtube.com/watch?v=${videoId}`;
  return atMs !== undefined && atMs > 0 ? `${base}&t=${Math.floor(atMs / 1000)}s` : base;
}

export function youtubeEmbedUrl(videoId: string, atMs?: number): string {
  const start = atMs !== undefined && atMs > 0 ? `?start=${Math.floor(atMs / 1000)}` : '';
  return `https://www.youtube-nocookie.com/embed/${videoId}${start}`;
}

export interface YouTubePageInfo {
  videoId: string;
  title: string;
  description: string;
  author?: string;
  captions: Segment[];
  via: 'reader' | 'description-only';
  warnings: string[];
}

/**
 * Attempt to read the public watch page through a reader service.
 * Returns description (always useful) and any transcript we can recover.
 */
export async function fetchYouTubePage(
  target: YouTubeTarget,
  options: { readerEndpoint?: string; signal?: AbortSignal; fetchImpl?: typeof fetch } = {},
): Promise<YouTubePageInfo> {
  if (!target.videoId) throw new IngestionError('invalid-input', 'Playlist URLs must be expanded into videos first.');
  const reader = (options.readerEndpoint ?? '').trim();
  if (!reader) {
    throw new IngestionError(
      'blocked',
      'YouTube watch pages cannot be fetched directly from a browser (CORS), and no reader endpoint is configured.',
      'Set a reader endpoint in Settings → Sources, or paste the transcript / upload a .vtt or .srt file for this video.',
    );
  }
  const doFetch = options.fetchImpl ?? fetch;
  const readerUrl = `${reader.replace(/\/+$/, '')}/${youtubeWatchUrl(target.videoId)}`;
  let res: Response;
  try {
    res = await doFetch(readerUrl, {
      headers: { Accept: 'text/plain, text/markdown;q=0.9, */*;q=0.5' },
      signal: options.signal,
    });
  } catch (err) {
    throw new IngestionError(
      'blocked',
      `The reader service could not fetch YouTube: ${err instanceof Error ? err.message : String(err)}.`,
      'Configure a working reader endpoint, or supply the transcript manually.',
    );
  }
  if (!res.ok) {
    throw new IngestionError('network', `The reader service returned HTTP ${res.status} for the YouTube page.`);
  }
  const raw = await res.text();
  const title = /^Title:\s*(.+)$/im.exec(raw)?.[1]?.trim() ?? `YouTube video ${target.videoId}`;
  const author = /^Author:\s*(.+)$/im.exec(raw)?.[1]?.trim();
  const body = stripReaderHeader(raw);
  const description = extractDescription(body);
  const captions = extractTimedTranscript(body);
  const warnings: string[] = [
    'YouTube pages are read through a third-party reader service; the browser never contacts YouTube directly.',
  ];
  if (!captions.length) {
    warnings.push('No transcript was present in the page source. Supply a caption file or transcribe the media with a provider for timestamped quotes.');
  }
  const info: YouTubePageInfo = {
    videoId: target.videoId,
    title,
    description,
    captions: captions.length ? mergeCaptionSegments(captions) : [],
    via: captions.length ? 'reader' : 'description-only',
    warnings,
  };
  if (author) info.author = author;
  return info;
}

function stripReaderHeader(raw: string): string {
  const idx = raw.search(/^Markdown Content:/im);
  if (idx >= 0) return raw.slice(idx).replace(/^Markdown Content:\s*/i, '');
  return raw
    .split('\n')
    .filter((l) => !/^(Title|URL Source|Published Time|Description|Author):/i.test(l))
    .join('\n');
}

function extractDescription(body: string): string {
  const cleaned = body.replace(/\n{3,}/g, '\n\n');
  const transcriptIdx = cleaned.search(/\b(transcript|subtitles\/cc)\b/i);
  const head = transcriptIdx > 200 ? cleaned.slice(0, transcriptIdx) : cleaned;
  return head
    .split('\n')
    .filter((l) => !/^!\[|\[\]\(|\b(share|save|like|subscribe)\b/i.test(l.trim()))
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
    .slice(0, 8000);
}

/** Find `0:00` style timestamp runs in reader output and turn them into segments. */
export function extractTimedTranscript(body: string): Segment[] {
  const lines = body.replace(/\r\n?/g, '\n').split('\n');
  const segments: Segment[] = [];
  const timeRe = /^\s*(?:\[|\*\*)?(\d{1,2}:\d{2}(?::\d{2})?)(?:\]|\*\*)?\s*(.*)$/;
  let pending: { start: number; text: string } | null = null;
  for (const line of lines) {
    const match = timeRe.exec(line);
    if (match) {
      const startMs = clockToMs(match[1]);
      const text = match[2].replace(/^[-–:]\s*/, '').trim();
      if (pending) {
        segments.push({
          index: segments.length,
          text: pending.text || '(silence)',
          startMs: pending.start,
          endMs: Math.max(pending.start + 500, startMs),
          startLine: 0,
          endLine: 0,
        });
      }
      pending = { start: startMs, text };
      continue;
    }
    if (pending && line.trim() && !/^#|^\[|^!\[/.test(line.trim())) {
      pending.text = `${pending.text} ${line.trim()}`.trim();
    }
  }
  if (pending) {
    segments.push({
      index: segments.length,
      text: pending.text || '(silence)',
      startMs: pending.start,
      endMs: pending.start + 4000,
      startLine: 0,
      endLine: 0,
    });
  }
  let line = 1;
  for (const segment of segments) {
    segment.startLine = line;
    line += 1;
    segment.endLine = line - 1;
  }
  return segments;
}

export function clockToMs(clock: string): number {
  const parts = clock.split(':').map(Number);
  if (parts.length === 3) return ((parts[0] * 3600 + parts[1] * 60 + parts[2]) * 1000) | 0;
  if (parts.length === 2) return ((parts[0] * 60 + parts[1]) * 1000) | 0;
  return 0;
}

/** Import from a caption file or pasted transcript for a given video URL. */
export function transcriptFromCaptionFile(content: string, fileName: string): { text: string; segments: Segment[]; format: string } {
  const parsed = parseTranscript(content, fileName);
  if (!parsed.segments.length) {
    throw new IngestionError('empty', 'No transcript text was found in that file.');
  }
  return { text: parsed.text, segments: parsed.segments, format: parsed.format };
}

/** List video ids on a playlist page through the reader service (bounded). */
export async function listPlaylistVideos(
  target: YouTubeTarget,
  options: { readerEndpoint?: string; signal?: AbortSignal; fetchImpl?: typeof fetch; max?: number } = {},
): Promise<string[]> {
  if (!target.playlistId) throw new IngestionError('invalid-input', 'This URL has no playlist id.');
  const reader = (options.readerEndpoint ?? '').trim();
  if (!reader) {
    throw new IngestionError('blocked', 'Playlist pages need a reader endpoint (browser requests to YouTube are CORS-blocked).');
  }
  const doFetch = options.fetchImpl ?? fetch;
  const res = await doFetch(`${reader.replace(/\/+$/, '')}/${target.url}`, {
    headers: { Accept: 'text/plain' },
    signal: options.signal,
  });
  if (!res.ok) throw new IngestionError('network', `The reader service returned HTTP ${res.status} for the playlist.`);
  const text = await res.text();
  const ids = [...text.matchAll(/[?&]v=([A-Za-z0-9_-]{6,20})/g)].map((m) => m[1]);
  return [...new Set(ids)].slice(0, options.max ?? 10);
}
