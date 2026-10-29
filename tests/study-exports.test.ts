import { describe, expect, it, vi } from 'vitest';
import { buildChunks } from '../src/lib/text';
import { makeSource } from '../src/lib/ingest';
import { generateArtifact, deterministicArtifact, type StudyContext } from '../src/lib/study';
import { generateAudioOverview, splitForSpeech, estimateSpeechMs, transcriptWithTimestamps } from '../src/lib/audio';
import { buildPdf, isValidPdf } from '../src/lib/exports/pdf';
import { buildPptx, isValidPptx } from '../src/lib/exports/pptx';
import { flashcardsCsv, quizCsv, toCsv, csvCell, validateCsv } from '../src/lib/exports/csv';
import { notebookMarkdown, studyReportHtml, markdownToHtml } from '../src/lib/exports/report';
import { DEFAULT_SETTINGS, type Artifact, type ArtifactKind, type Settings } from '../src/lib/types';

const NOTEBOOK = { id: 'nb-study', title: 'Renewals research', description: 'Evidence about retention', emoji: '📓', createdAt: 1, updatedAt: 1 };

function context(settings: Settings = { ...DEFAULT_SETTINGS }): StudyContext {
  const notebookId = NOTEBOOK.id;
  const sources = [
    makeSource({
      notebookId,
      title: 'Annual report 2023',
      kind: 'text',
      text: 'Enterprise renewal rates reached 92 percent in 2023. Net revenue retention improved on 2023-06-01 after the pricing change. Customers cited onboarding quality as the main reason for renewing.',
    }),
    makeSource({
      notebookId,
      title: 'Competitor brief',
      kind: 'text',
      text: 'A competitor claims enterprise renewal rates of 97 percent. Their analyst note on 2023-09-15 describes bundled pricing and migration incentives.',
    }),
    makeSource({
      notebookId,
      title: 'Interview notes',
      kind: 'text',
      text: 'Customers value transparent pricing. Onboarding quality and renewal terms came up repeatedly in interviews.',
    }),
  ];
  const chunks = sources.flatMap((s) => buildChunks(s.id, notebookId, s.text, { chunkSize: 400, chunkOverlap: 40 }, s.segments));
  return { notebookId, notebookTitle: NOTEBOOK.title, sources, chunks, settings };
}

const ALL_KINDS: ArtifactKind[] = [
  'overview', 'study-guide', 'flashcards', 'quiz', 'faq', 'timeline', 'comparison', 'glossary', 'quotes',
  'concept-map', 'notes', 'audio-script', 'storyboard',
];

describe('deterministic study generation (offline)', () => {
  it('produces every artifact kind from real source text', () => {
    const ctx = context();
    for (const kind of ALL_KINDS) {
      const result = deterministicArtifact(kind, ctx, 'enterprise renewal rates onboarding', { count: 8 });
      expect(result.retrieval.evidence.length, `evidence for ${kind}`).toBeGreaterThan(0);
      expect(result.body.length, `body for ${kind}`).toBeGreaterThan(40);
      expect(result.data).toBeTruthy();
    }
  });

  it('builds flashcards with citation markers', () => {
    const { data, retrieval } = deterministicArtifact('flashcards', context(), 'enterprise renewal rates', { count: 6 });
    const cards = (data as { cards: { front: string; back: string; citation: number[] }[] }).cards;
    expect(cards.length).toBeGreaterThan(0);
    for (const card of cards) {
      expect(card.front.length).toBeGreaterThan(3);
      expect(card.back.length).toBeGreaterThan(10);
      expect(card.citation[0]).toBeGreaterThanOrEqual(1);
      expect(card.citation[0]).toBeLessThanOrEqual(retrieval.evidence.length);
    }
  });

  it('builds quizzes whose answer index always points at a real option', () => {
    const { data } = deterministicArtifact('quiz', context(), 'enterprise renewal rates pricing', { count: 6 });
    const questions = (data as { questions: { options: string[]; answerIndex: number; question: string }[] }).questions;
    expect(questions.length).toBeGreaterThan(0);
    for (const question of questions) {
      expect(question.options.length).toBe(4);
      expect(question.options[question.answerIndex]).toBeTruthy();
      expect(new Set(question.options).size).toBe(4);
    }
  });

  it('extracts dated events for the timeline', () => {
    const { data } = deterministicArtifact('timeline', context(), 'renewal pricing changes');
    const events = (data as { events: { date: string }[] }).events;
    expect(events.length).toBeGreaterThanOrEqual(2);
    expect(events[0].date <= events[events.length - 1].date).toBe(true);
  });

  it('builds a comparison table and a concept map', () => {
    const comparison = deterministicArtifact('comparison', context(), 'sources');
    const table = comparison.data as { columns: string[]; rows: string[][] };
    expect(table.columns).toContain('Source');
    expect(table.rows.length).toBe(3);
    const map = deterministicArtifact('concept-map', context(), 'renewal onboarding pricing');
    const nodes = (map.data as { nodes: { id: string; label: string }[] }).nodes;
    expect(nodes.length).toBeGreaterThanOrEqual(3);
    expect(map.body).toContain('```mermaid');
  });

  it('creates a storyboard that is honest about missing video rendering', () => {
    const { body, data } = deterministicArtifact('storyboard', context(), 'renewal rates');
    expect(body).toMatch(/Video rendering is not available/);
    expect((data as { scenes: unknown[] }).scenes.length).toBeGreaterThan(0);
  });
});

describe('AI study generation with mocked provider', () => {
  const provider = {
    id: 'p',
    label: 'Mock',
    protocol: 'openai' as const,
    baseUrl: 'https://api.example.com/v1',
    apiKey: 'k',
    defaultModel: 'mock',
    models: {},
    capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false },
  };

  it('parses structured flashcard JSON and rebuilds Markdown', async () => {
    const settings: Settings = { ...DEFAULT_SETTINGS, providers: [provider], activeProviderId: provider.id };
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            choices: [
              {
                message: {
                  content: '```json\n{"cards":[{"front":"What was the renewal rate?","back":"92 percent","citation":[1]}]}\n```',
                },
              },
            ],
            model: 'mock',
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    const result = await generateArtifact('flashcards', context(settings), { count: 3 });
    expect(result.artifact.mode).toBe('ai');
    const cards = (result.artifact.data as { cards: { front: string }[] }).cards;
    expect(cards[0].front).toBe('What was the renewal rate?');
    expect(result.artifact.body).toContain('What was the renewal rate?');
    expect(result.artifact.citations.length).toBeGreaterThan(0);
  });

  it('falls back to deterministic output when the model returns junk', async () => {
    const settings: Settings = { ...DEFAULT_SETTINGS, providers: [provider], activeProviderId: provider.id };
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ choices: [{ message: { content: 'Here is a quiz but not as JSON: question one?' } }] }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    const result = await generateArtifact('quiz', context(settings), { count: 4 });
    expect(result.artifact.mode).toBe('ai');
    expect(result.artifact.body).toContain('question one?');
    expect(result.warning ?? '').toMatch(/valid quiz JSON|raw answer/i);
  });

  it('falls back and warns when the provider errors', async () => {
    const settings: Settings = { ...DEFAULT_SETTINGS, providers: [provider], activeProviderId: provider.id };
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: { message: 'nope' } }), { status: 500 })));
    const result = await generateArtifact('study-guide', context(settings));
    expect(result.artifact.mode).toBe('local');
    expect(result.warning).toMatch(/AI generation failed/);
  });
});

describe('audio overview', () => {
  it('reports unavailability without a TTS provider (never fake audio)', async () => {
    const result = await generateAudioOverview(context());
    expect(result.available).toBe(false);
    expect(result.parts).toHaveLength(0);
    expect(result.reason).toMatch(/Speech synthesis needs a provider/);
    expect(result.script.body.length).toBeGreaterThan(50);
  });

  it('synthesizes real audio parts with a mocked TTS provider', async () => {
    const ttsProvider = {
      id: 'tts',
      label: 'Mock TTS',
      protocol: 'openai' as const,
      baseUrl: 'https://api.example.com/v1',
      apiKey: 'k',
      defaultModel: 'tts-1',
      models: { tts: 'tts-1' },
      capabilities: { text: false, vision: false, embeddings: false, tts: true, transcription: false, streaming: false },
    };
    const settings: Settings = { ...DEFAULT_SETTINGS, providers: [ttsProvider], activeProviderId: ttsProvider.id };
    vi.stubGlobal('fetch', vi.fn(async () => new Response(new Uint8Array([1, 2, 3, 4, 5]).buffer, { status: 200, headers: { 'content-type': 'audio/mpeg' } })));
    const result = await generateAudioOverview(context(settings), { onProgress: () => {} });
    expect(result.available).toBe(true);
    expect(result.parts.length).toBeGreaterThan(0);
    expect(result.parts[0].blob.size).toBe(5);
    expect(result.parts[0].url).toMatch(/^blob:/);
    const transcript = transcriptWithTimestamps(result);
    expect(transcript).toMatch(/\[\d+:\d\d–\d+:\d\d\]/);
  });

  it('chunks long scripts at sentence boundaries and estimates duration', () => {
    const chunks = splitForSpeech('Sentence one is here. Sentence two follows. Sentence three ends.', 30);
    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.join(' ')).toContain('Sentence three');
    expect(estimateSpeechMs('one two three four five', 100)).toBe(3000);
  });
});

describe('exported artifacts', () => {
  const ctx = context();

  it('produces valid Markdown and escaped HTML reports', () => {
    const artifact = deterministicArtifact('study-guide', ctx, 'renewal rates').body;
    const art: Artifact = {
      id: 'a1', notebookId: NOTEBOOK.id, kind: 'study-guide', title: 'Study guide', body: artifact,
      citations: [], mode: 'local', createdAt: Date.now(), updatedAt: Date.now(),
    };
    const markdown = notebookMarkdown({ notebook: NOTEBOOK, sources: ctx.sources, artifacts: [art] });
    expect(markdown).toContain('# Renewals research');
    expect(markdown).toContain('## Sources');
    expect(markdown).toContain('Annual report 2023');
    const html = studyReportHtml({ notebook: { ...NOTEBOOK, title: '<script>alert(1)</script>' }, sources: ctx.sources, artifacts: [art] });
    expect(html.startsWith('<!doctype html>')).toBe(true);
    expect(html).not.toContain('<script>alert(1)</script>');
    expect(html).toContain('&lt;script&gt;');
    expect(markdownToHtml('**bold** and [1]')).toContain('<strong>bold</strong>');
  });

  it('creates a valid PDF with %PDF header and EOF marker', async () => {
    const body = deterministicArtifact('study-guide', ctx, 'renewal rates').body;
    const bytes = await buildPdf({ title: 'Renewals research', artifacts: [{ id: 'a', notebookId: 'nb', kind: 'study-guide', title: 'Guide', body, citations: [], mode: 'local', createdAt: Date.now(), updatedAt: Date.now() }] });
    expect(isValidPdf(bytes)).toBe(true);
    expect(new TextDecoder('latin1').decode(bytes.subarray(0, 5))).toBe('%PDF-');
  }, 30000);

  it('creates a valid PPTX archive containing presentation parts', async () => {
    const body = deterministicArtifact('overview', ctx, 'sources').body;
    const artifact: Artifact = { id: 'a', notebookId: 'nb', kind: 'overview', title: 'Overview', body, citations: [], mode: 'local', createdAt: Date.now(), updatedAt: Date.now() };
    const bytes = await buildPptx({ title: 'Renewals deck', artifacts: [artifact] });
    expect(await isValidPptx(bytes)).toBe(true);
    const JSZip = (await import('jszip')).default;
    const zip = await JSZip.loadAsync(bytes);
    const slides = Object.keys(zip.files).filter((p) => /^ppt\/slides\/slide\d+\.xml$/.test(p));
    expect(slides.length).toBeGreaterThanOrEqual(2);
  }, 30000);

  it('exports CSV with escaped cells and formula-injection protection', () => {
    const artifact: Artifact = {
      id: 'a', notebookId: 'nb', kind: 'flashcards', title: 'Cards',
      body: '', citations: [], mode: 'local', createdAt: 1, updatedAt: 1,
      data: { cards: [{ front: 'Q, with comma', back: '=SUM(A1:A2)', citation: [1] }] },
    };
    const csv = flashcardsCsv(artifact);
    expect(csv).toContain('"Q, with comma"');
    expect(csv).toContain("'=SUM(A1:A2)");
    expect(validateCsv(csv).ok).toBe(true);
    const quiz: Artifact = {
      ...artifact,
      kind: 'quiz',
      data: { questions: [{ question: 'Pick', options: ['a', 'b', 'c', 'd'], answerIndex: 2, explanation: 'because', citation: [1] }] },
    };
    const quizRows = quizCsv(quiz).trim().split('\r\n');
    expect(quizRows).toHaveLength(2);
    expect(quizRows[1]).toContain(',C,');
    expect(csvCell('plain')).toBe('plain');
    expect(toCsv([['a', 'b'], ['c', 'd']])).toBe('a,b\r\nc,d\r\n');
  });
});
