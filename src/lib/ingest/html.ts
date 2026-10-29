/**
 * HTML → readable text. Works in the browser (DOMParser) and in tests, with a
 * conservative regex fallback for environments without a DOM.
 */

export interface HtmlExtraction {
  title: string;
  text: string;
  headings: string[];
  byline?: string;
  siteName?: string;
  wordCount: number;
}

const BLOCK_TAGS = new Set([
  'p', 'div', 'section', 'article', 'main', 'header', 'footer', 'aside', 'nav',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'table', 'thead',
  'tbody', 'tr', 'blockquote', 'pre', 'figure', 'figcaption', 'br', 'hr',
  'dl', 'dt', 'dd', 'details', 'summary',
]);

const DROP_TAGS = new Set(['script', 'style', 'noscript', 'template', 'svg', 'canvas', 'iframe', 'form', 'button', 'input']);

export function decodeEntities(input: string): string {
  return input
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#0?39;|&apos;/gi, "'")
    .replace(/&mdash;/gi, '—')
    .replace(/&ndash;/gi, '–')
    .replace(/&hellip;/gi, '…')
    .replace(/&#(\d+);/g, (_, code: string) => String.fromCodePoint(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_, code: string) => String.fromCodePoint(parseInt(code, 16)));
}

function normaliseWhitespace(text: string): string {
  return text
    .replace(/\r\n?/g, '\n')
    .replace(/[ \t\u00a0]+/g, ' ')
    .replace(/ *\n */g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function regexExtraction(html: string): HtmlExtraction {
  const titleMatch = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html);
  const headings = Array.from(html.matchAll(/<h[1-3][^>]*>([\s\S]*?)<\/h[1-3]>/gi)).map((m) =>
    normaliseWhitespace(decodeEntities(m[1].replace(/<[^>]+>/g, ' '))),
  );
  const titleFromHeading = headings[0];
  const withoutNoise = html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<(script|style|noscript|template|svg|canvas|iframe|nav|footer|form)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(p|div|section|article|h[1-6]|li|tr|blockquote|pre|figure)>/gi, '\n\n')
    .replace(/<[^>]+>/g, ' ');
  return {
    title: normaliseWhitespace(decodeEntities(titleMatch?.[1] ?? titleFromHeading ?? '')),
    text: normaliseWhitespace(decodeEntities(withoutNoise)),
    headings,
    wordCount: 0,
  };
}

export function extractHtml(html: string): HtmlExtraction {
  const fallback = regexExtraction(html);
  let result = fallback;

  if (typeof DOMParser !== 'undefined') {
    try {
      const doc = new DOMParser().parseFromString(html, 'text/html');
      doc
        .querySelectorAll(
          'script,style,noscript,template,svg,canvas,iframe,form,button,input,nav,footer,aside[role="complementary"],.cookie-banner,.newsletter',
        )
        .forEach((node) => node.remove());

      const main = doc.querySelector('article, main, [role="main"], #content, .post, .article-body, .markdown-body') ?? doc.body;
      const headings = Array.from(main.querySelectorAll('h1,h2,h3'))
        .map((node) => normaliseWhitespace(node.textContent ?? ''))
        .filter(Boolean);

      const inline = (node: Node): string =>
        (node.textContent ?? '').replace(/\s+/g, ' ').trim();

      const serialize = (node: Node): string => {
        if (node.nodeType === 3) return (node.textContent ?? '').replace(/\s+/g, ' ');
        if (node.nodeType !== 1) return '';
        const el = node as HTMLElement;
        const tag = el.tagName.toLowerCase();
        if (DROP_TAGS.has(tag)) return '';
        switch (tag) {
          case 'br':
            return '\n';
          case 'h1':
          case 'h2':
          case 'h3':
          case 'h4':
          case 'h5':
          case 'h6':
            return `\n\n${'#'.repeat(Number(tag[1]))} ${inline(el)}\n\n`;
          case 'li':
            return `\n- ${inline(el)}`;
          case 'tr': {
            const cells = Array.from(el.querySelectorAll('th,td')).map((cell) => inline(cell));
            return cells.length ? `\n${cells.join(' | ')}` : '';
          }
          case 'pre':
          case 'code':
            return `\n\n\`\`\`\n${(el.textContent ?? '').trim()}\n\`\`\`\n\n`;
          case 'blockquote':
            return `\n\n> ${inline(el)}\n\n`;
          case 'td':
          case 'th':
            return `${inline(el)} | `;
          default: {
            const joined = Array.from(el.childNodes).map(serialize).join('');
            return BLOCK_TAGS.has(tag) ? `\n\n${joined}\n\n` : joined;
          }
        }
      };

      const rawText = main ? Array.from(main.childNodes).map(serialize).join('') : '';
      const text = normaliseWhitespace(rawText);

      const meta = (selector: string) => doc.querySelector(selector)?.getAttribute('content')?.trim() ?? '';
      const title =
        normaliseWhitespace(doc.querySelector('h1')?.textContent ?? '') ||
        meta('meta[property="og:title"]') ||
        normaliseWhitespace(doc.title) ||
        fallback.title;

      if (text.length > 20) {
        result = {
          title,
          text,
          headings,
          byline: meta('meta[name="author"]') || meta('meta[property="article:author"]') || undefined,
          siteName: meta('meta[property="og:site_name"]') || undefined,
          wordCount: countWords(text),
        };
      } else {
        result = { ...fallback, title: title || fallback.title, wordCount: countWords(fallback.text) };
      }
    } catch {
      result = fallback;
    }
  }

  return { ...result, wordCount: result.wordCount || countWords(result.text) };
}

export function countWords(text: string): number {
  const matches = text.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu);
  return matches ? matches.length : 0;
}

export interface HtmlTextResult {
  title: string;
  text: string;
  description?: string;
}

/** Extract readable text from an HTML document, preserving paragraph breaks. */
export function htmlToText(html: string, _options: { baseUrl?: string } = {}): HtmlTextResult {
  const extracted = extractHtml(html);
  return { title: extracted.title, text: extracted.text, description: extracted.siteName };
}

const META_PATTERN = /<meta\s+([^>]+)>/gi;

/** Collect `<meta>` name/property values plus `<title>`. */
export function extractMeta(html: string): Record<string, string> {
  const meta: Record<string, string> = {};
  for (const match of html.matchAll(META_PATTERN)) {
    const attrs = match[1];
    const key = /(?:name|property|itemprop)\s*=\s*["']([^"']+)["']/i.exec(attrs)?.[1];
    const value = /content\s*=\s*["']([^"']*)["']/i.exec(attrs)?.[1];
    if (key && value) meta[key.toLowerCase()] = decodeEntities(value);
  }
  const title = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html);
  if (title) meta.title = normaliseWhitespace(decodeEntities(title[1]));
  return meta;
}

/** Convert an HTML table into CSV-like rows for indexing. */
export function tablesToText(html: string): string[] {
  if (typeof DOMParser === 'undefined') return [];
  try {
    const doc = new DOMParser().parseFromString(html, 'text/html');
    return Array.from(doc.querySelectorAll('table'))
      .map((table) =>
        Array.from(table.querySelectorAll('tr'))
          .map((row) =>
            Array.from(row.querySelectorAll('th,td'))
              .map((cell) => normaliseWhitespace(cell.textContent ?? '').replace(/\s*\|\s*/g, '/'))
              .join(' | '),
          )
          .filter(Boolean)
          .join('\n'),
      )
      .filter((table) => table.length > 20);
  } catch {
    return [];
  }
}

/** Markdown → text, keeping heading markers so chunk sectioning works. */
export function markdownToText(markdown: string): string {
  return normaliseWhitespace(
    markdown
      .replace(/\r\n?/g, '\n')
      .replace(/```[\s\S]*?```/g, (block) => block.replace(/```\w*\n?/g, ''))
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1 ($2)')
      .replace(/^>\s?/gm, '')
      .replace(/^\s*[-*+]\s+/gm, '• '),
  );
}
