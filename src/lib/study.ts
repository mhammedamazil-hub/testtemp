/**
 * Study-material generation.
 *
 * Two modes:
 *  - AI mode: a configured provider writes the material from retrieved
 *    evidence (citations validated afterwards).
 *  - Offline mode: deterministic generators below build genuinely useful
 *    material directly from the retrieved passages. No fake "AI" output.
 */

import { quoteFrom, validateAnswerCitations } from './citations';
import { buildStudyPrompt } from './prompts';
import { providerForTask, ProviderError, resolveModel, runChat } from './providers';
import { retrieve } from './retrieval';
import { contentTokens, formatClock, truncate } from './text';
import type {
  Artifact,
  ArtifactKind,
  Chunk,
  Citation,
  Evidence,
  RetrievalResult,
  Settings,
  Source,
  TaskCategory,
} from './types';

export interface StudyContext {
  notebookId: string;
  notebookTitle: string;
  sources: Source[];
  chunks: Chunk[];
  settings: Settings;
  activeSourceIds?: string[] | null;
  onDelta?: (delta: string) => void;
  signal?: AbortSignal;
}

export interface GeneratorOptions {
  topic?: string;
  count?: number;
  forceLocal?: boolean;
}

export interface Flashcard {
  front: string;
  back: string;
  citation: number[];
}

export interface QuizQuestion {
  question: string;
  options: string[];
  answerIndex: number;
  explanation: string;
  citation: number[];
}

export interface GlossaryTerm {
  term: string;
  definition: string;
  citation: number[];
}

export interface TimelineEvent {
  date: string;
  label: string;
  citation: number[];
}

export interface ConceptNode {
  id: string;
  label: string;
  citation: number[];
}

export interface ConceptEdge {
  from: string;
  to: string;
  label: string;
}

export interface ComparisonTable {
  columns: string[];
  rows: string[][];
}

export function emptyRetrieval(query: string): RetrievalResult {
  return retrieve({ query, chunks: [], sources: [] });
}

function retrievalFor(ctx: StudyContext, query: string): RetrievalResult {
  const q = query.trim() || ctx.notebookTitle;
  return retrieve({
    query: q,
    chunks: ctx.chunks,
    sources: ctx.sources,
    activeSourceIds: ctx.activeSourceIds ?? null,
    topK: Math.max(ctx.settings.topK, 10),
    threshold: ctx.settings.insufficientEvidenceThreshold,
  });
}

function citationsFromEvidence(evidence: Evidence[]): Citation[] {
  return evidence.map((e) => ({
    marker: e.marker,
    chunkId: e.chunkId,
    sourceId: e.sourceId,
    sourceTitle: e.sourceTitle,
    quote: e.quote,
    locator: e.locator,
    score: e.score,
  }));
}

function sentences(text: string): string[] {
  return text
    .replace(/\s+/g, ' ')
    .split(/(?<=[.!?])\s+(?=[A-Z0-9“"'(])/)
    .map((s) => s.trim())
    .filter((s) => s.length > 30);
}

function rankedSentences(evidence: Evidence[], limit: number): { sentence: string; marker: number; sourceTitle: string; locator: string }[] {
  const out: { sentence: string; marker: number; sourceTitle: string; locator: string }[] = [];
  for (const item of evidence) {
    for (const sentence of sentences(item.quote)) {
      out.push({ sentence, marker: item.marker, sourceTitle: item.sourceTitle, locator: item.locator });
      if (out.length >= limit) return out;
    }
  }
  return out;
}

function keywordScores(ctx: StudyContext): { term: string; score: number; marker?: number }[] {
  const counts = new Map<string, { count: number; marker?: number }>();
  const sources = ctx.sources;
  const withMarkers = new Map<string, number>();
  sources.forEach((source, index) => {
    for (const token of contentTokens(source.title)) withMarkers.set(token, index + 1);
  });
  for (const chunk of ctx.chunks) {
    const source = sources.find((s) => s.id === chunk.sourceId);
    const marker = source ? sources.indexOf(source) + 1 : undefined;
    for (const token of new Set(contentTokens(chunk.text))) {
      if (token.length < 4) continue;
      const entry = counts.get(token) ?? { count: 0, ...(marker !== undefined ? { marker } : {}) };
      entry.count += 1;
      counts.set(token, entry);
    }
  }
  return [...counts.entries()]
    .map(([term, data]) => ({ term, score: data.count, ...(data.marker !== undefined ? { marker: data.marker } : {}) }))
    .sort((a, b) => b.score - a.score || a.term.localeCompare(b.term));
}

function definitionFor(term: string, evidence: Evidence[]): { text: string; marker: number } | null {
  for (const item of evidence) {
    for (const sentence of sentences(item.quote)) {
      if (sentence.toLowerCase().includes(term.toLowerCase())) {
        return { text: sentence, marker: item.marker };
      }
    }
  }
  return null;
}

export function buildMarkdown(kind: ArtifactKind, data: unknown, ctx: StudyContext, retrieval: RetrievalResult): string {
  const cite = (markers: number[]) => markers.map((m) => `[${m}]`).join('');
  switch (kind) {
    case 'flashcards': {
      const { cards } = data as { cards: Flashcard[] };
      return [`# Flashcards — ${ctx.notebookTitle}`, '', ...cards.map((card, i) => `**${i + 1}. ${card.front}**\n\n_${card.back}_ ${cite(card.citation)}\n`)].join('\n');
    }
    case 'quiz': {
      const { questions } = data as { questions: QuizQuestion[] };
      return [
        `# Quiz — ${ctx.notebookTitle}`,
        '',
        ...questions.map((q, i) => {
          const options = q.options.map((o, j) => `${String.fromCharCode(65 + j)}. ${o}`).join('\n');
          return `**Q${i + 1}. ${q.question}**\n\n${options}\n\n<details><summary>Answer</summary>\n\n${String.fromCharCode(65 + q.answerIndex)} — ${q.explanation} ${cite(q.citation)}\n\n</details>\n`;
        }),
      ].join('\n');
    }
    case 'glossary': {
      const { terms } = data as { terms: GlossaryTerm[] };
      return [`# Glossary — ${ctx.notebookTitle}`, '', ...terms.map((t) => `**${t.term}** — ${t.definition} ${cite(t.citation)}`)].join('\n\n');
    }
    case 'timeline': {
      const { events } = data as { events: TimelineEvent[] };
      return [`# Timeline — ${ctx.notebookTitle}`, '', ...events.map((e) => `- **${e.date}** — ${e.label} ${cite(e.citation)}`)].join('\n');
    }
    case 'quotes': {
      const { quotes } = data as { quotes: { text: string; source: string; locator: string; citation: number[] }[] };
      return [`# Key quotations — ${ctx.notebookTitle}`, '', ...quotes.map((q) => `> ${q.text}\n\n— *${q.source}*, ${q.locator} ${cite(q.citation)}\n`)].join('\n');
    }
    case 'comparison': {
      const table = data as ComparisonTable;
      const header = `| ${table.columns.join(' | ')} |`;
      const divider = `| ${table.columns.map(() => '---').join(' | ')} |`;
      return [`# Comparison — ${ctx.notebookTitle}`, '', header, divider, ...table.rows.map((r) => `| ${r.join(' | ')} |`)].join('\n');
    }
    case 'concept-map': {
      const { nodes, edges } = data as { nodes: ConceptNode[]; edges: ConceptEdge[] };
      return [
        `# Concept map — ${ctx.notebookTitle}`,
        '',
        '```mermaid',
        'graph LR',
        ...nodes.map((n) => `  ${n.id}["${n.label.replace(/"/g, "'")}"]`),
        ...edges.map((e) => `  ${e.from} -->|${e.label.replace(/\|/g, '/')}| ${e.to}`),
        '```',
        '',
        'Nodes:',
        ...nodes.map((n) => `- **${n.label}** ${cite(n.citation)}`),
      ].join('\n');
    }
    case 'study-guide':
      return [`# Study guide — ${ctx.notebookTitle}`, '', retrieval.evidence.map((e) => `## ${e.sourceTitle}\n\n${e.quote} [${e.marker}]`).join('\n\n')].join('\n');
    default:
      return `# ${kind}\n\n${retrieval.evidence.map((e) => `- ${e.quote} [${e.marker}]`).join('\n')}`;
  }
}

export function deterministicArtifact(
  kind: ArtifactKind,
  ctx: StudyContext,
  query: string,
  options: GeneratorOptions = {},
): { data: unknown; retrieval: RetrievalResult; body: string } {
  const retrieval = retrievalFor(ctx, query);
  const evidence = retrieval.evidence;
  const markers = (m: number) => [m];
  const count = options.count ?? 10;

  switch (kind) {
    case 'flashcards': {
      const cards: Flashcard[] = rankedSentences(evidence, count).map((s) => ({
        front: `What do the sources say here? (${truncate(s.sourceTitle, 40)})`,
        back: s.sentence,
        citation: markers(s.marker),
      }));
      const data = { cards };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'quiz': {
      const ranked = rankedSentences(evidence, count * 2);
      const allTerms = keywordScores(ctx).slice(0, 40).map((k) => k.term);
      const questions: QuizQuestion[] = [];
      for (const [i, item] of ranked.entries()) {
        if (questions.length >= count) break;
        const tokens = contentTokens(item.sentence).filter((t) => t.length > 4);
        const answerTerm = tokens.sort((a, b) => b.length - a.length)[0];
        if (!answerTerm) continue;
        const distractors = allTerms.filter((t) => t !== answerTerm).slice(i * 3, i * 3 + 3);
        if (distractors.length < 3) continue;
        const options = [answerTerm, ...distractors].sort();
        questions.push({
          question: `Which term completes this statement from ${item.sourceTitle}? “${item.sentence.replace(new RegExp(answerTerm, 'i'), '____')}”`,
          options,
          answerIndex: options.indexOf(answerTerm),
          explanation: item.sentence,
          citation: markers(item.marker),
        });
      }
      const data = { questions };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'glossary': {
      const terms: GlossaryTerm[] = [];
      for (const entry of keywordScores(ctx).slice(0, count * 2)) {
        const definition = definitionFor(entry.term, evidence);
        if (!definition) continue;
        terms.push({ term: entry.term, definition: definition.text, citation: markers(definition.marker) });
        if (terms.length >= count) break;
      }
      const data = { terms };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'timeline': {
      const events: TimelineEvent[] = [];
      const seen = new Set<string>();
      const datePattern = /\b((?:19|20)\d{2}(?:-\d{2}(?:-\d{2})?)?|\b(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},?\s+(?:19|20)\d{2})\b/g;
      for (const item of evidence) {
        for (const sentence of sentences(item.quote)) {
          const matches = sentence.match(datePattern);
          if (!matches) continue;
          for (const date of matches) {
            if (seen.has(date + sentence.slice(0, 30))) continue;
            seen.add(date + sentence.slice(0, 30));
            events.push({ date, label: sentence, citation: markers(item.marker) });
          }
        }
      }
      events.sort((a, b) => a.date.localeCompare(b.date));
      const data = { events: events.slice(0, 20) };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'quotes': {
      const quotes = evidence.slice(0, options.count ?? 8).map((e) => ({
        text: quoteFrom(e.quote, 300),
        source: e.sourceTitle,
        locator: e.locator,
        citation: markers(e.marker),
      }));
      const data = { quotes };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'comparison': {
      const table: ComparisonTable = {
        columns: ['Source', 'Type', 'Size', 'Main topic', 'Citation'],
        rows: ctx.sources.map((s) => {
          const marker = evidence.find((e) => e.sourceId === s.id)?.marker ?? 0;
          const topTerm = keywordScores({ ...ctx, sources: [s], chunks: ctx.chunks.filter((c) => c.sourceId === s.id) })[0]?.term ?? '—';
          return [
            s.title.replace(/\|/g, '/'),
            s.kind,
            `${s.text.length.toLocaleString()} chars`,
            topTerm,
            marker ? `[${marker}]` : '—',
          ];
        }),
      };
      const data = table;
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'concept-map': {
      const keywords = keywordScores(ctx).slice(0, 8);
      const nodes: ConceptNode[] = keywords.map((k, i) => ({
        id: `n${i + 1}`,
        label: k.term,
        citation: k.marker ? markers(k.marker) : [],
      }));
      const edges: ConceptEdge[] = [];
      for (let i = 0; i < nodes.length; i += 1) {
        edges.push({ from: nodes[i].id, to: nodes[(i + 1) % nodes.length].id, label: 'relates to' });
      }
      const data = { nodes, edges };
      return { data, retrieval, body: buildMarkdown(kind, data, ctx, retrieval) };
    }
    case 'faq': {
      const items = evidence.slice(0, 6).map((e, i) => ({
        question: `What does ${e.sourceTitle} say about “${(keywordScores(ctx)[i]?.term ?? 'this topic')}”?`,
        answer: `${e.quote} [${e.marker}]`,
      }));
      const data = { items };
      const body = [`# FAQ — ${ctx.notebookTitle}`, '', ...items.map((it) => `## ${it.question}\n\n${it.answer}`)].join('\n');
      return { data, retrieval, body };
    }
    case 'overview': {
      const themes = keywordScores(ctx).slice(0, 8).map((k) => k.term);
      const sourceLines = ctx.sources.map((s) => `- **${s.title}** (${s.kind}) — ${s.statusDetail}`);
      const body = [
        `# ${ctx.notebookTitle} — overview`,
        '',
        `**${ctx.sources.length} source${ctx.sources.length === 1 ? '' : 's'}**, ${ctx.chunks.length} indexed passages.`,
        '',
        '## Sources',
        ...sourceLines,
        '',
        '## Key themes (frequency across sources)',
        themes.map((t) => `- ${t}`).join('\n') || '- (not enough text to extract themes)',
        '',
        '## Most relevant passages to this notebook',
        ...evidence.slice(0, 5).map((e) => `> ${truncate(e.quote, 240)} [${e.marker}] — *${e.sourceTitle}*, ${e.locator}`),
        '',
        '_Deterministic overview generated offline from the imported text. Configure an AI provider for a synthesised narrative._',
      ].join('\n');
      const data = { themes, sources: ctx.sources.map((s) => ({ id: s.id, title: s.title, kind: s.kind })) };
      return { data, retrieval, body };
    }
    case 'notes': {
      const body = [
        `# Structured notes — ${ctx.notebookTitle}`,
        '',
        ...ctx.sources.map((source) => {
          const top = keywordScores({ ...ctx, sources: [source], chunks: ctx.chunks.filter((c) => c.sourceId === source.id) }).slice(0, 5);
          return [
            `## ${source.title}`,
            `*${source.kind}* · ${source.statusDetail}`,
            '',
            top.length ? top.map((t) => `- Theme: ${t.term}`).join('\n') : '- (no dominant terms)',
            '',
          ].join('\n');
        }),
      ].join('\n');
      return { data: { themes: keywordScores(ctx).slice(0, 8) }, retrieval, body };
    }
    case 'audio-script': {
      const lines: string[] = [];
      const speakers = ['Host A', 'Host B'];
      lines.push('Host A: Welcome to this SourceForge audio overview.');
      lines.push(`Host B: Today we are digging into ${ctx.notebookTitle}. We have ${ctx.sources.length} sources in play.`);
      evidence.slice(0, 6).forEach((e, i) => {
        const speaker = speakers[(i + 1) % 2];
        lines.push(`${speaker}: ${quoteFrom(e.quote, 260)} [${e.marker}]`);
        const other = speakers[i % 2];
        lines.push(`${other}: That comes from ${e.sourceTitle}, ${e.locator}.`);
      });
      lines.push('Host A: That is our evidence-backed summary — every claim above carries a source marker.');
      const body = lines.join('\n');
      return { data: { lines, citations: citationsFromEvidence(evidence) }, retrieval, body };
    }
    case 'storyboard': {
      const scenes = evidence.slice(0, 6).map((e, i) => ({
        scene: i + 1,
        timecode: `${formatClock(i * 8000)}–${formatClock((i + 1) * 8000)}`,
        onScreen: truncate(e.sourceTitle, 70),
        narration: quoteFrom(e.quote, 200),
        citation: [e.marker],
        visual: e.sourceTitle,
      }));
      const data = { scenes, media: { audio: 'narrate with the audio overview workflow', captions: 'generated from narration' } };
      const body = [
        `# Storyboard — ${ctx.notebookTitle}`,
        '',
        '> Video rendering is not available in this build. This is a production storyboard with narration, timings, and citations that can be assembled with any editor.',
        '',
        ...scenes.map((s) => `## Scene ${s.scene} · ${s.timecode}\n**On screen:** ${s.onScreen}\n\n**Narration:** ${s.narration} ${s.citation.map((c) => `[${c}]`).join('')}\n`),
      ].join('\n');
      return { data, retrieval, body };
    }
    case 'study-guide':
    default: {
      const guide = [
        `# Study guide — ${ctx.notebookTitle}`,
        '',
        '## Overview',
        `${ctx.sources.length} sources, ${ctx.chunks.length} indexed passages.`,
        '',
        '## Key concepts',
        ...keywordScores(ctx).slice(0, 10).map((k) => `- ${k.term}`),
        '',
        '## Evidence-backed details',
        ...evidence.slice(0, 8).map((e) => `- ${quoteFrom(e.quote, 300)} [${e.marker}] — *${e.sourceTitle}*, ${e.locator}`),
        '',
        '## Review questions',
        ...evidence.slice(0, 5).map((e, i) => `${i + 1}. What does ${e.sourceTitle} state about “${keywordScores(ctx)[i]?.term ?? 'this passage'}”? [${e.marker}]`),
        '',
        '_Generated offline by deterministic extraction. Configure an AI provider for a narrative study guide._',
      ].join('\n');
      return { data: { evidence }, retrieval, body: guide };
    }
  }
}

const AI_KINDS: ArtifactKind[] = ['study-guide', 'flashcards', 'quiz', 'faq', 'timeline', 'comparison', 'glossary', 'quotes', 'overview', 'notes', 'concept-map', 'audio-script', 'storyboard'];

export function taskForKind(kind: ArtifactKind): TaskCategory {
  if (kind === 'audio-script' || kind === 'storyboard') return 'audioScript';
  return 'study';
}

export interface GenerateResult {
  artifact: Artifact;
  retrieval: RetrievalResult;
  warning?: string;
}

export async function generateArtifact(
  kind: ArtifactKind,
  ctx: StudyContext,
  options: GeneratorOptions = {},
): Promise<GenerateResult> {
  const query = options.topic?.trim() || `${ctx.notebookTitle} key concepts evidence`;
  const local = deterministicArtifact(kind, ctx, query, options);
  const profile = options.forceLocal ? null : providerForTask(ctx.settings, taskForKind(kind));
  if (!profile || !AI_KINDS.includes(kind)) {
    return {
      artifact: makeArtifact(kind, ctx, local.body, local.data, local.retrieval, 'local', undefined),
      retrieval: local.retrieval,
    };
  }
  const messages = buildStudyPrompt(kind, local.retrieval, {
    ...(options.count !== undefined ? { count: options.count } : {}),
    ...(options.topic ? { topic: options.topic } : {}),
  });
  try {
    const result = await runChat(profile, {
      task: taskForKind(kind),
      request: {
        messages,
        model: resolveModel(profile, taskForKind(kind)),
        temperature: 0.3,
        maxTokens: 2600,
        ...(ctx.signal ? { signal: ctx.signal } : {}),
      },
      ...(ctx.onDelta ? { onDelta: ctx.onDelta } : {}),
    });
    const validation = validateAnswerCitations(result.text, local.retrieval.evidence);
    const text = validation.cleanedText;
    const parsed = parseStructured(kind, text);
    if (structuredKinds.has(kind)) {
      if (parsed) {
        const body = buildMarkdown(kind, parsed, ctx, local.retrieval);
        const warning = parserWarning(kind, parsed);
        return {
          artifact: makeArtifact(kind, ctx, body, parsed, local.retrieval, 'ai', result.model),
          retrieval: local.retrieval,
          ...(warning ? { warning } : {}),
        };
      }
      return {
        artifact: makeArtifact(kind, ctx, text, local.data, local.retrieval, 'ai', result.model),
        retrieval: local.retrieval,
        warning: `The model did not return valid ${kind} JSON, so its raw answer is shown instead. Regenerate for a structured version.`,
      };
    }
    return {
      artifact: makeArtifact(kind, ctx, text, local.data, local.retrieval, 'ai', result.model),
      retrieval: local.retrieval,
    };
  } catch (err) {
    const message = err instanceof ProviderError ? `${err.code}: ${err.message}` : err instanceof Error ? err.message : String(err);
    return {
      artifact: makeArtifact(kind, ctx, local.body, local.data, local.retrieval, 'local', undefined),
      retrieval: local.retrieval,
      warning: `AI generation failed (${message}). Generated the deterministic offline version instead.`,
    };
  }
}

const structuredKinds = new Set<ArtifactKind>(['flashcards', 'quiz', 'glossary', 'timeline', 'quotes', 'comparison', 'concept-map', 'storyboard']);

function parseStructured(kind: ArtifactKind, text: string): unknown {
  if (!structuredKinds.has(kind)) return undefined;
  const fenced = /```(?:json)?\s*([\s\S]*?)```/.exec(text);
  const candidate = (fenced ? fenced[1] : text).trim();
  const start = candidate.indexOf('{');
  const end = candidate.lastIndexOf('}');
  if (start === -1 || end === -1) return undefined;
  try {
    return JSON.parse(candidate.slice(start, end + 1)) as unknown;
  } catch {
    return undefined;
  }
}

function parserWarning(kind: ArtifactKind, parsed: unknown): string | undefined {
  if (kind === 'flashcards' && !(parsed as { cards?: unknown[] }).cards) return 'The model did not return valid flashcard JSON; the raw answer is shown.';
  if (kind === 'quiz' && !(parsed as { questions?: unknown[] }).questions) return 'The model did not return valid quiz JSON; the raw answer is shown.';
  return undefined;
}

function makeArtifact(
  kind: ArtifactKind,
  ctx: StudyContext,
  body: string,
  data: unknown,
  retrieval: RetrievalResult,
  mode: 'ai' | 'local',
  model: string | undefined,
): Artifact {
  const now = Date.now();
  const titles: Record<string, string> = {
    'study-guide': 'Study guide',
    flashcards: 'Flashcards',
    quiz: 'Quiz',
    faq: 'FAQ',
    timeline: 'Timeline',
    comparison: 'Comparison',
    glossary: 'Glossary',
    quotes: 'Key quotations',
    overview: 'Notebook overview',
    notes: 'Structured notes',
    'concept-map': 'Concept map',
    'audio-script': 'Audio script',
    storyboard: 'Storyboard',
  };
  return {
    id: `art-${now.toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    notebookId: ctx.notebookId,
    kind,
    title: `${titles[kind] ?? kind} — ${ctx.notebookTitle}`,
    body,
    data,
    citations: citationsFromEvidence(retrieval.evidence),
    mode,
    ...(model ? { model } : {}),
    createdAt: now,
    updatedAt: now,
  };
}
