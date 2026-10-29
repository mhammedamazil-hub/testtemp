/**
 * Small, safe Markdown renderer for the UI.
 *
 * Everything is HTML-escaped first, so model output can never inject markup.
 * Citation markers become buttons carrying `data-citation` so the chat panel
 * can open the exact supporting passage.
 */

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (ch) => ESCAPES[ch]);
}

export interface MarkdownOptions {
  citationButtons?: boolean;
  linkify?: boolean;
}

export function renderMarkdown(markdown: string, options: MarkdownOptions = {}): string {
  const { citationButtons = true, linkify = true } = options;
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let inCode = false;
  let codeLang = '';
  let listType: 'ul' | 'ol' | null = null;
  let inTable = false;
  let inQuote = false;

  const closeList = () => {
    if (listType) {
      out.push(`</${listType}>`);
      listType = null;
    }
  };
  const closeTable = () => {
    if (inTable) {
      out.push('</tbody></table>');
      inTable = false;
    }
  };
  const closeQuote = () => {
    if (inQuote) {
      out.push('</blockquote>');
      inQuote = false;
    }
  };
  const closeAll = () => {
    closeList();
    closeTable();
    closeQuote();
  };

  const inline = (raw: string): string => {
    let html = escapeHtml(raw);
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,;:!?]|$)/g, '$1<em>$2</em>');
    html = html.replace(/(^|[\s(])_([^_\n]+)_(?=[\s).,;:!?]|$)/g, '$1<em>$2</em>');
    if (linkify) {
      html = html.replace(
        /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
        '<a href="$2" target="_blank" rel="noopener noreferrer nofollow">$1</a>',
      );
      html = html.replace(
        /(^|[\s(])(https?:\/\/[^\s<)]+)/g,
        '$1<a href="$2" target="_blank" rel="noopener noreferrer nofollow">$2</a>',
      );
    }
    // Citation markers: [1] or [1, 2]
    const citeRe = /\[(\d{1,3}(?:\s*,\s*\d{1,3})*)\]/g;
    if (citationButtons) {
      html = html.replace(citeRe, (_m, group: string) => {
        const markers = group.split(',').map((m) => m.trim());
        return markers
          .map((m) => `<button type="button" class="cite-ref" data-citation="${m}" aria-label="Open source citation ${m}">[${m}]</button>`)
          .join('');
      });
    } else {
      html = html.replace(citeRe, '<sup class="cite">[$1]</sup>');
    }
    return html;
  };

  for (const raw of lines) {
    const line = raw;
    const fence = /^```(\w*)\s*$/.exec(line.trim());
    if (fence) {
      if (inCode) {
        out.push('</code></pre>');
        inCode = false;
      } else {
        closeAll();
        codeLang = fence[1];
        out.push(`<pre><code${codeLang ? ` class="lang-${escapeHtml(codeLang)}"` : ''}>`);
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      out.push(escapeHtml(line));
      continue;
    }
    if (!line.trim()) {
      closeAll();
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      closeAll();
      const level = Math.min(heading[1].length + 1, 6);
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
      continue;
    }
    if (/^\s*[-*]\s+/.test(line)) {
      if (listType !== 'ul') {
        closeAll();
        out.push('<ul>');
        listType = 'ul';
      }
      out.push(`<li>${inline(line.replace(/^\s*[-*]\s+/, ''))}</li>`);
      continue;
    }
    if (/^\s*\d+\.\s+/.test(line)) {
      if (listType !== 'ol') {
        closeAll();
        out.push('<ol>');
        listType = 'ol';
      }
      out.push(`<li>${inline(line.replace(/^\s*\d+\.\s+/, ''))}</li>`);
      continue;
    }
    if (/^\s*>\s?/.test(line)) {
      if (!inQuote) {
        closeAll();
        out.push('<blockquote>');
        inQuote = true;
      }
      out.push(`<p>${inline(line.replace(/^\s*>\s?/, ''))}</p>`);
      continue;
    }
    if (/^\|.*\|$/.test(line.trim())) {
      const isDivider = /^\|[\s:|-]+\|$/.test(line.trim());
      if (isDivider) continue;
      const cells = line.trim().slice(1, -1).split('|').map((c) => c.trim());
      if (!inTable) {
        closeAll();
        out.push('<table><thead><tr>' + cells.map((c) => `<th>${inline(c)}</th>`).join('') + '</tr></thead><tbody>');
        inTable = true;
      } else {
        out.push('<tr>' + cells.map((c) => `<td>${inline(c)}</td>`).join('') + '</tr>');
      }
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      closeAll();
      out.push('<hr />');
      continue;
    }
    closeAll();
    out.push(`<p>${inline(line)}</p>`);
  }
  closeAll();
  if (inCode) out.push('</code></pre>');
  return out.join('\n');
}
