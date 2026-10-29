/**
 * The research pipeline: retrieve -> prompt -> generate -> validate citations.
 * Works with or without an AI provider (offline mode returns an extractive,
 * fully cited answer built from the retrieved passages).
 */

import { validateAnswerCitations } from './citations';
import { buildResearchMessages } from './prompts';
import { providerForTask, ProviderError, resolveModel, runChat } from './providers';
import { retrieve } from './retrieval';
import { locatorFor } from './text';
import type {
  ChatMessage,
  Chunk,
  Citation,
  RetrievalResult,
  Settings,
  Source,
} from './types';

export interface AskOptions {
  question: string;
  notebookId: string;
  chunks: Chunk[];
  sources: Source[];
  settings: Settings;
  history?: ChatMessage[];
  activeSourceIds?: string[] | null;
  onDelta?: (delta: string) => void;
  signal?: AbortSignal;
  queryEmbedding?: number[] | null;
  /** Force local extractive mode even if a provider is configured. */
  forceLocal?: boolean;
}

export interface AskResult {
  message: ChatMessage;
  retrieval: RetrievalResult;
}

function uid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID();
  return `m-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function retrieveFor(options: AskOptions): RetrievalResult {
  return retrieve({
    query: options.question,
    chunks: options.chunks,
    sources: options.sources,
    activeSourceIds: options.activeSourceIds,
    topK: options.settings.topK,
    threshold: options.settings.insufficientEvidenceThreshold,
    queryEmbedding: options.queryEmbedding ?? null,
  });
}

export function localAnswer(retrieval: RetrievalResult): string {
  if (!retrieval.hits.length) {
    return 'Insufficient evidence in the selected sources. No passage matched your question. Add a source that covers this topic, or rephrase the question with terms that appear in the material.';
  }
  if (retrieval.insufficient) {
    const preview = retrieval.evidence
      .slice(0, 2)
      .map((e) => `- ${e.quote} [${e.marker}] — ${e.sourceTitle}, ${e.locator}`)
      .join('\n');
    return [
      '**Insufficient evidence in the selected sources.**',
      '',
      'The best matches were only weakly related to the question:',
      '',
      preview || '(no close matches)',
      '',
      'Offline mode cannot reason beyond the retrieved text. Rephrase using wording from the sources, select more sources, or configure an AI provider for synthesis.',
    ].join('\n');
  }
  const lines: string[] = ['**Offline extractive answer** — these are the most relevant passages from your selected sources:', ''];
  retrieval.evidence.slice(0, 5).forEach((item) => {
    const sentences = item.quote.split(/(?<=[.!?])\s+/).slice(0, 2).join(' ');
    const source = retrieval.hits.find((h) => h.chunk.id === item.chunkId)?.source;
    const link = source?.origin.url ? ` ([open source](${source.origin.url}))` : '';
    lines.push(`- ${sentences} [${item.marker}] — *${item.sourceTitle}*, ${item.locator}${link}`);
  });
  lines.push('');
  if (retrieval.conflicts.length) {
    lines.push('**Conflicting evidence detected.** The sources state different figures:');
    retrieval.conflicts.forEach((c) => lines.push(`- ${c.note}`));
    lines.push('');
  }
  lines.push('_Offline mode quotes and ranks evidence; it does not write new prose. Configure an AI provider in Settings for a synthesised, cited answer._');
  return lines.join('\n');
}

function offlineCitations(retrieval: RetrievalResult): Citation[] {
  return retrieval.evidence.slice(0, 5).map((e) => ({
    marker: e.marker,
    chunkId: e.chunkId,
    sourceId: e.sourceId,
    sourceTitle: e.sourceTitle,
    quote: e.quote,
    locator: e.locator,
    score: e.score,
  }));
}

export async function ask(options: AskOptions): Promise<AskResult> {
  const retrieval = retrieveFor(options);
  const base: Omit<ChatMessage, 'content'> = {
    id: uid(),
    notebookId: options.notebookId,
    role: 'assistant',
    createdAt: Date.now(),
  };

  if (retrieval.insufficient || options.forceLocal) {
    const content = localAnswer(retrieval);
    return {
      retrieval,
      message: {
        ...base,
        content,
        citations: offlineCitations(retrieval),
        insufficientEvidence: retrieval.insufficient,
        conflicting: retrieval.conflicts.length > 0,
        mode: 'local',
      },
    };
  }

  const profile = providerForTask(options.settings, 'chat');
  if (!profile) {
    return {
      retrieval,
      message: {
        ...base,
        content: localAnswer(retrieval),
        citations: offlineCitations(retrieval),
        conflicting: retrieval.conflicts.length > 0,
        mode: 'local',
      },
    };
  }

  const messages = buildResearchMessages({
    question: options.question,
    retrieval,
    history: options.history ?? [],
  });
  const model = resolveModel(profile, 'chat');
  try {
    const result = await runChat(profile, {
      task: 'chat',
      request: {
        messages,
        model,
        temperature: 0.2,
        maxTokens: 1800,
        signal: options.signal,
      },
      ...(options.onDelta ? { onDelta: options.onDelta } : {}),
    });
    const validation = validateAnswerCitations(result.text, retrieval.evidence);
    const content = validation.cleanedText || result.text;
    const insufficient = /insufficient evidence/i.test(content.slice(0, 400));
    const notes: string[] = [];
    if (validation.invalidMarkers.length) {
      notes.push(`Removed ${validation.invalidMarkers.length} citation marker(s) that did not match retrieved evidence.`);
    }
    return {
      retrieval,
      message: {
        ...base,
        content,
        citations: validation.citations,
        insufficientEvidence: insufficient,
        conflicting: retrieval.conflicts.length > 0,
        model: result.model,
        providerId: profile.id,
        mode: 'ai',
        ...(notes.length ? { error: notes.join(' ') } : {}),
      },
    };
  } catch (err) {
    const message = err instanceof ProviderError ? `${err.code}: ${err.message}` : err instanceof Error ? err.message : String(err);
    return {
      retrieval,
      message: {
        ...base,
        content: `**AI request failed.** ${message}\n\nFalling back to the evidence extract so your research is not blocked:`,
        citations: offlineCitations(retrieval),
        conflicting: retrieval.conflicts.length > 0,
        mode: 'local',
        error: message,
        providerId: profile.id,
        model,
      },
    };
  }
}

export function questionMessage(notebookId: string, content: string): ChatMessage {
  return { id: uid(), notebookId, role: 'user', content, createdAt: Date.now() };
}

export function evidenceLocator(_source: Source, chunk: Chunk): string {
  return locatorFor(chunk);
}

export function contextsForSources(sources: Source[]): string {
  return sources.map((s) => `${s.title} (${s.kind}): ${s.statusDetail}`).join('\n');
}
