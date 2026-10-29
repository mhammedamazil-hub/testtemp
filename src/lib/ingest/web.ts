/**
 * Public web page ingestion.
 *
 * Browsers cannot bypass CORS. Strategy:
 *  1. Optional reader service (r.jina.ai by default, configurable or blank).
 *     The reader fetches the page server-side and returns clean markdown with
 *     CORS headers, so it works for most public pages.
 *  2. Direct fetch, only for servers that send CORS headers.
 *  3. Paste / upload fallbacks (handled by the UI).
 * Failures always state the concrete reason. No claim of "arbitrary URL works".
 */

import { IngestionError } from './files';
import { decodeEntities, extractMeta, htmlToText } from './html';

export interface WebPage {
  url: string;
  title: string;
  text: string;
  description?: string;
  siteName?: string;
  via: 'reader' | 'direct';
  warnings: string[];
}

const MAX_PAGE_BYTES = 5 * 1024 * 1024;

export function assertPublicUrl(input: string): URL {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    throw new IngestionError('invalid-input', `“${input}” is not a valid URL.`);
  }
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new IngestionError('invalid-input', 'Only http:// and https:// URLs can be imported.');
  }
  const host = url.hostname.toLowerCase();
  if (['localhost', '127.0.0.1', '0.0.0.0', '::1', '[::1]'].includes(host) || /^(10\.|192\.168\.|169\.254\.|172\.(1[6-9]|2\d|3[01])\.)/.test(host)) {
    throw new IngestionError('invalid-input', 'Local and private-network URLs are not imported, to avoid exposing internal services.');
  }
  return url;
}

export interface WebFetchOptions {
  readerEndpoint?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
}

export async function fetchWebPage(rawUrl: string, options: WebFetchOptions = {}): Promise<WebPage> {
  const url = assertPublicUrl(rawUrl);
  const doFetch = options.fetchImpl ?? fetch;
  const warnings: string[] = [];
  const reader = (options.readerEndpoint ?? '').trim();

  if (reader) {
    try {
      const readerUrl = `${reader.replace(/\/+$/, '')}/${url.toString()}`;
      const res = await withTimeoutFetch(doFetch, readerUrl, {
        headers: { Accept: 'text/plain, text/markdown;q=0.9, */*;q=0.5' },
        signal: options.signal,
      }, options.timeoutMs ?? 45000);
      if (res.ok) {
        const raw = await res.text();
        const parsed = parseReaderOutput(raw, url.toString());
        if (parsed.text.length > 20) {
          warnings.push(`Extracted through the reader service (${new URL(reader).hostname}); the page itself was not fetched by your browser.`);
          return { url: url.toString(), via: 'reader', warnings, ...parsed };
        }
      } else {
        warnings.push(`The reader service returned HTTP ${res.status}. A direct fetch was attempted instead.`);
      }
    } catch (err) {
      warnings.push(`Reader service unavailable (${err instanceof Error ? err.message : String(err)}). Trying a direct fetch.`);
    }
  }

  let res: Response;
  try {
    res = await withTimeoutFetch(doFetch, url.toString(), {
      headers: { Accept: 'text/html,application/xhtml+xml,text/plain;q=0.8' },
      signal: options.signal,
    }, options.timeoutMs ?? 30000);
  } catch (err) {
    throw new IngestionError(
      'blocked',
      `The browser could not fetch ${url.hostname}: ${err instanceof Error ? err.message : String(err)}. This is almost always a CORS or a network block.`,
      'Configure a reader endpoint in Settings, upload the saved HTML, or paste the text.',
    );
  }
  if (!res.ok) {
    throw new IngestionError('network', `${url.hostname} responded with HTTP ${res.status}.`, 'Some sites block automated requests.');
  }
  const contentType = res.headers.get('content-type') ?? '';
  if (!/text|html|json|xml/i.test(contentType)) {
    throw new IngestionError('unsupported', `The URL returned ${contentType || 'an unknown content type'}, not a readable page.`);
  }
  const length = Number(res.headers.get('content-length') ?? '0');
  if (length > MAX_PAGE_BYTES) {
    throw new IngestionError('too-large', `The page is ${(length / 1024 / 1024).toFixed(1)} MB, above the 5 MB limit.`);
  }
  const raw = await res.text();
  if (raw.length > MAX_PAGE_BYTES) throw new IngestionError('too-large', 'The page exceeded the 5 MB limit.');
  const meta = extractMeta(raw);
  const extracted = htmlToText(raw, { baseUrl: url.toString() });
  if (!extracted.text.trim()) {
    throw new IngestionError('empty', 'The page had no readable text (it may require JavaScript or a login).', 'Try a reader endpoint or paste the content.');
  }
  const page: WebPage = {
    url: url.toString(),
    title: extracted.title || meta['og:title'] || url.hostname,
    text: extracted.text,
    via: 'direct',
    warnings: [...warnings, 'Fetched directly by your browser. Sites without CORS headers will fail — that is a browser security rule, not a bug.'],
  };
  const description = extracted.description ?? meta['og:description'] ?? meta['description'];
  if (description) page.description = decodeEntities(description);
  if (meta['og:site_name']) page.siteName = meta['og:site_name'];
  return page;
}

function parseReaderOutput(raw: string, url: string): { title: string; text: string; description?: string } {
  const trimmed = raw.trim();
  // r.jina.ai returns a "Title:" / "URL Source:" header block followed by markdown.
  const lines = trimmed.split('\n');
  let title = '';
  let description: string | undefined;
  const bodyStart: number[] = [];
  lines.forEach((line, index) => {
    const titleMatch = /^Title:\s*(.+)$/i.exec(line);
    if (titleMatch && index < 6) title = titleMatch[1].trim();
    const descMatch = /^Description:\s*(.+)$/i.exec(line);
    if (descMatch && index < 8) description = descMatch[1].trim();
    if (/^Markdown Content:/i.test(line)) bodyStart.push(index + 1);
  });
  let body = bodyStart.length ? lines.slice(bodyStart[0]).join('\n') : trimmed;
  if (!bodyStart.length && /^Title:/im.test(trimmed)) {
    body = lines.filter((l) => !/^(Title|URL Source|Published Time|Description|Markdown Content):/i.test(l)).join('\n');
  }
  body = body.replace(/!\[[^\]]*\]\([^)]*\)/g, '').replace(/\n{3,}/g, '\n\n').trim();
  return {
    title: title || new URL(url).hostname,
    text: body,
    ...(description ? { description } : {}),
  };
}

async function withTimeoutFetch(
  doFetch: typeof fetch,
  url: string,
  init: RequestInit,
  timeoutMs: number,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new DOMException('timeout', 'TimeoutError')), timeoutMs);
  const outer = init.signal;
  const onAbort = () => controller.abort(outer?.reason);
  if (outer) {
    if (outer.aborted) controller.abort(outer.reason);
    else outer.addEventListener('abort', onAbort, { once: true });
  }
  try {
    return await doFetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timer);
    outer?.removeEventListener('abort', onAbort);
  }
}
