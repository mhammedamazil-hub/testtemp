/**
 * Instagram ingestion (public metadata best-effort).
 *
 * Honest limits: Instagram requires authentication and blocks unauthenticated
 * browser requests; posts are frequently login-gated. We do NOT bypass auth,
 * anti-bot systems or DRM. What works:
 *  - Public post/Reel URL parsing and provenance.
 *  - Best-effort page read through an optional reader service (caption often
 *    present in the page metadata when the post is public).
 *  - User-provided caption/transcript and uploaded media transcription.
 */

import { IngestionError } from './files';

export interface InstagramTarget {
  shortcode: string;
  kind: 'post' | 'reel' | 'tv';
  url: string;
  canonicalUrl: string;
}

export function parseInstagramUrl(input: string): InstagramTarget {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    throw new IngestionError('invalid-input', `“${input}” is not a valid URL.`);
  }
  const host = url.hostname.toLowerCase().replace(/^www\./, '');
  if (host !== 'instagram.com') throw new IngestionError('invalid-input', 'This is not an Instagram URL.');
  const parts = url.pathname.split('/').filter(Boolean);
  const [marker, shortcode] = parts;
  if (!marker || !shortcode) {
    throw new IngestionError('invalid-input', 'Instagram URLs must point to a post or Reel, e.g. https://www.instagram.com/p/SHORTCODE/.');
  }
  if (!['p', 'reel', 'reels', 'tv'].includes(marker)) {
    throw new IngestionError('invalid-input', `Instagram /${marker}/ pages are not supported. Use /p/, /reel/ or /tv/.`);
  }
  if (!/^[A-Za-z0-9_-]{5,30}$/.test(shortcode)) {
    throw new IngestionError('invalid-input', `“${shortcode}” is not a valid Instagram shortcode.`);
  }
  const kind = marker === 'reels' || marker === 'reel' ? 'reel' : marker === 'tv' ? 'tv' : 'post';
  return {
    shortcode,
    kind,
    url: url.toString(),
    canonicalUrl: `https://www.instagram.com/${kind === 'reel' ? 'reel' : kind}/${shortcode}/`,
  };
}

export interface InstagramPost {
  shortcode: string;
  caption: string;
  author?: string;
  title: string;
  text: string;
  via: 'reader' | 'page-metadata';
  warnings: string[];
}

export async function fetchInstagramPost(
  target: InstagramTarget,
  options: { readerEndpoint?: string; signal?: AbortSignal; fetchImpl?: typeof fetch } = {},
): Promise<InstagramPost> {
  const reader = (options.readerEndpoint ?? '').trim();
  if (!reader) {
    throw new IngestionError(
      'blocked',
      'Instagram blocks direct browser requests (CORS + login wall), and no reader endpoint is configured.',
      'Add a reader endpoint in Settings, or paste the caption / upload a downloaded video for transcription.',
    );
  }
  const doFetch = options.fetchImpl ?? fetch;
  let res: Response;
  try {
    res = await doFetch(`${reader.replace(/\/+$/, '')}/${target.canonicalUrl}`, {
      headers: { Accept: 'text/plain, text/markdown;q=0.9, */*;q=0.5' },
      signal: options.signal,
    });
  } catch (err) {
    throw new IngestionError('blocked', `The reader service failed for Instagram: ${err instanceof Error ? err.message : String(err)}.`);
  }
  if (!res.ok) {
    if (res.status === 401 || res.status === 403 || res.status === 429) {
      throw new IngestionError(
        'blocked',
        `Instagram/reader refused the request (HTTP ${res.status}). Instagram often requires login for automated reads.`,
        'Paste the caption manually or upload a downloaded video.',
      );
    }
    throw new IngestionError('network', `The reader service returned HTTP ${res.status} for the post.`);
  }
  const raw = await res.text();
  const title = /^Title:\s*(.+)$/im.exec(raw)?.[1]?.trim() ?? `Instagram ${target.kind} ${target.shortcode}`;
  const authorMatch = /\(@([A-Za-z0-9._]+)\)/.exec(raw) ?? /instagram\.com\/([A-Za-z0-9._]+)\//.exec(raw);
  const description = /^Description:\s*(.+)$/im.exec(raw)?.[1]?.trim();
  const body = raw
    .replace(/^Title:.*$/im, '')
    .replace(/^URL Source:.*$/im, '')
    .replace(/^Description:.*$/im, '')
    .replace(/^Markdown Content:/im, '')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/^#+\s*/gm, '')
    .trim();
  const caption = pickCaption(description ?? '', body);
  const warnings = [
    'Instagram is read through a third-party reader service. Only public data that the page exposes without login can be retrieved.',
    'No authentication, anti-bot or DRM circumvention is performed.',
  ];
  if (!caption) {
    warnings.push('No caption text was exposed. Paste the caption or upload the downloaded video for transcription.');
  }
  const post: InstagramPost = {
    shortcode: target.shortcode,
    caption,
    title,
    text: caption ? `Instagram ${target.kind} by ${authorMatch?.[1] ?? 'unknown author'}\n\n${caption}` : `Instagram ${target.kind} ${target.shortcode} (no caption available)`,
    via: caption ? 'reader' : 'page-metadata',
    warnings,
  };
  if (authorMatch) post.author = authorMatch[1].replace(/^@/, '');
  return post;
}

function pickCaption(description: string, body: string): string {
  const candidates = [description, ...body.split(/\n{2,}/)]
    .map((c) => c.trim())
    .filter((c) => c.length >= 20 && !/^(log ?in|sign ?up|cookie|privacy|terms|about|help|press|api|jobs)/i.test(c));
  return (candidates[0] ?? '').slice(0, 8000);
}
