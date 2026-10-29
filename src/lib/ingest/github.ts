/**
 * GitHub ingestion via the public REST API + raw.githubusercontent.com.
 *
 * - One API call lists a repo/directory (git trees), then raw fetches (which
 *   are not rate limited) download contents.
 * - Without a token GitHub allows ~60 API requests/hour per IP; with a
 *   personal access token it is ~5000/hour. We surface that honestly.
 * - Binary and oversized files are skipped, never downloaded wastefully.
 */

import type { Segment, SourceOrigin } from '../types';
import { IngestionError, extensionOf, MAX_TEXT_CHARS } from './files';

export interface GithubTarget {
  owner: string;
  repo: string;
  ref?: string;
  path?: string;
  type: 'repo' | 'dir' | 'file';
  canonicalUrl: string;
}

export interface GithubEntry {
  path: string;
  size: number;
  sha?: string;
  htmlUrl: string;
  rawUrl: string;
  importable: boolean;
  reason?: string;
}

export interface GithubFileContent {
  path: string;
  text: string;
  sha?: string;
  htmlUrl: string;
  rawUrl: string;
  size: number;
  truncated: boolean;
}

const BINARY_EXTENSIONS = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'ico', 'bmp', 'tiff', 'psd', 'pdf', 'zip', 'gz', 'tgz', 'bz2', 'xz', '7z',
  'rar', 'tar', 'mp3', 'mp4', 'mov', 'avi', 'mkv', 'webm', 'wav', 'flac', 'ogg', 'woff', 'woff2', 'ttf', 'otf',
  'eot', 'so', 'dylib', 'dll', 'exe', 'bin', 'class', 'jar', 'wasm', 'pyc', 'o', 'a', 'lib', 'node', 'lock',
]);

export const MAX_GITHUB_FILE_BYTES = 512 * 1024;
export const MAX_GITHUB_FILES = 40;
export const MAX_GITHUB_TOTAL_BYTES = 6 * 1024 * 1024;

export function parseGithubUrl(input: string): GithubTarget {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    throw new IngestionError('invalid-input', `“${input}” is not a valid URL.`);
  }
  const host = url.hostname.toLowerCase();
  if (host === 'raw.githubusercontent.com') {
    const parts = url.pathname.split('/').filter(Boolean);
    if (parts.length < 4) throw new IngestionError('invalid-input', 'Raw GitHub URLs need /owner/repo/ref/path.');
    const [owner, repo, ref, ...rest] = parts;
    return {
      owner,
      repo,
      ref,
      path: decodeURIComponent(rest.join('/')),
      type: 'file',
      canonicalUrl: `https://github.com/${owner}/${repo}/blob/${ref}/${rest.join('/')}`,
    };
  }
  if (host !== 'github.com' && host !== 'www.github.com') {
    throw new IngestionError('invalid-input', 'Only github.com and raw.githubusercontent.com URLs are supported.');
  }
  const parts = url.pathname.split('/').filter(Boolean).map(decodeURIComponent);
  if (parts.length < 2) {
    throw new IngestionError('invalid-input', 'A GitHub URL must include an owner and a repository, e.g. https://github.com/owner/repo.');
  }
  const [owner, repo, marker, ...rest] = parts;
  const cleanRepo = repo.replace(/\.git$/, '');
  if (marker === 'tree' || marker === 'blob') {
    const ref = rest[0];
    const path = rest.slice(1).join('/');
    if (!ref) throw new IngestionError('invalid-input', `The ${marker} URL is missing a branch or tag.`);
    return {
      owner,
      repo: cleanRepo,
      ref,
      ...(path ? { path } : {}),
      type: marker === 'blob' ? 'file' : 'dir',
      canonicalUrl: url.toString(),
    };
  }
  if (marker && ['issues', 'pull', 'pulls', 'actions', 'releases', 'wiki', 'settings', 'commit'].includes(marker)) {
    throw new IngestionError('invalid-input', `“${marker}” GitHub pages are not importable. Use a repository, directory, or file URL.`);
  }
  return { owner, repo: cleanRepo, type: 'repo', canonicalUrl: `https://github.com/${owner}/${cleanRepo}` };
}

export interface GithubRequestOptions {
  token?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
}

interface TreeEntry {
  path: string;
  type: 'blob' | 'tree';
  size?: number;
  sha: string;
}

async function api<T>(path: string, options: GithubRequestOptions = {}): Promise<{ data: T; remaining?: number; limit?: number }> {
  const doFetch = options.fetchImpl ?? fetch;
  const headers: Record<string, string> = { Accept: 'application/vnd.github+json' };
  if (options.token) headers.Authorization = `Bearer ${options.token}`;
  let res: Response;
  try {
    res = await doFetch(`https://api.github.com${path}`, { headers, signal: options.signal });
  } catch (err) {
    if ((err as Error)?.name === 'AbortError') throw new IngestionError('network', 'GitHub request cancelled.');
    throw new IngestionError(
      'network',
      `Could not reach api.github.com: ${err instanceof Error ? err.message : String(err)}`,
      'Check your connection or a browser extension blocking GitHub.',
    );
  }
  const remaining = Number(res.headers.get('x-ratelimit-remaining') ?? '');
  const limit = Number(res.headers.get('x-ratelimit-limit') ?? '');
  if (!res.ok) {
    if (res.status === 404) {
      throw new IngestionError(
        'invalid-input',
        'GitHub returned 404. The repository, branch, or path does not exist, or it is private (private repos need a token with access).',
      );
    }
    if (res.status === 403 || res.status === 429) {
      const reset = Number(res.headers.get('x-ratelimit-reset') ?? '');
      const when = reset ? new Date(reset * 1000).toLocaleTimeString() : 'later';
      const authed = Boolean(options.token);
      throw new IngestionError(
        'rate-limit',
        authed
          ? `GitHub rate limit or permission error (HTTP ${res.status}). The token may lack scope or the limit resets at ${when}.`
          : `GitHub rate limit reached (HTTP ${res.status}, ${remaining}/${limit} left). Unauthenticated requests are limited to ~60 API calls/hour.`,
        authed
          ? 'Wait for the reset, or use a token with the public_repo scope.'
          : 'Add a GitHub token in Settings (5000 requests/hour), or import fewer files.',
      );
    }
    throw new IngestionError('network', `GitHub request failed with HTTP ${res.status}.`);
  }
  const data = (await res.json()) as T;
  return { data, remaining: Number.isFinite(remaining) ? remaining : undefined, limit: Number.isFinite(limit) ? limit : undefined };
}

function importability(path: string, size: number | undefined): { importable: boolean; reason?: string } {
  const ext = extensionOf(path);
  if (BINARY_EXTENSIONS.has(ext)) return { importable: false, reason: `binary .${ext}` };
  if (size !== undefined && size > MAX_GITHUB_FILE_BYTES) {
    return { importable: false, reason: `${(size / 1024).toFixed(0)} KB exceeds the ${MAX_GITHUB_FILE_BYTES / 1024} KB limit` };
  }
  return { importable: true };
}

/** List importable files for a repo/dir target using a single trees call. */
export async function listGithubFiles(
  target: GithubTarget,
  options: GithubRequestOptions & { maxFiles?: number; recursive?: boolean } = {},
): Promise<{ entries: GithubEntry[]; ref: string; truncated: boolean; rateRemaining?: number; rateLimit?: number }> {
  const maxFiles = options.maxFiles ?? MAX_GITHUB_FILES;
  const ref = target.ref ?? (await resolveDefaultBranch(target, options));
  let entries: GithubEntry[] = [];
  let truncated = false;
  let rateRemaining: number | undefined;
  let rateLimit: number | undefined;

  if (target.type === 'file' && target.path) {
    const { data, remaining, limit } = await api<{ sha: string; size: number; html_url: string; download_url: string }>(
      `/repos/${target.owner}/${target.repo}/contents/${encodeURIComponent(target.path).replace(/%2F/g, '/')}?ref=${encodeURIComponent(ref)}`,
      options,
    );
    rateRemaining = remaining;
    rateLimit = limit;
    const info = importability(target.path, data.size);
    entries = [
      {
        path: target.path,
        size: data.size,
        sha: data.sha,
        htmlUrl: data.html_url,
        rawUrl: `https://raw.githubusercontent.com/${target.owner}/${target.repo}/${ref}/${target.path}`,
        ...info,
      },
    ];
    return { entries, ref, truncated, ...(rateRemaining !== undefined ? { rateRemaining } : {}), ...(rateLimit !== undefined ? { rateLimit } : {}) };
  }

  const prefix = target.path ? target.path.replace(/\/+$/, '') : '';
  const treePath = `/repos/${target.owner}/${target.repo}/git/trees/${encodeURIComponent(ref)}${options.recursive === false ? '' : '?recursive=1'}`;
  const { data, remaining, limit } = await api<{ tree: TreeEntry[]; truncated: boolean }>(treePath, options);
  rateRemaining = remaining;
  rateLimit = limit;
  truncated = data.truncated;
  const blobs = data.tree.filter((node) => node.type === 'blob');
  const inScope = prefix ? blobs.filter((n) => n.path === prefix || n.path.startsWith(`${prefix}/`)) : blobs;
  const sorted = inScope.sort((a, b) => a.path.localeCompare(b.path));
  const candidates = sorted.slice(0, maxFiles * 4);
  entries = candidates.map((node) => {
    const info = importability(node.path, node.size);
    return {
      path: node.path,
      size: node.size ?? 0,
      sha: node.sha,
      htmlUrl: `https://github.com/${target.owner}/${target.repo}/blob/${ref}/${node.path}`,
      rawUrl: `https://raw.githubusercontent.com/${target.owner}/${target.repo}/${ref}/${node.path}`,
      ...info,
    };
  });
  const importable = entries.filter((e) => e.importable);
  if (importable.length > maxFiles) {
    truncated = true;
    const keep = new Set(importable.slice(0, maxFiles).map((e) => e.path));
    entries = entries.filter((e) => !e.importable || keep.has(e.path));
  }
  if (entries.length > maxFiles * 2) entries = entries.slice(0, maxFiles * 2);
  return { entries, ref, truncated, ...(rateRemaining !== undefined ? { rateRemaining } : {}), ...(rateLimit !== undefined ? { rateLimit } : {}) };
}

/** Pick the most useful subset by default: README + docs + top-level source files. */
export function defaultSelection(entries: GithubEntry[]): GithubEntry[] {
  const importable = entries.filter((e) => e.importable);
  const readme = importable.filter((e) => /^readme(\.[a-z]+)?$/i.test(e.path));
  const docs = importable.filter((e) => /^(docs?|documentation)\//i.test(e.path));
  const root = importable.filter((e) => !e.path.includes('/') && !readme.includes(e));
  const chosen = [...readme, ...docs, ...root].slice(0, 12);
  return chosen.length ? chosen : importable.slice(0, 8);
}

async function resolveDefaultBranch(target: GithubTarget, options: GithubRequestOptions): Promise<string> {
  const { data } = await api<{ default_branch: string }>(`/repos/${target.owner}/${target.repo}`, options);
  return data.default_branch;
}

export async function fetchGithubFile(
  target: GithubTarget,
  entry: GithubEntry,
  ref: string,
  options: GithubRequestOptions = {},
): Promise<GithubFileContent> {
  const doFetch = options.fetchImpl ?? fetch;
  const headers: Record<string, string> = {};
  // Raw URLs are public; private repos require the API + token.
  const useApi = Boolean(options.token);
  const url = useApi
    ? `https://api.github.com/repos/${target.owner}/${target.repo}/contents/${encodeURIComponent(entry.path).replace(/%2F/g, '/')}?ref=${encodeURIComponent(ref)}`
    : entry.rawUrl;
  if (useApi) {
    headers.Accept = 'application/vnd.github.raw';
    headers.Authorization = `Bearer ${options.token}`;
  }
  let res: Response;
  try {
    res = await doFetch(url, { headers, signal: options.signal });
  } catch (err) {
    throw new IngestionError('network', `Could not download ${entry.path}: ${err instanceof Error ? err.message : String(err)}`);
  }
  if (!res.ok) {
    if (res.status === 403 || res.status === 429) {
      throw new IngestionError('rate-limit', `GitHub refused the download for ${entry.path} (HTTP ${res.status}).`);
    }
    throw new IngestionError('network', `Downloading ${entry.path} failed with HTTP ${res.status}.`);
  }
  const buffer = await res.arrayBuffer();
  const truncated = buffer.byteLength > MAX_GITHUB_FILE_BYTES;
  const bytes = truncated ? buffer.slice(0, MAX_GITHUB_FILE_BYTES) : buffer;
  const text = new TextDecoder('utf-8').decode(bytes);
  return {
    path: entry.path,
    text: text.slice(0, MAX_TEXT_CHARS),
    ...(entry.sha ? { sha: entry.sha } : {}),
    htmlUrl: entry.htmlUrl,
    rawUrl: entry.rawUrl,
    size: buffer.byteLength,
    truncated,
  };
}

export function segmentsForLines(text: string): Segment[] {
  return [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }];
}

export function githubOrigin(target: GithubTarget, ref: string, entry: GithubEntry, sha?: string): SourceOrigin {
  const origin: SourceOrigin = {
    repo: `${target.owner}/${target.repo}`,
    path: entry.path,
    ref,
    htmlUrl: entry.htmlUrl,
    url: entry.htmlUrl,
  };
  if (sha) origin.commit = sha;
  return origin;
}
