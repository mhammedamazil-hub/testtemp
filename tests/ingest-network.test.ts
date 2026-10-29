import { describe, expect, it, vi } from 'vitest';
import { parseGithubUrl, listGithubFiles, defaultSelection, fetchGithubFile } from '../src/lib/ingest/github';
import { importGithubFiles, importYouTube, importWebUrl, importInstagram, importTranscriptFile, detectImportRoute, IngestionError } from '../src/lib/ingest';
import { parseYouTubeUrl } from '../src/lib/ingest/youtube';
import { parseInstagramUrl } from '../src/lib/ingest/instagram';
import { parseTimedCaptions, parseJson3Captions, parseTranscript, mergeCaptionSegments, detectCaptionFormat } from '../src/lib/ingest/captions';
import { DEFAULT_SETTINGS } from '../src/lib/types';

const settings = { ...DEFAULT_SETTINGS, readerEndpoint: 'https://reader.test/', githubToken: '' };

function routeFetch(routes: Record<string, () => Response>): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
    for (const [pattern, handler] of Object.entries(routes)) {
      if (url.includes(pattern)) return handler();
    }
    throw new TypeError(`Unexpected fetch: ${url}`);
  }) as typeof fetch;
}

describe('GitHub URL parsing', () => {
  it('accepts repository, directory, blob and raw URLs', () => {
    expect(parseGithubUrl('https://github.com/owner/repo')).toMatchObject({ owner: 'owner', repo: 'repo', type: 'repo' });
    expect(parseGithubUrl('https://github.com/owner/repo/tree/main/docs')).toMatchObject({ ref: 'main', path: 'docs', type: 'dir' });
    expect(parseGithubUrl('https://github.com/owner/repo/blob/dev/src/app.ts')).toMatchObject({ ref: 'dev', path: 'src/app.ts', type: 'file' });
    expect(parseGithubUrl('https://raw.githubusercontent.com/owner/repo/main/README.md')).toMatchObject({ path: 'README.md', type: 'file' });
    expect(parseGithubUrl('https://github.com/owner/repo.git')).toMatchObject({ repo: 'repo' });
  });

  it('rejects unrelated GitHub pages and bad hosts', () => {
    expect(() => parseGithubUrl('https://github.com/owner/repo/issues/1')).toThrow(IngestionError);
    expect(() => parseGithubUrl('https://gitlab.com/owner/repo')).toThrow(/Only github.com/);
    expect(() => parseGithubUrl('not a url')).toThrow(/not a valid URL/);
    expect(() => parseGithubUrl('https://github.com/owner')).toThrow(/owner and a repository/);
  });

  it('routes URLs to the right importer', () => {
    expect(detectImportRoute('https://github.com/a/b')).toBe('github');
    expect(detectImportRoute('https://youtu.be/abc123')).toBe('youtube');
    expect(detectImportRoute('https://www.instagram.com/reel/abc/')).toBe('instagram');
    expect(detectImportRoute('https://example.com')).toBe('web');
    expect(detectImportRoute('ftp://example.com')).toBe('invalid');
  });
});

describe('GitHub listing and import', () => {
  const tree = {
    tree: [
      { path: 'README.md', type: 'blob', size: 120, sha: 'sha-readme' },
      { path: 'docs/guide.md', type: 'blob', size: 900, sha: 'sha-guide' },
      { path: 'assets/logo.png', type: 'blob', size: 2000, sha: 'sha-logo' },
      { path: 'huge.bin', type: 'blob', size: 900000, sha: 'sha-huge' },
      { path: 'src', type: 'tree', sha: 'sha-dir' },
    ],
    truncated: false,
  };

  it('lists files, flags non-importable ones and keeps provenance', async () => {
    const fetchMock = vi.fn(routeFetch({ '/git/trees/main': () => new Response(JSON.stringify(tree), { status: 200, headers: { 'x-ratelimit-remaining': '58' } }) }));
    vi.stubGlobal('fetch', fetchMock);
    const target = parseGithubUrl('https://github.com/owner/repo/tree/main');
    const listing = await listGithubFiles(target, { signal: undefined, fetchImpl: fetchMock as unknown as typeof fetch });
    expect(listing.ref).toBe('main');
    const readme = listing.entries.find((e) => e.path === 'README.md')!;
    expect(readme).toMatchObject({ importable: true, sha: 'sha-readme' });
    expect(readme.rawUrl).toBe('https://raw.githubusercontent.com/owner/repo/main/README.md');
    expect(listing.entries.find((e) => e.path === 'assets/logo.png')!.importable).toBe(false);
    expect(listing.entries.find((e) => e.path === 'huge.bin')!.importable).toBe(false);
    const suggested = defaultSelection(listing.entries).map((e) => e.path);
    expect(suggested).toContain('README.md');
    expect(suggested).toContain('docs/guide.md');
    expect(suggested).not.toContain('assets/logo.png');
  });

  it('surfaces rate limits with an explanation and a token hint', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ message: 'API rate limit exceeded' }), {
        status: 403,
        headers: { 'x-ratelimit-remaining': '0', 'x-ratelimit-limit': '60', 'x-ratelimit-reset': '1900000000' },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const error = await listGithubFiles(parseGithubUrl('https://github.com/owner/repo/tree/main'), { fetchImpl: fetchMock as unknown as typeof fetch }).catch(
      (err: IngestionError) => err,
    );
    expect(error).toBeInstanceOf(IngestionError);
    expect((error as IngestionError).code).toBe('rate-limit');
    expect((error as IngestionError).message).toMatch(/60 API calls\/hour/);
    expect((error as IngestionError).hint).toMatch(/token/i);
  });

  it('reports 404 clearly', async () => {
    const fetchMock = vi.fn(async () => new Response('{}', { status: 404 }));
    const error = await listGithubFiles(parseGithubUrl('https://github.com/owner/repo/tree/main'), { fetchImpl: fetchMock as unknown as typeof fetch }).catch(
      (err: IngestionError) => err,
    );
    expect((error as IngestionError).message).toMatch(/404/);
  });

  it('imports selected files into sources with repo provenance', async () => {
    const fetchMock = vi.fn(routeFetch({
      '/git/trees/main': () => new Response(JSON.stringify(tree), { status: 200 }),
      'raw.githubusercontent.com': () => new Response('# Guide\n\nContent from the repo.', { status: 200 }),
    }));
    vi.stubGlobal('fetch', fetchMock);
    const target = parseGithubUrl('https://github.com/owner/repo/tree/main');
    const listing = await listGithubFiles(target, { fetchImpl: fetchMock as unknown as typeof fetch });
    const selected = listing.entries.filter((e) => e.path.endsWith('.md'));
    const result = await importGithubFiles(target, selected, listing.ref, 'nb-gh', settings, undefined);
    expect(result.sources).toHaveLength(2);
    for (const source of result.sources) {
      expect(source.kind).toBe('github-file');
      expect(source.origin.repo).toBe('owner/repo');
      expect(source.origin.ref).toBe('main');
      expect(source.text).toContain('Content from the repo');
    }
    expect(result.sources[0].origin.htmlUrl).toContain('github.com/owner/repo/blob/main/');
  });

  it('downloads through the API when a token is configured (private repos)', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes('/contents/')) return new Response('secret text', { status: 200 });
      return new Response('{}', { status: 404 });
    });
    vi.stubGlobal('fetch', fetchMock);
    const target = parseGithubUrl('https://github.com/owner/private');
    const file = await fetchGithubFile(
      target,
      { path: 'notes.md', size: 11, htmlUrl: 'h', rawUrl: 'r', importable: true },
      'main',
      { token: 'tok', fetchImpl: fetchMock as unknown as typeof fetch },
    );
    expect(file.text).toBe('secret text');
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer tok');
  });
});

describe('web page ingestion', () => {
  it('parses reader output with title and markdown body', async () => {
    const fetchMock = vi.fn(async () =>
      new Response('Title: Example Article\nURL Source: https://example.com/a\nMarkdown Content:\n# Headline\n\nThe body text.', { status: 200 }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const source = await importWebUrl('https://example.com/a', 'nb-web', settings);
    expect(source.title).toBe('Example Article');
    expect(source.text).toContain('Headline');
    expect(source.kind).toBe('web');
    expect(source.origin.url).toBe('https://example.com/a');
  });

  it('falls back to a direct fetch when the reader fails', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith('https://reader.test/')) return new Response('nope', { status: 500 });
      return new Response('<html><head><title>Direct</title></head><body><p>Direct body</p></body></html>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      });
    });
    vi.stubGlobal('fetch', fetchMock);
    const source = await importWebUrl('https://example.com/direct', 'nb-web2', settings);
    expect(source.title).toBe('Direct');
    expect(source.text).toContain('Direct body');
    expect(source.warnings.join(' ')).toMatch(/reader service/i);
  });

  it('explains CORS/network blocks instead of silently failing', async () => {
    const fetchMock = vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    });
    vi.stubGlobal('fetch', fetchMock);
    const error = await importWebUrl('https://blocked.example.com', 'nb', { ...settings, readerEndpoint: '' }).catch((err: IngestionError) => err);
    expect(error).toBeInstanceOf(IngestionError);
    expect((error as IngestionError).code).toBe('blocked');
    expect((error as IngestionError).message).toMatch(/CORS or a network block/);
    expect((error as IngestionError).hint).toMatch(/reader endpoint/i);
  });

  it('refuses localhost and private network URLs', async () => {
    await expect(importWebUrl('http://localhost:8080/admin', 'nb', settings)).rejects.toMatchObject({ code: 'invalid-input' });
    await expect(importWebUrl('http://192.168.1.1/', 'nb', settings)).rejects.toMatchObject({ code: 'invalid-input' });
  });

  it('rejects binary content types', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('x', { status: 200, headers: { 'content-type': 'application/pdf' } })));
    const error = await importWebUrl('https://example.com/file.pdf', 'nb', { ...settings, readerEndpoint: '' }).catch((err: IngestionError) => err);
    expect((error as IngestionError).message).toMatch(/not a readable page/);
  });
});

describe('YouTube ingestion', () => {
  it('parses video, shorts, short-link and playlist URLs', () => {
    expect(parseYouTubeUrl('https://www.youtube.com/watch?v=dQw4w9WgXcQ')).toMatchObject({ videoId: 'dQw4w9WgXcQ' });
    expect(parseYouTubeUrl('https://youtu.be/dQw4w9WgXcQ?t=42')).toMatchObject({ videoId: 'dQw4w9WgXcQ' });
    expect(parseYouTubeUrl('https://www.youtube.com/shorts/abcdef12345')).toMatchObject({ videoId: 'abcdef12345' });
    expect(parseYouTubeUrl('https://www.youtube.com/playlist?list=PLabcdefghijklmnop')).toMatchObject({ playlistId: 'PLabcdefghijklmnop' });
    expect(() => parseYouTubeUrl('https://vimeo.com/123')).toThrow(/not a YouTube URL/);
    expect(() => parseYouTubeUrl('https://www.youtube.com/watch?v=')).toThrow(IngestionError);
  });

  it('extracts metadata and timestamped transcript through the reader', async () => {
    const readerBody = [
      'Title: Lecture on caching',
      'Author: Prof Example',
      'Markdown Content:',
      '# Lecture on caching',
      '',
      'This lecture covers cache invalidation strategies.',
      '',
      '0:00 Introduction to caches',
      '0:42 Write-through versus write-back',
      '1:30 Conclusion',
    ].join('\n');
    vi.stubGlobal('fetch', vi.fn(async () => new Response(readerBody, { status: 200 })));
    const source = await importYouTube('https://www.youtube.com/watch?v=dQw4w9WgXcQ', 'nb-yt', settings);
    expect(source.title).toBe('Lecture on caching');
    expect(source.kind).toBe('youtube');
    expect(source.origin.externalId).toBe('dQw4w9WgXcQ');
    expect(source.segments.length).toBeGreaterThanOrEqual(1);
    expect(source.segments[0].startMs).toBe(0);
    expect(source.segments[source.segments.length - 1].endMs).toBeGreaterThan(0);
    expect(source.status).toBe('ready');
    expect(source.text).toContain('Write-through');
  });

  it('imports metadata only (partial status) when no transcript is present', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('Title: Silent video\nMarkdown Content:\nJust a description.', { status: 200 })));
    const source = await importYouTube('https://youtu.be/dQw4w9WgXcQ', 'nb-yt2', settings);
    expect(source.status).toBe('partial');
    expect(source.warnings.join(' ')).toMatch(/No transcript/);
  });

  it('explains the blocker when no reader endpoint is configured', async () => {
    const error = await importYouTube('https://youtu.be/dQw4w9WgXcQ', 'nb', { ...settings, readerEndpoint: '' }).catch((err: IngestionError) => err);
    expect((error as IngestionError).code).toBe('blocked');
    expect((error as IngestionError).hint).toMatch(/\.vtt or \.srt/);
  });

  it('imports a supplied caption file with video provenance', () => {
    const vtt = 'WEBVTT\n\n00:00:01.000 --> 00:00:03.500\nHello world\n\n00:00:04.000 --> 00:00:06.000\nSecond line';
    const source = importTranscriptFile('nb', 'captions.vtt', vtt, { videoUrl: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ' });
    expect(source.kind).toBe('media-transcript');
    expect(source.origin.externalId).toBe('dQw4w9WgXcQ');
    expect(source.segments[1]).toMatchObject({ startMs: 4000, endMs: 6000 });
    expect(source.statusDetail).toMatch(/Timed transcript/);
  });
});

describe('Instagram ingestion', () => {
  it('parses post and reel URLs and rejects profiles', () => {
    expect(parseInstagramUrl('https://www.instagram.com/p/CabcDEF123/')).toMatchObject({ shortcode: 'CabcDEF123', kind: 'post' });
    expect(parseInstagramUrl('https://instagram.com/reel/Cxyz987654/')).toMatchObject({ kind: 'reel' });
    expect(() => parseInstagramUrl('https://www.instagram.com/username/')).toThrow(/post or Reel/);
    expect(() => parseInstagramUrl('https://example.com/p/abc/')).toThrow(/not an Instagram URL/);
  });

  it('extracts a public caption through the reader and documents the limits', async () => {
    const body = [
      'Title: A post on Instagram',
      'Description: Great tips for note taking and spaced repetition.',
      'Markdown Content:',
      'Great tips for note taking and spaced repetition. #study',
    ].join('\n');
    vi.stubGlobal('fetch', vi.fn(async () => new Response(body, { status: 200 })));
    const source = await importInstagram('https://www.instagram.com/p/CabcDEF123/', 'nb-ig', settings);
    expect(source.kind).toBe('instagram');
    expect(source.origin.externalId).toBe('CabcDEF123');
    expect(source.text).toMatch(/spaced repetition/);
    expect(source.warnings.join(' ')).toMatch(/No authentication/);
    expect(source.status).toBe('partial');
  });

  it('explains restrictions without a reader endpoint', async () => {
    const error = await importInstagram('https://www.instagram.com/p/CabcDEF123/', 'nb', { ...settings, readerEndpoint: '' }).catch((err: IngestionError) => err);
    expect((error as IngestionError).code).toBe('blocked');
    expect((error as IngestionError).message).toMatch(/login wall|CORS/);
  });
});

describe('caption parsing', () => {
  it('detects formats and parses VTT with timestamps', () => {
    const vtt = 'WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello <b>there</b>\n\n00:01:05.500 --> 00:01:07.000\nSecond cue';
    expect(detectCaptionFormat(vtt, 'x.vtt')).toBe('vtt');
    const segments = parseTimedCaptions(vtt);
    expect(segments).toHaveLength(2);
    expect(segments[0]).toMatchObject({ startMs: 1000, endMs: 4000, text: 'Hello there' });
    expect(segments[1].startMs).toBe(65500);
  });

  it('parses SRT with comma decimals', () => {
    const srt = '1\n00:00:02,500 --> 00:00:05,000\nSRT line one\n\n2\n00:00:06,000 --> 00:00:08,000\nSRT line two';
    expect(detectCaptionFormat(srt, 'x.srt')).toBe('srt');
    const segments = parseTranscript(srt,'x.srt');
    expect(segments.segments[0].startMs).toBe(2500);
    expect(segments.segments).toHaveLength(2);
  });

  it('parses YouTube json3 captions', () => {
    const json = JSON.stringify({ events: [{ tStartMs: 0, dDurationMs: 1500, segs: [{ utf8: 'Hello ' }, { utf8: 'world' }] }] });
    expect(detectCaptionFormat(json)).toBe('json3');
    const segments = parseJson3Captions(json);
    expect(segments[0]).toMatchObject({ startMs: 0, endMs: 1500, text: 'Hello world' });
  });

  it('falls back to paragraph segments for plain text and merges captions', () => {
    const parsed = parseTranscript('First paragraph.\n\nSecond paragraph.');
    expect(parsed.format).toBe('text');
    expect(parsed.segments).toHaveLength(2);
    const merged = mergeCaptionSegments(
      [
        { index: 0, text: 'one', startLine: 1, endLine: 1, startMs: 0, endMs: 1000 },
        { index: 1, text: 'two', startLine: 2, endLine: 2, startMs: 1000, endMs: 2000 },
      ],
      50,
    );
    expect(merged).toHaveLength(1);
    expect(merged[0].text).toBe('one two');
    expect(merged[0].endMs).toBe(2000);
  });
});
