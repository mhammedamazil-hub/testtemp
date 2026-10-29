/**
 * Ingestion orchestration: turns parsed material into `Source` records with
 * stable identity, provenance and an honest extraction status.
 */

import type { Segment, Settings, Source, SourceKind, SourceOrigin } from '../types';
import { hashString, normalizeText } from '../text';
import {
  IngestionError,
  parseDocument,
  type FileLike,
  type ParsedDocument,
} from './files';
import {
  defaultSelection,
  fetchGithubFile,
  githubOrigin,
  listGithubFiles,
  parseGithubUrl,
  type GithubEntry,
  type GithubTarget,
} from './github';
import { fetchWebPage } from './web';
import { fetchYouTubePage, parseYouTubeUrl, transcriptFromCaptionFile, youtubeWatchUrl } from './youtube';
import { fetchInstagramPost, parseInstagramUrl } from './instagram';
import { parseTranscript } from './captions';
import { mediaTypeOf } from './media';

export { IngestionError } from './files';
export * from './files';

export function newId(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return `${prefix}-${crypto.randomUUID()}`;
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export interface SourceInput {
  notebookId: string;
  title: string;
  kind: SourceKind;
  text: string;
  segments?: Segment[];
  status?: 'ready' | 'partial' | 'error';
  statusDetail?: string;
  warnings?: string[];
  origin?: SourceOrigin;
  byteSize?: number;
  id?: string;
}

export function makeSource(input: SourceInput): Source {
  const text = normalizeText(input.text);
  const now = Date.now();
  const id = input.id ?? newId('src');
  const segments: Segment[] =
    input.segments && input.segments.length
      ? input.segments
      : [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }];
  return {
    id,
    notebookId: input.notebookId,
    title: input.title.trim() || 'Untitled source',
    kind: input.kind,
    origin: input.origin ?? {},
    text,
    segments,
    status: input.status ?? 'ready',
    statusDetail: input.statusDetail ?? `Imported ${text.length.toLocaleString()} characters.`,
    warnings: input.warnings ?? [],
    byteSize: input.byteSize ?? text.length,
    checksum: hashString(text),
    createdAt: now,
    updatedAt: now,
  };
}

function fromParsed(parsed: ParsedDocument, notebookId: string, origin: SourceOrigin): Source {
  return makeSource({
    notebookId,
    title: parsed.title,
    kind: parsed.kind,
    text: parsed.text,
    segments: parsed.segments,
    status: parsed.status,
    statusDetail: parsed.statusDetail,
    warnings: parsed.warnings,
    origin: { ...origin, ...(parsed.mime ? { mime: parsed.mime } : {}) },
    byteSize: parsed.byteSize,
  });
}

export async function importFile(file: FileLike, notebookId: string): Promise<Source> {
  const media = mediaTypeOf(file.name);
  if (media) {
    throw new IngestionError(
      'unsupported',
      `${media === 'video' ? 'Video' : 'Audio'} files cannot be read as text directly.`,
      media === 'video'
        ? 'Use the media panel to transcribe audio and sample frames with a configured provider.'
        : 'Use the media panel to transcribe this audio with a configured provider.',
    );
  }
  const parsed = await parseDocument(file);
  return fromParsed(parsed, notebookId, { fileName: file.name, ...(file.type ? { mime: file.type } : {}) });
}

export function importPastedText(
  notebookId: string,
  title: string,
  text: string,
  options: { kind?: SourceKind; url?: string; origin?: SourceOrigin } = {},
): Source {
  if (!text.trim()) throw new IngestionError('empty', 'Pasted content is empty.');
  const segments = [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }];
  return makeSource({
    notebookId,
    title,
    kind: options.kind ?? 'text',
    text,
    segments,
    origin: options.origin ?? (options.url ? { url: options.url } : {}),
    statusDetail: `Pasted content, ${text.length.toLocaleString()} characters.`,
  });
}

/** Import a caption/transcript file, optionally linked to a YouTube video. */
export function importTranscriptFile(
  notebookId: string,
  fileName: string,
  content: string,
  options: { videoUrl?: string; title?: string } = {},
): Source {
  const parsed = transcriptFromCaptionFile(content, fileName);
  const timed = parsed.segments.some((s) => s.startMs !== undefined);
  const videoId = options.videoUrl ? safeVideoId(options.videoUrl) : undefined;
  return makeSource({
    notebookId,
    title: options.title ?? fileName.replace(/\.[a-z0-9]+$/i, ''),
    kind: 'media-transcript',
    text: parsed.text,
    segments: parsed.segments,
    status: 'ready',
    statusDetail: `${timed ? 'Timed transcript' : 'Transcript'} imported (${parsed.segments.length} segments, ${parsed.format.toUpperCase()}).`,
    warnings: timed ? [] : ['This transcript has no timestamps, so citations use line numbers only.'],
    origin: {
      fileName,
      ...(options.videoUrl ? { url: options.videoUrl, externalId: videoId, author: undefined } : {}),
    },
  });
}

function safeVideoId(url: string): string | undefined {
  try {
    return parseYouTubeUrl(url).videoId;
  } catch {
    return undefined;
  }
}

export async function importWebUrl(url: string, notebookId: string, settings: Settings, signal?: AbortSignal): Promise<Source> {
  const page = await fetchWebPage(url, {
    readerEndpoint: settings.readerEndpoint,
    ...(signal ? { signal } : {}),
  });
  return makeSource({
    notebookId,
    title: page.title,
    kind: 'web',
    text: page.text,
    origin: { url: page.url, ...(page.description ? { author: undefined } : {}) },
    status: page.warnings.length > 1 ? 'partial' : 'ready',
    statusDetail: `Fetched public page via ${page.via === 'reader' ? 'reader service' : 'direct request'} (${page.text.length.toLocaleString()} characters).`,
    warnings: page.warnings,
    byteSize: page.text.length,
  });
}

export interface GithubImportResult {
  sources: Source[];
  target: GithubTarget;
  ref: string;
  truncated: boolean;
  warnings: string[];
}

export async function listGithubTarget(url: string, settings: Settings, signal?: AbortSignal) {
  const target = parseGithubUrl(url);
  const listing = await listGithubFiles(target, {
    ...(settings.githubToken ? { token: settings.githubToken } : {}),
    ...(signal ? { signal } : {}),
  });
  const entries = listing.entries;
  return { target, ref: listing.ref, entries, truncated: listing.truncated, suggested: defaultSelection(entries) };
}

export async function importGithubFiles(
  target: GithubTarget,
  entries: GithubEntry[],
  ref: string,
  notebookId: string,
  settings: Settings,
  signal?: AbortSignal,
): Promise<GithubImportResult> {
  const sources: Source[] = [];
  const warnings: string[] = [];
  let totalBytes = 0;
  for (const entry of entries) {
    if (signal?.aborted) break;
    if (!entry.importable) continue;
    if (totalBytes + entry.size > 6 * 1024 * 1024) {
      warnings.push(`Stopped at the 6 MB import budget before ${entry.path}.`);
      break;
    }
    try {
      const file = await fetchGithubFile(target, entry, ref, {
        ...(settings.githubToken ? { token: settings.githubToken } : {}),
        ...(signal ? { signal } : {}),
      });
      totalBytes += file.size;
      const name = entry.path.split('/').pop() ?? entry.path;
      sources.push(
        makeSource({
          notebookId,
          title: entry.path,
          kind: 'github-file',
          text: file.text,
          origin: githubOrigin(target, ref, entry, file.sha),
          status: file.truncated ? 'partial' : 'ready',
          statusDetail: `Imported ${entry.path} from ${target.owner}/${target.repo}@${ref} (${(file.size / 1024).toFixed(1)} KB).`,
          warnings: file.truncated ? [`${name} was truncated at 512 KB for indexing.`] : [],
          byteSize: file.size,
        }),
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      warnings.push(`${entry.path}: ${message}`);
    }
  }
  if (!sources.length) {
    throw new IngestionError(
      'invalid-input',
      warnings[0] ?? 'No importable text files were found in this selection.',
      'Pick individual files, or choose a directory that contains Markdown/source files.',
    );
  }
  return { sources, target, ref, truncated: false, warnings };
}

export async function importYouTube(
  url: string,
  notebookId: string,
  settings: Settings,
  signal?: AbortSignal,
): Promise<Source> {
  const target = parseYouTubeUrl(url);
  if (!target.videoId) {
    throw new IngestionError(
      'invalid-input',
      'This is a playlist URL. Import individual videos, or paste the playlist and import the first few.',
      'Playlist expansion uses the reader service; each video needs its own transcript.',
    );
  }
  const page = await fetchYouTubePage(target, {
    readerEndpoint: settings.readerEndpoint,
    ...(signal ? { signal } : {}),
  });
  const text = page.captions.length
    ? `${page.title}\n\n${page.description}\n\n--- Transcript ---\n${page.captions.map((s) => s.text).join('\n')}`
    : `${page.title}\n\n${page.description || '(no description available)'}`;
  const warnings = [...page.warnings];
  if (!page.captions.length) {
    warnings.push('No transcript was retrieved, so this source contains only public metadata. Add a caption file or transcribe the audio with a provider.');
  }
  return makeSource({
    notebookId,
    title: page.title,
    kind: 'youtube',
    text,
    segments: page.captions.length
      ? page.captions.map((s, i) => ({ ...s, index: i, text: s.text }))
      : undefined,
    origin: {
      url: youtubeWatchUrl(target.videoId),
      externalId: target.videoId,
      ...(page.author ? { author: page.author } : {}),
    },
    status: page.captions.length ? 'ready' : 'partial',
    statusDetail: page.captions.length
      ? `YouTube metadata + ${page.captions.length} timed transcript segments.`
      : 'YouTube metadata only — no captions were available to the reader service.',
    warnings,
  });
}

export async function importInstagram(url: string, notebookId: string, settings: Settings, signal?: AbortSignal): Promise<Source> {
  const target = parseInstagramUrl(url);
  const post = await fetchInstagramPost(target, {
    readerEndpoint: settings.readerEndpoint,
    ...(signal ? { signal } : {}),
  });
  return makeSource({
    notebookId,
    title: post.title,
    kind: 'instagram',
    text: post.text,
    origin: {
      url: target.canonicalUrl,
      externalId: target.shortcode,
      ...(post.author ? { author: post.author } : {}),
    },
    status: post.caption ? 'partial' : 'partial',
    statusDetail: post.caption
      ? `Public ${target.kind} metadata and caption extracted (${post.caption.length} characters).`
      : 'Only post metadata was exposed; no caption was available.',
    warnings: post.warnings,
  });
}

/** Import a pasted plain-text transcript with timestamps for a media URL. */
export function importPastedTranscript(
  notebookId: string,
  title: string,
  content: string,
  mediaUrl?: string,
): Source {
  const parsed = parseTranscript(content);
  return makeSource({
    notebookId,
    title,
    kind: 'media-transcript',
    text: parsed.text,
    segments: parsed.segments,
    origin: mediaUrl ? { url: mediaUrl, externalId: safeVideoId(mediaUrl) } : {},
    status: 'ready',
    statusDetail: `Pasted transcript parsed (${parsed.segments.length} segments, ${parsed.format.toUpperCase()}).`,
    warnings: parsed.format === 'text' ? ['No timestamps were found; citations use line numbers.'] : [],
  });
}

export function detectImportRoute(rawUrl: string): 'github' | 'youtube' | 'instagram' | 'web' | 'invalid' {
  let url: URL;
  try {
    url = new URL(rawUrl.trim());
  } catch {
    return 'invalid';
  }
  const host = url.hostname.toLowerCase().replace(/^www\./, '');
  if (host === 'github.com' || host === 'raw.githubusercontent.com') return 'github';
  if (host === 'youtu.be' || host.endsWith('youtube.com')) return 'youtube';
  if (host === 'instagram.com') return 'instagram';
  if (['http:', 'https:'].includes(url.protocol)) return 'web';
  return 'invalid';
}
