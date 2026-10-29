import { useEffect, useRef, useState, type ReactNode } from 'react';
import { actions, activeSourceIds, notebookChunks, notebookMessages, notebookSources, useAppState } from '../lib/store';
import { ask, questionMessage } from '../lib/research';
import { providerForTask } from '../lib/providers';
import { downloadText, slugify } from '../lib/download';
import type { ChatMessage, RetrievalResult } from '../lib/types';
import { EmptyState, MarkdownView, Spinner } from './ui';
import type { SourceViewerTarget } from './SourceViewer';

export function ChatPanel(props: {
  notebookId: string;
  notebookTitle: string;
  onOpenSource: (target: SourceViewerTarget) => void;
  onAddSource: () => void;
}): ReactNode {
  const state = useAppState();
  const messages = notebookMessages(state, props.notebookId);
  const sources = notebookSources(state, props.notebookId);
  const chunks = notebookChunks(state, props.notebookId);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState<{ text: string; question: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [lastRetrieval, setLastRetrieval] = useState<RetrievalResult | null>(null);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const chatProvider = providerForTask(state.settings, 'chat');
  const selected = activeSourceIds(state, props.notebookId);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end', behavior: 'smooth' });
  }, [messages.length, streaming?.text]);

  const send = async () => {
    const question = draft.trim();
    if (!question || busy) return;
    if (!chunks.length) {
      actions.toast('warning', 'Add at least one source before asking questions.');
      return;
    }
    if (selected && selected.length === 0) {
      actions.toast('warning', 'No sources are selected. Select sources to retrieve evidence from.');
      return;
    }
    setDraft('');
    setBusy(true);
    actions.setBusy({ thinking: true, label: 'Retrieving evidence…' });
    const userMessage = questionMessage(props.notebookId, question);
    await actions.appendMessage(userMessage);
    setStreaming({ text: '', question });
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const history = messages.filter((m) => m.role !== 'system');
      const result = await ask({
        question,
        notebookId: props.notebookId,
        chunks,
        sources,
        settings: state.settings,
        history,
        activeSourceIds: selected,
        signal: controller.signal,
        onDelta: (delta) =>
          setStreaming((current) => (current ? { ...current, text: current.text + delta } : { text: delta, question })),
      });
      setLastRetrieval(result.retrieval);
      await actions.appendMessage(result.message);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      const fallback: ChatMessage = {
        id: `msg-error-${Date.now()}`,
        notebookId: props.notebookId,
        role: 'assistant',
        content: `**The request failed.** ${message}`,
        createdAt: Date.now(),
        error: message,
        mode: 'local',
      };
      await actions.appendMessage(fallback);
      actions.toast('error', 'Research request failed', message);
    } finally {
      abortRef.current = null;
      setStreaming(null);
      setBusy(false);
      actions.setBusy({ thinking: false, label: '' });
    }
  };

  const cancel = () => {
    abortRef.current?.abort();
    setStreaming(null);
    setBusy(false);
    actions.setBusy({ thinking: false, label: '' });
    actions.toast('info', 'Request cancelled.');
  };

  const openCitation = (citation: NonNullable<ChatMessage['citations']>[number]) => {
    props.onOpenSource({
      sourceId: citation.sourceId,
      chunkId: citation.chunkId,
      citation: { quote: citation.quote, locator: citation.locator, marker: citation.marker },
    });
  };

  const exportConversation = () => {
    const body = messages
      .map((m) => {
        const who = m.role === 'user' ? 'You' : `Assistant${m.model ? ` (${m.model})` : ''}`;
        const cites = m.citations?.length ? `\n\nCitations: ${m.citations.map((c) => `[${c.marker}] ${c.sourceTitle} — ${c.locator}`).join('; ')}` : '';
        return `### ${who}\n\n${m.content}${cites}`;
      })
      .join('\n\n');
    downloadText(`# ${props.notebookTitle} — research conversation\n\n${body}\n`, `${slugify(props.notebookTitle)}-conversation.md`, 'text/markdown;charset=utf-8');
    actions.toast('success', 'Conversation exported as Markdown.');
  };

  if (!sources.length) {
    return (
      <EmptyState
        icon="💬"
        title="Ask questions once you have sources"
        action={
          <button type="button" className="btn btn--primary" onClick={props.onAddSource}>
            Add a source
          </button>
        }
      >
        <p>
          Answers are grounded in your selected sources, with inline citations that open the exact passage. Without an AI provider you get an
          offline extractive answer instead of a synthesised one.
        </p>
      </EmptyState>
    );
  }

  const suggested = [
    `What are the main themes across these sources?`,
    `Summarise the key facts with citations.`,
    `What do the sources disagree about?`,
  ];

  return (
    <div className="chat">
      {!chatProvider ? (
        <div className="notice notice--info">
          <strong>Offline research mode</strong>
          No AI provider is configured, so answers are extractive: the most relevant passages with resolved citations. Retrieval, citations,
          search, study tools and exports all work locally. Configure a provider in{' '}
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => actions.setView('settings')}>
            Settings
          </button>{' '}
          for synthesised answers.
        </div>
      ) : null}

      {messages.length === 0 && !streaming ? (
        <div>
          <EmptyState icon="🧭" title="Start a grounded conversation">
            <p>
              Ask anything about the {sources.length} source{sources.length === 1 ? '' : 's'} in this notebook. Every answer cites the passages
              it used, and unsupported questions are reported as insufficient evidence rather than guessed.
            </p>
            <div className="btn-row" style={{ justifyContent: 'center' }}>
              {suggested.map((question) => (
                <button key={question} type="button" className="btn btn--sm" onClick={() => setDraft(question)}>
                  {question}
                </button>
              ))}
            </div>
          </EmptyState>
        </div>
      ) : null}

      {messages.map((message) => (
        <article key={message.id} className={`msg msg--${message.role}`}>
          <div className="msg__avatar" aria-hidden="true">
            {message.role === 'user' ? 'You' : 'SF'}
          </div>
          <div className="msg__body">
            {message.role === 'assistant' ? (
              <MarkdownView markdown={message.content} onCitation={(marker) => {
                const citation = message.citations?.find((c) => c.marker === marker);
                if (citation) openCitation(citation);
                else actions.toast('warning', `Citation [${marker}] is not in the validated evidence for this answer.`);
              }} />
            ) : (
              <div className="msg__content">{message.content}</div>
            )}

            {message.insufficientEvidence ? (
              <div className="notice notice--warning" style={{ marginTop: '0.5rem' }}>
                <strong>Insufficient evidence</strong>
                The selected sources do not support a confident answer to this question.
              </div>
            ) : null}

            {message.conflicting ? (
              <div className="notice notice--warning" style={{ marginTop: '0.5rem' }}>
                <strong>Sources may disagree</strong>
                Conflicting figures were detected between sources. Check each cited passage before relying on either figure.
              </div>
            ) : null}

            {message.citations?.length ? (
              <div className="cite-list">
                {message.citations.map((citation) => (
                  <button
                    key={`${citation.marker}-${citation.chunkId}`}
                    type="button"
                    className="cite-chip"
                    onClick={() => openCitation(citation)}
                    title={`${citation.sourceTitle} · ${citation.locator}\n${citation.quote}`}
                  >
                    [{citation.marker}] {citation.sourceTitle} · {citation.locator}
                  </button>
                ))}
              </div>
            ) : null}

            <div className="msg__meta">
              <span>{new Date(message.createdAt).toLocaleTimeString()}</span>
              {message.mode === 'ai' ? (
                <span className="badge badge--accent">AI · {message.model ?? 'model'}</span>
              ) : (
                <span className="badge">offline extract</span>
              )}
              {message.error ? <span style={{ color: 'var(--danger)' }}>{message.error}</span> : null}
            </div>
            {message.role === 'assistant' ? (
              <div className="msg__actions">
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  onClick={() => {
                    void navigator.clipboard?.writeText(message.content);
                    actions.toast('success', 'Answer copied.');
                  }}
                >
                  Copy
                </button>
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  onClick={() => {
                    downloadText(message.content, `answer-${message.id.slice(-6)}.md`, 'text/markdown;charset=utf-8');
                  }}
                >
                  Download
                </button>
              </div>
            ) : null}
          </div>
        </article>
      ))}

      {streaming ? (
        <article className="msg msg--assistant">
          <div className="msg__avatar" aria-hidden="true">SF</div>
          <div className="msg__body">
            {streaming.text ? <MarkdownView markdown={streaming.text} /> : <Spinner label="Retrieving evidence and drafting…" />}
            <span className="typing" aria-hidden="true" />
            <div className="msg__actions">
              <button type="button" className="btn btn--sm" onClick={cancel}>
                Stop
              </button>
            </div>
          </div>
        </article>
      ) : null}

      <div ref={bottomRef} />

      {lastRetrieval ? (
        <details className="card" onToggle={(event) => setShowDiagnostics((event.currentTarget as HTMLDetailsElement).open)}>
          <summary className="small" style={{ cursor: 'pointer' }}>
            Retrieval diagnostics — {lastRetrieval.diagnostics.candidateChunks} chunks scanned, {lastRetrieval.evidence.length} cited
            {showDiagnostics ? '' : ' (click to expand)'}
          </summary>
          <div className="small muted" style={{ marginTop: '0.5rem' }}>
            <p>{lastRetrieval.reason}</p>
            <ul>
              {lastRetrieval.evidence.map((e) => (
                <li key={e.chunkId}>
                  [{e.marker}] score {e.score.toFixed(3)} · {e.sourceTitle} · {e.locator}
                </li>
              ))}
            </ul>
            {lastRetrieval.conflicts.length ? <p>Conflicts detected: {lastRetrieval.conflicts.length}</p> : null}
          </div>
        </details>
      ) : null}

      <div className="composer">
        <div className="composer__box">
          <label htmlFor="chat-input" className="sr-only">
            Ask a question about your sources
          </label>
          <textarea
            id="chat-input"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                void send();
              }
            }}
            placeholder="Ask about your sources… (Enter to send, Shift+Enter for a new line)"
            rows={2}
            disabled={busy}
          />
          {busy ? (
            <button type="button" className="btn" onClick={cancel}>
              Stop
            </button>
          ) : (
            <button type="button" className="btn btn--primary" onClick={() => void send()} disabled={!draft.trim()}>
              Ask
            </button>
          )}
        </div>
        <div className="composer__hint">
          <span>
            {chatProvider ? `Answers via ${chatProvider.label}` : 'Offline extractive answers'} · {selected === null ? 'all sources' : `${selected.length} selected`}
          </span>
          <span className="row" style={{ gap: '0.5rem' }}>
            <button type="button" className="btn btn--ghost btn--sm" onClick={exportConversation} disabled={!messages.length}>
              Export chat
            </button>
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => {
                void actions.clearConversation(props.notebookId);
                setLastRetrieval(null);
              }}
              disabled={!messages.length}
            >
              Clear
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}
