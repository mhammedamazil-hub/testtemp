/** Real PPTX generation with pptxgenjs (a valid ZIP with OOXML presentation parts). */

import type { Artifact, Citation } from '../types';

export interface SlideDeckInput {
  title: string;
  subtitle?: string;
  artifacts: Artifact[];
  maxSlides?: number;
}

const THEME = {
  bg: '0B1220',
  fg: 'F5F7FA',
  accent: '4C8DFF',
  muted: '9AA7BA',
};

export async function buildPptx(input: SlideDeckInput): Promise<Uint8Array> {
  const mod = await import('pptxgenjs');
  const PptxGenJs = (mod.default ?? mod) as unknown as new () => {
    layout: string;
    author: string;
    subject: string;
    title: string;
    company: string;
    addSlide: () => {
      background: { color: string };
      addText: (text: string | { text: string; options?: Record<string, unknown> }[], options: Record<string, unknown>) => void;
      addNotes: (notes: string) => void;
    };
    write: (options: { outputType: 'arraybuffer' }) => Promise<ArrayBuffer>;
  };
  const pptx = new PptxGenJs();
  pptx.layout = 'LAYOUT_16x9';
  pptx.author = 'SourceForge';
  pptx.company = 'SourceForge';
  pptx.title = input.title;
  pptx.subject = 'SourceForge research deck';

  const addBase = (title: string, kicker?: string) => {
    const slide = pptx.addSlide();
    slide.background = { color: THEME.bg };
    slide.addText(title, { x: 0.6, y: 0.5, w: 8.8, h: 1.0, fontSize: 26, bold: true, color: THEME.fg });
    if (kicker) slide.addText(kicker, { x: 0.6, y: 1.4, w: 8.8, h: 0.4, fontSize: 12, color: THEME.accent });
    return slide;
  };

  const titleSlide = pptx.addSlide();
  titleSlide.background = { color: THEME.bg };
  titleSlide.addText(input.title, { x: 0.7, y: 2.0, w: 8.6, h: 1.4, fontSize: 40, bold: true, color: THEME.fg });
  titleSlide.addText(input.subtitle ?? 'Generated from grounded sources', { x: 0.7, y: 3.3, w: 8.6, h: 0.6, fontSize: 16, color: THEME.muted });
  titleSlide.addText('SourceForge', { x: 0.7, y: 4.9, w: 4, h: 0.4, fontSize: 12, color: THEME.accent });

  const maxSlides = input.maxSlides ?? 18;
  let slideCount = 1;
  const citationBlock = (citations: Citation[]) =>
    citations.slice(0, 3).map((c) => `[${c.marker}] ${c.sourceTitle} — ${c.locator}`).join('   ·   ');

  for (const artifact of input.artifacts) {
    if (slideCount >= maxSlides) break;
    const sections = artifact.body.split(/\n(?=#{1,3}\s)/).filter((s) => s.trim());
    const intro = addBase(artifact.title, artifact.mode === 'ai' ? `AI-generated · ${artifact.model ?? ''}` : 'Deterministic offline generation');
    slideCount += 1;
    const bullets = (sections[0] ?? artifact.body)
      .replace(/^#{1,6}\s*/gm, '')
      .replace(/```[\s\S]*?```/g, '[code]')
      .replace(/[*_`>]/g, '')
      .split('\n')
      .map((line) => line.replace(/^[-•]\s*/, '').trim())
      .filter((line) => line.length > 0 && line.length < 220)
      .slice(0, 6);
    if (bullets.length) {
      intro.addText(bullets.map((b) => ({ text: b, options: { bullet: true, color: THEME.fg, fontSize: 14, breakLine: true } })), {
        x: 0.6, y: 1.9, w: 8.8, h: 3.4, valign: 'top',
      });
    }
    if (artifact.citations.length) {
      intro.addNotes(`Citations: ${citationBlock(artifact.citations)}`);
      intro.addText(citationBlock(artifact.citations), { x: 0.6, y: 4.9, w: 8.8, h: 0.5, fontSize: 9, color: THEME.muted });
    }
    for (const section of sections.slice(1)) {
      if (slideCount >= maxSlides) break;
      const heading = /^#{1,3}\s*(.+)$/m.exec(section)?.[1] ?? artifact.title;
      const body = section
        .replace(/^#{1,3}\s*.+$/m, '')
        .replace(/```[\s\S]*?```/g, '[code block]')
        .replace(/[*_`>]/g, '')
        .split('\n')
        .map((l) => l.replace(/^[-•]\s*/, '').trim())
        .filter((l) => l.length > 0)
        .slice(0, 7);
      const slide = addBase(heading.slice(0, 90), artifact.title);
      slide.addText(
        body.map((line) => ({ text: line.slice(0, 240), options: { bullet: true, fontSize: 13, color: THEME.fg, breakLine: true } })),
        { x: 0.6, y: 2.0, w: 8.8, h: 3.2, valign: 'top' },
      );
      if (artifact.citations.length) slide.addText(citationBlock(artifact.citations), { x: 0.6, y: 4.9, w: 8.8, h: 0.4, fontSize: 9, color: THEME.muted });
      slideCount += 1;
    }
  }

  if (slideCount < maxSlides) {
    const closing = pptx.addSlide();
    closing.background = { color: THEME.bg };
    closing.addText('Verify before you cite', { x: 0.7, y: 2.2, w: 8.6, h: 0.8, fontSize: 28, bold: true, color: THEME.fg });
    closing.addText('Every slide references the source locator it came from. Open the notebook to inspect the exact passages.', {
      x: 0.7, y: 3.1, w: 8.6, h: 1.0, fontSize: 14, color: THEME.muted,
    });
  }

  const buffer = await pptx.write({ outputType: 'arraybuffer' });
  return new Uint8Array(buffer);
}

/** A PPTX must be a ZIP (PK header) containing presentation.xml. */
export async function isValidPptx(bytes: Uint8Array): Promise<boolean> {
  if (bytes.length < 2000) return false;
  if (!(bytes[0] === 0x50 && bytes[1] === 0x4b)) return false;
  const JSZip = (await import('jszip')).default;
  try {
    const zip = await JSZip.loadAsync(bytes);
    return Boolean(zip.files['ppt/presentation.xml'] && zip.files['[Content_Types].xml']);
  } catch {
    return false;
  }
}
