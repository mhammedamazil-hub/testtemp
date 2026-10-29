/**
 * Local file ingestion.
 *
 * Heavy parsers (PDF.js, mammoth, JSZip) are imported lazily so the initial
 * bundle stays small and the offline shell loads fast.
 */

import type { Segment, SourceKind } from '../types';
import { htmlToText } from './html';
import { parseTranscript } from './captions';

export const MAX_FILE_BYTES = 25 * 1024 * 1024;
export const MAX_TEXT_CHARS = 900_000;

export type IngestionErrorCode =
  | 'unsupported'
  | 'corrupt'
  | 'encrypted'
  | 'too-large'
  | 'empty'
  | 'network'
  | 'blocked'
  | 'rate-limit'
  | 'invalid-input'
  | 'no-transcript'
  | 'no-text';

export class IngestionError extends Error {
  code: IngestionErrorCode;
  hint?: string;
  constructor(code: IngestionErrorCode, message: string, hint?: string) {
    super(message);
    this.name = 'IngestionError';
    this.code = code;
    if (hint) this.hint = hint;
  }
}

export interface FileLike {
  name: string;
  type?: string;
  size?: number;
  arrayBuffer(): Promise<ArrayBuffer>;
  text?: () => Promise<string>;
}

export interface ParsedDocument {
  title: string;
  text: string;
  segments: Segment[];
  warnings: string[];
  status: 'ready' | 'partial';
  statusDetail: string;
  kind: SourceKind;
  mime?: string;
  byteSize: number;
}

const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'mdx', 'rst', 'log', 'csv', 'tsv', 'json', 'jsonl', 'yaml', 'yml', 'toml', 'ini', 'conf',
  'html', 'htm', 'xml', 'svg', 'css', 'scss', 'js', 'jsx', 'ts', 'tsx', 'mjs', 'cjs', 'py', 'rb', 'go', 'rs', 'java',
  'kt', 'kts', 'c', 'h', 'cc', 'cpp', 'hpp', 'cs', 'php', 'swift', 'scala', 'sh', 'bash', 'zsh', 'sql', 'r', 'lua',
  'vue', 'svelte', 'dart', 'ex', 'exs', 'erl', 'hs', 'clj', 'pl', 'tex', 'bib', 'vtt', 'srt', 'json3',
]);

const CODE_EXTENSIONS = new Set([
  'js', 'jsx', 'ts', 'tsx', 'mjs', 'cjs', 'py', 'rb', 'go', 'rs', 'java', 'kt', 'kts', 'c', 'h', 'cc', 'cpp', 'hpp',
  'cs', 'php', 'swift', 'scala', 'sh', 'bash', 'zsh', 'sql', 'r', 'lua', 'vue', 'svelte', 'dart', 'ex', 'exs', 'erl',
  'hs', 'clj', 'pl', 'css', 'scss', 'html', 'htm', 'xml', 'yaml', 'yml', 'toml',
]);

export function extensionOf(name: string): string {
  const match = /\.([a-z0-9]+)$/i.exec(name);
  return match ? match[1].toLowerCase() : '';
}

export function kindForFile(name: string, mime = ''): SourceKind | null {
  const ext = extensionOf(name);
  if (ext === 'pdf' || mime === 'application/pdf') return 'pdf';
  if (ext === 'docx' || mime.includes('wordprocessingml')) return 'docx';
  if (ext === 'pptx' || mime.includes('presentationml')) return 'pptx';
  if (ext === 'csv' || ext === 'tsv') return 'csv';
  if (ext === 'json' || ext === 'jsonl') return 'json';
  if (ext === 'md' || ext === 'markdown' || ext === 'mdx') return 'markdown';
  if (ext === 'html' || ext === 'htm') return 'html';
  if (ext === 'vtt' || ext === 'srt' || ext === 'json3') return 'media-transcript';
  if (CODE_EXTENSIONS.has(ext)) return 'code';
  if (TEXT_EXTENSIONS.has(ext) || mime.startsWith('text/')) return 'text';
  return null;
}

export function decodeText(buffer: ArrayBuffer): { text: string; encoding: string; warning?: string } {
  const bytes = new Uint8Array(buffer);
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return { text: new TextDecoder('utf-16le').decode(bytes.subarray(2)), encoding: 'utf-16le' };
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return { text: new TextDecoder('utf-16be').decode(bytes.subarray(2)), encoding: 'utf-16be' };
  }
  const utf8 = new TextDecoder('utf-8').decode(bytes);
  const replacementCount = (utf8.match(/\uFFFD/g) ?? []).length;
  if (replacementCount > 0 && replacementCount / Math.max(1, utf8.length) > 0.01) {
    const latin = new TextDecoder('windows-1252').decode(bytes);
    return {
      text: latin,
      encoding: 'windows-1252',
      warning: 'The file was not valid UTF-8; it was decoded as Windows-1252. Some characters may be inaccurate.',
    };
  }
  return { text: utf8.replace(/^\uFEFF/, ''), encoding: 'utf-8' };
}

function lineSegments(text: string): Segment[] {
  return [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }];
}

function csvToMarkdown(text: string, delimiter = ','): { text: string; warnings: string[] } {
  const rows = parseDelimited(text, delimiter);
  const warnings: string[] = [];
  const MAX_ROWS = 2000;
  const trimmed = rows.slice(0, MAX_ROWS);
  if (rows.length > MAX_ROWS) warnings.push(`Only the first ${MAX_ROWS} of ${rows.length} rows were imported.`);
  if (!trimmed.length) return { text: '', warnings };
  const width = Math.max(...trimmed.map((r) => r.length));
  const pad = (row: string[]) => [...row, ...Array(Math.max(0, width - row.length)).fill('')];
  const header = pad(trimmed[0]);
  const lines = [`| ${header.join(' | ')} |`, `| ${header.map(() => '---').join(' | ')} |`];
  for (const row of trimmed.slice(1)) lines.push(`| ${pad(row).join(' | ')} |`);
  return { text: lines.join('\n'), warnings };
}

/** RFC4180-ish CSV parser (handles quotes, escaped quotes, CRLF, embedded newlines). */
export function parseDelimited(text: string, delimiter = ','): string[][] {
  const rows: string[][] = [];
  let field = '';
  let row: string[] = [];
  let inQuotes = false;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (inQuotes) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        field += ch;
      }
      continue;
    }
    if (ch === '"') {
      inQuotes = true;
    } else if (ch === delimiter) {
      row.push(field);
      field = '';
    } else if (ch === '\n') {
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else if (ch !== '\r') {
      field += ch;
    }
  }
  if (field.length || row.length) {
    row.push(field);
    rows.push(row);
  }
  return rows.filter((r) => r.some((cell) => cell.trim() !== ''));
}

export async function parseDocument(file: FileLike): Promise<ParsedDocument> {
  const size = file.size ?? (await file.arrayBuffer()).byteLength;
  if (size > MAX_FILE_BYTES) {
    throw new IngestionError(
      'too-large',
      `“${file.name}” is ${(size / 1024 / 1024).toFixed(1)} MB, above the ${MAX_FILE_BYTES / 1024 / 1024} MB limit.`,
      'Split the file, or paste the relevant excerpt instead.',
    );
  }
  const kind = kindForFile(file.name, file.type ?? '');
  if (!kind) {
    throw new IngestionError(
      'unsupported',
      `“.${extensionOf(file.name) || 'unknown'}” files are not supported.`,
      'Supported: PDF, DOCX, PPTX, TXT, MD, CSV/TSV, JSON, HTML, VTT/SRT and source code.',
    );
  }
  if (kind === 'pdf') return parsePdf(await file.arrayBuffer(), file.name, size);
  if (kind === 'docx') return parseDocx(await file.arrayBuffer(), file.name, size);
  if (kind === 'pptx') return parsePptx(await file.arrayBuffer(), file.name, size);

  const buffer = await file.arrayBuffer();
  const decoded = decodeText(buffer);
  const warnings = decoded.warning ? [decoded.warning] : [];
  let text = decoded.text;
  let kindOut = kind;
  if (kind === 'csv') {
    const parsed = csvToMarkdown(text, extensionOf(file.name) === 'tsv' ? '\t' : ',');
    text = parsed.text;
    warnings.push(...parsed.warnings);
    if (!text) throw new IngestionError('empty', `“${file.name}” contains no data rows.`);
  } else if (kind === 'json') {
    try {
      const parsed: unknown = JSON.parse(decoded.text);
      text = JSON.stringify(parsed, null, 2);
      warnings.push('JSON was pretty-printed for indexing; structure outside string values is not searchable.');
    } catch {
      text = decoded.text;
      warnings.push('JSON could not be parsed; the raw text was imported instead.');
    }
  } else if (kind === 'html') {
    const extracted = htmlToText(decoded.text, { baseUrl: file.name });
    text = extracted.text;
    if (!text.trim()) throw new IngestionError('empty', `“${file.name}” has no readable text after removing markup.`);
  } else if (kind === 'media-transcript') {
    const parsed = parseTranscript(decoded.text, file.name);
    text = parsed.text;
    if (!parsed.segments.length) throw new IngestionError('empty', `No transcript text found in “${file.name}”.`);
    return {
      title: file.name.replace(/\.[a-z0-9]+$/i, ''),
      text: text.slice(0, MAX_TEXT_CHARS),
      segments: parsed.segments,
      warnings,
      status: 'ready',
      statusDetail: `Transcript parsed (${parsed.segments.length} timed segments, ${parsed.format.toUpperCase()}).`,
      kind: kindOut,
      ...(file.type ? { mime: file.type } : {}),
      byteSize: size,
    };
  }
  if (!text.trim()) throw new IngestionError('empty', `“${file.name}” appears to be empty.`);
  if (text.length > MAX_TEXT_CHARS) {
    text = text.slice(0, MAX_TEXT_CHARS);
    warnings.push(`Text truncated to ${MAX_TEXT_CHARS.toLocaleString()} characters for indexing.`);
  }
  void kindOut;
  return {
    title: file.name.replace(/\.[a-z0-9]+$/i, ''),
    text,
    segments: lineSegments(text),
    warnings,
    status: warnings.length ? 'partial' : 'ready',
    statusDetail: `Extracted ${text.length.toLocaleString()} characters from ${kindLabel(kind)} (${decoded.encoding.toUpperCase()}).`,
    kind,
    ...(file.type ? { mime: file.type } : {}),
    byteSize: size,
  };
}

function kindLabel(kind: SourceKind): string {
  switch (kind) {
    case 'csv':
      return 'CSV (converted to a Markdown table)';
    case 'json':
      return 'JSON';
    case 'html':
      return 'HTML (markup removed)';
    case 'code':
      return 'source code';
    case 'markdown':
      return 'Markdown';
    default:
      return 'plain text';
  }
}

export async function parsePdf(buffer: ArrayBuffer, name: string, size: number): Promise<ParsedDocument> {
  let pdfjs: typeof import('pdfjs-dist/legacy/build/pdf.mjs');
  try {
    pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
  } catch (err) {
    throw new IngestionError('corrupt', `The PDF engine failed to load: ${err instanceof Error ? err.message : String(err)}`);
  }
  const bytes = new Uint8Array(buffer);
  // Encrypted PDFs are rejected by PDF.js with a password exception.
  let doc: Awaited<ReturnType<typeof pdfjs.getDocument>['promise']>;
  try {
    doc = await pdfjs.getDocument({ data: bytes, isEvalSupported: false, useSystemFonts: true, disableFontFace: true }).promise;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (/password/i.test(message)) {
      throw new IngestionError('encrypted', `“${name}” is password-protected and cannot be read.`, 'Remove the password and try again.');
    }
    throw new IngestionError('corrupt', `“${name}” could not be parsed as a PDF: ${message}`);
  }
  const warnings: string[] = [];
  const segments: Segment[] = [];
  const pageTexts: string[] = [];
  const maxPages = Math.min(doc.numPages, 400);
  if (doc.numPages > maxPages) warnings.push(`Only the first ${maxPages} of ${doc.numPages} pages were extracted.`);
  for (let pageNo = 1; pageNo <= maxPages; pageNo += 1) {
    const page = await doc.getPage(pageNo);
    const content = await page.getTextContent();
    const rows = new Map<number, { x: number; str: string }[]>();
    for (const item of content.items) {
      if (!('str' in item)) continue;
      const textItem = item as { str: string; transform: number[] };
      const str = textItem.str;
      if (!str.trim()) continue;
      const y = Math.round(textItem.transform[5] / 2) * 2;
      const bucket = rows.get(y) ?? [];
      bucket.push({ x: textItem.transform[4], str });
      rows.set(y, bucket);
    }
    const sortedY = [...rows.keys()].sort((a, b) => b - a);
    const pageText = sortedY
      .map((y) =>
        (rows.get(y) ?? [])
          .sort((a, b) => a.x - b.x)
          .map((i) => i.str)
          .join(' ')
          .replace(/\s+/g, ' ')
          .trim(),
      )
      .filter(Boolean)
      .join('\n');
    if (pageText) {
      segments.push({ index: segments.length, text: pageText, startLine: 0, endLine: 0, page: pageNo });
      pageTexts.push(`--- Page ${pageNo} ---\n${pageText}`);
    }
    page.cleanup();
  }
  await doc.destroy();
  if (!segments.length) {
    throw new IngestionError(
      'no-text',
      `“${name}” has no extractable text layer (it is probably a scanned image PDF).`,
      'OCR is not available offline. Paste the text, or upload a text-based PDF.',
    );
  }
  let line = 1;
  for (const segment of segments) {
    segment.startLine = line;
    line += segment.text.split('\n').length;
    segment.endLine = line - 1;
    line += 2;
  }
  let text = pageTexts.join('\n\n');
  if (text.length > MAX_TEXT_CHARS) {
    text = text.slice(0, MAX_TEXT_CHARS);
    warnings.push('Text truncated for indexing (page references beyond the cutoff are unavailable).');
  }
  return {
    title: name.replace(/\.[a-z0-9]+$/i, ''),
    text,
    segments,
    warnings,
    status: warnings.length ? 'partial' : 'ready',
    statusDetail: `Extracted ${segments.length} page${segments.length === 1 ? '' : 's'} of text from a ${doc.numPages}-page PDF.`,
    kind: 'pdf',
    mime: 'application/pdf',
    byteSize: size,
  };
}

export async function parseDocx(buffer: ArrayBuffer, name: string, size: number): Promise<ParsedDocument> {
  let mammoth: typeof import('mammoth');
  try {
    mammoth = await import('mammoth');
  } catch (err) {
    throw new IngestionError('corrupt', `The DOCX engine failed to load: ${err instanceof Error ? err.message : String(err)}`);
  }
  try {
    // mammoth's Node build wants a Buffer; its browser build wants an ArrayBuffer.
    const input =
      typeof Buffer !== 'undefined'
        ? ({ buffer: Buffer.from(new Uint8Array(buffer)) } as unknown as { arrayBuffer: ArrayBuffer })
        : { arrayBuffer: buffer };
    const result = await mammoth.convertToHtml(input);
    const warnings = result.messages.map((m: { message: string }) => m.message).slice(0, 5);
    const text = htmlToText(result.value).text.slice(0, MAX_TEXT_CHARS);
    if (!text.trim()) throw new IngestionError('empty', `“${name}” has no readable text.`);
    return {
      title: name.replace(/\.[a-z0-9]+$/i, ''),
      text,
      segments: lineSegments(text),
      warnings,
      status: warnings.length ? 'partial' : 'ready',
      statusDetail: `Extracted ${text.length.toLocaleString()} characters from DOCX (headings preserved).`,
      kind: 'docx',
      mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      byteSize: size,
    };
  } catch (err) {
    if (err instanceof IngestionError) throw err;
    throw new IngestionError(
      'corrupt',
      `“${name}” could not be parsed as DOCX: ${err instanceof Error ? err.message : String(err)}`,
      'Legacy .doc files are not supported — save as .docx or PDF first.',
    );
  }
}

export async function parsePptx(buffer: ArrayBuffer, name: string, size: number): Promise<ParsedDocument> {
  const JSZip = (await import('jszip')).default;
  let zip: InstanceType<typeof JSZip>;
  try {
    zip = await JSZip.loadAsync(buffer);
  } catch (err) {
    throw new IngestionError('corrupt', `“${name}” is not a valid PPTX archive: ${err instanceof Error ? err.message : String(err)}`);
  }
  const slidePaths = Object.keys(zip.files)
    .filter((p) => /^ppt\/slides\/slide\d+\.xml$/.test(p))
    .sort((a, b) => slideNumber(a) - slideNumber(b));
  if (!slidePaths.length) throw new IngestionError('corrupt', `“${name}” contains no slides.`);
  const warnings: string[] = [];
  const segments: Segment[] = [];
  const blocks: string[] = [];
  for (const path of slidePaths) {
    const xml = await zip.files[path].async('string');
    const texts = extractPptxTexts(xml);
    if (!texts.length) continue;
    const slideNumberValue = slideNumber(path);
    const body = texts.join('\n');
    const notesPath = `ppt/notesSlides/notesSlide${slideNumberValue}.xml`;
    let notes = '';
    if (zip.files[notesPath]) {
      notes = extractPptxTexts(await zip.files[notesPath].async('string')).join(' ');
    }
    const content = `## Slide ${slideNumberValue}\n${body}${notes ? `\n\nSpeaker notes: ${notes}` : ''}`;
    blocks.push(content);
    segments.push({
      index: segments.length,
      text: content,
      startLine: 0,
      endLine: 0,
      page: slideNumberValue,
      section: `Slide ${slideNumberValue}`,
    });
  }
  if (!segments.length) throw new IngestionError('empty', `“${name}” has no text on its slides (image-only deck?).`);
  let line = 1;
  for (const segment of segments) {
    segment.startLine = line;
    line += segment.text.split('\n').length;
    segment.endLine = line - 1;
    line += 2;
  }
  const text = blocks.join('\n\n').slice(0, MAX_TEXT_CHARS);
  if (slidePaths.length > segments.length) {
    warnings.push(`${slidePaths.length - segments.length} slide(s) had no text and were skipped.`);
  }
  return {
    title: name.replace(/\.[a-z0-9]+$/i, ''),
    text,
    segments,
    warnings,
    status: warnings.length ? 'partial' : 'ready',
    statusDetail: `Extracted ${segments.length} slide${segments.length === 1 ? '' : 's'} (slide-level citations).`,
    kind: 'pptx',
    mime: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    byteSize: size,
  };
}

function slideNumber(path: string): number {
  const match = /slide(\d+)\.xml$/.exec(path);
  return match ? Number(match[1]) : 0;
}

export function extractPptxTexts(xml: string): string[] {
  const texts: string[] = [];
  const re = /<a:t>([\s\S]*?)<\/a:t>/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(xml))) {
    const value = match[1]
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&amp;/g, '&')
      .replace(/&quot;/g, '"')
      .replace(/&#(\d+);/g, (_m, d: string) => String.fromCodePoint(Number(d)))
      .trim();
    if (value) texts.push(value);
  }
  return texts;
}
