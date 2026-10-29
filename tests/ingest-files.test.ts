import { describe, expect, it } from 'vitest';
import JSZip from 'jszip';
import { parseDocument, parseDelimited, kindForFile, decodeText, IngestionError, type FileLike } from '../src/lib/ingest/files';
import { importFile, importPastedText, makeSource } from '../src/lib/ingest';

/** jsdom's File lacks arrayBuffer(); build a FileLike directly instead. */
function fileFrom(name: string, content: string | Uint8Array, type = ''): FileLike {
  const bytes = typeof content === 'string' ? new TextEncoder().encode(content) : content;
  return {
    name,
    type,
    size: bytes.byteLength,
    arrayBuffer: async () => bytes.slice().buffer as ArrayBuffer,
  };
}

async function docxFile(text: string): Promise<FileLike> {
  const zip = new JSZip();
  zip.file(
    '[Content_Types].xml',
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>`,
  );
  zip.file(
    '_rels/.rels',
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`,
  );
  const paragraphs = text
    .split('\n')
    .map((line) => `<w:p><w:r><w:t xml:space="preserve">${line.replace(/&/g, '&amp;').replace(/</g, '&lt;')}</w:t></w:r></w:p>`)
    .join('');
  zip.file(
    'word/document.xml',
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${paragraphs}</w:body></w:document>`,
  );
  const bytes = await zip.generateAsync({ type: 'uint8array' });
  return fileFrom('report.docx', bytes, 'application/vnd.openxmlformats-officedocument.wordprocessingml.document');
}

async function pptxFile(slides: { title: string; body: string; notes?: string }[]): Promise<FileLike> {
  const zip = new JSZip();
  zip.file('[Content_Types].xml', '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>');
  zip.file('ppt/presentation.xml', '<?xml version="1.0"?><p:presentation xmlns:p="x"/>');
  slides.forEach((slide, index) => {
    const number = index + 1;
    const xml = `<?xml version="1.0"?><p:sld xmlns:p="x" xmlns:a="y"><p:cSld><p:spTree>
      <p:sp><p:txBody><a:p><a:r><a:t>${slide.title}</a:t></a:r></a:p></p:txBody></p:sp>
      <p:sp><p:txBody><a:p><a:r><a:t>${slide.body}</a:t></a:r></a:p></p:txBody></p:sp>
    </p:spTree></p:cSld></p:sld>`;
    zip.file(`ppt/slides/slide${number}.xml`, xml);
    if (slide.notes) {
      zip.file(
        `ppt/notesSlides/notesSlide${number}.xml`,
        `<?xml version="1.0"?><p:notes xmlns:p="x" xmlns:a="y"><a:p><a:r><a:t>${slide.notes}</a:t></a:r></a:p></p:notes>`,
      );
    }
  });
  const bytes = await zip.generateAsync({ type: 'uint8array' });
  return fileFrom('deck.pptx', bytes, 'application/vnd.openxmlformats-officedocument.presentationml.presentation');
}

describe('file type detection', () => {
  it('maps extensions and mime types to source kinds', () => {
    expect(kindForFile('a.pdf')).toBe('pdf');
    expect(kindForFile('a.md')).toBe('markdown');
    expect(kindForFile('a.csv')).toBe('csv');
    expect(kindForFile('a.py')).toBe('code');
    expect(kindForFile('a.vtt')).toBe('media-transcript');
    expect(kindForFile('archive.zip')).toBeNull();
    expect(kindForFile('noext', 'text/plain')).toBe('text');
  });

  it('rejects unsupported files with an actionable hint', async () => {
    await expect(parseDocument(fileFrom('a.zip', 'PK'))).rejects.toMatchObject({ code: 'unsupported' });
  });

  it('rejects oversized files before reading them', async () => {
    const fake = { name: 'huge.txt', type: 'text/plain', size: 30 * 1024 * 1024, arrayBuffer: async () => new ArrayBuffer(0) };
    await expect(parseDocument(fake)).rejects.toMatchObject({ code: 'too-large' });
  });

  it('rejects empty text files', async () => {
    await expect(parseDocument(fileFrom('empty.txt', '   \n  '))).rejects.toBeInstanceOf(IngestionError);
  });
});

describe('text-family parsing', () => {
  it('imports Markdown with metadata preserved', async () => {
    const parsed = await parseDocument(fileFrom('notes.md', '# Heading\n\nSome **bold** content.'));
    expect(parsed.kind).toBe('markdown');
    expect(parsed.text).toContain('# Heading');
    expect(parsed.status).toBe('ready');
    expect(parsed.title).toBe('notes');
  });

  it('converts CSV to a Markdown table and escapes quotes', async () => {
    const csv = 'name,role,note\n"Ada, L.","engineer","says ""hi"""\nGrace,admiral,\n';
    const parsed = await parseDocument(fileFrom('people.csv', csv));
    expect(parsed.text).toContain('| name | role | note |');
    expect(parsed.text).toContain('Ada, L.');
    expect(parsed.text).toContain('says "hi"');
  });

  it('treats TSV with tab delimiters', async () => {
    const parsed = await parseDocument(fileFrom('data.tsv', 'a\tb\n1\t2\n'));
    expect(parsed.text).toContain('| a | b |');
  });

  it('pretty-prints valid JSON and warns on invalid JSON', async () => {
    const good = await parseDocument(fileFrom('data.json', '{"a":1,"nested":{"b":[1,2]}}'));
    expect(good.text).toContain('"nested"');
    const bad = await parseDocument(fileFrom('broken.json', '{"a": '));
    expect(bad.warnings.join(' ')).toMatch(/could not be parsed/i);
  });

  it('strips HTML markup, scripts and navigation', async () => {
    const html = `<html><head><title>Doc</title><meta name="description" content="Desc"></head>
      <body><nav>menu</nav><script>alert(1)</script><h1>Title</h1><p>First <b>paragraph</b>.</p>
      <ul><li>one</li><li>two</li></ul></body></html>`;
    const parsed = await parseDocument(fileFrom('page.html', html));
    expect(parsed.text).toContain('# Title');
    expect(parsed.text).toContain('First paragraph');
    expect(parsed.text).toContain('- one');
    expect(parsed.text).not.toContain('alert(1)');
    expect(parsed.text).not.toContain('menu');
    expect(parsed.title).toBe('page');
  });

  it('decodes Windows-1252 when UTF-8 is invalid', () => {
    const bytes = new Uint8Array([0x63, 0x61, 0x66, 0xe9]); // "café" in cp1252
    const decoded = decodeText(bytes.buffer);
    expect(decoded.encoding).toBe('windows-1252');
    expect(decoded.text).toContain('café');
    expect(decoded.warning).toBeTruthy();
  });

  it('imports code files as code sources', async () => {
    const parsed = await parseDocument(fileFrom('main.rs', 'fn main() { println!("hi"); }'));
    expect(parsed.kind).toBe('code');
    expect(parsed.text).toContain('fn main');
  });
});

describe('structured documents', () => {
  it('extracts DOCX text (headings preserved)', async () => {
    const parsed = await parseDocument(await docxFile('Project brief\nScope and timeline'));
    expect(parsed.kind).toBe('docx');
    expect(parsed.text).toContain('Project brief');
    expect(parsed.text).toContain('Scope and timeline');
  });

  it('rejects a corrupt DOCX with a readable error', async () => {
    await expect(parseDocument(fileFrom('bad.docx', 'not a zip'))).rejects.toMatchObject({ code: 'corrupt' });
  });

  it('extracts PPTX slides with slide-level page references and notes', async () => {
    const parsed = await parseDocument(
      await pptxFile([
        { title: 'Intro', body: 'Welcome to the deck', notes: 'Say hello' },
        { title: 'Results', body: 'Revenue grew 20%' },
      ]),
    );
    expect(parsed.kind).toBe('pptx');
    expect(parsed.segments).toHaveLength(2);
    expect(parsed.segments[0].page).toBe(1);
    expect(parsed.segments[1].page).toBe(2);
    expect(parsed.text).toContain('Revenue grew 20%');
    expect(parsed.text).toContain('Speaker notes: Say hello');
    expect(parsed.statusDetail).toMatch(/2 slides/);
  });

  it('rejects a PPTX with no slides', async () => {
    const zip = new JSZip();
    zip.file('[Content_Types].xml', '<Types/>');
    const bytes = await zip.generateAsync({ type: 'uint8array' });
    await expect(parseDocument(fileFrom('empty.pptx', bytes))).rejects.toBeInstanceOf(IngestionError);
  });

  it('parses a real generated PDF with page references', async () => {
    const { buildPdf } = await import('../src/lib/exports/pdf');
    const bytes = await buildPdf({
      title: 'Parity report',
      artifacts: [
        {
          id: 'a1',
          notebookId: 'nb',
          kind: 'notes',
          title: 'Findings',
          body: 'The measured throughput was steady across regions.',
          citations: [],
          mode: 'local',
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ],
    });
    const parsed = await parseDocument(fileFrom('report.pdf', bytes, 'application/pdf'));
    expect(parsed.kind).toBe('pdf');
    expect(parsed.segments.length).toBeGreaterThan(0);
    expect(parsed.segments[0].page).toBe(1);
    expect(parsed.text).toMatch(/Parity report|Findings|throughput/i);
  }, 60000);
});

describe('pasted content and source identity', () => {
  it('creates a stable source with checksum and line segments', () => {
    const source = importPastedText('nb', 'Pasted', 'line one\nline two');
    expect(source.id).toMatch(/^src-/);
    expect(source.checksum).toBeTruthy();
    expect(source.segments[0].endLine).toBe(2);
    const again = makeSource({ notebookId: 'nb', title: 'Pasted', kind: 'text', text: 'line one\nline two' });
    expect(again.checksum).toBe(source.checksum);
  });

  it('rejects empty pasted content', () => {
    expect(() => importPastedText('nb', 'x', '   ')).toThrow(IngestionError);
  });

  it('importFile refuses media files with guidance', async () => {
    await expect(importFile(fileFrom('clip.mp4', 'x', 'video/mp4'), 'nb')).rejects.toMatchObject({ code: 'unsupported' });
  });

  it('parses CSV delimiters including embedded newlines', () => {
    const rows = parseDelimited('a,"multi\nline",c\n1,2,3');
    expect(rows).toHaveLength(2);
    expect(rows[0][1]).toBe('multi\nline');
  });
});
