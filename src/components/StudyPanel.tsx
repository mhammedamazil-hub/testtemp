import { useState, type ReactNode } from 'react';
import { actions, activeSourceIds, notebookArtifacts, notebookChunks, notebookSources, useAppState } from '../lib/store';
import { generateArtifact, type Flashcard, type QuizQuestion, type GlossaryTerm, type TimelineEvent, type ConceptNode, type ComparisonTable } from '../lib/study';
import { providerForTask } from '../lib/providers';
import type { Artifact, ArtifactKind } from '../lib/types';
import { EmptyState, MarkdownView, Spinner } from './ui';
import { exportArtifactCsv, exportArtifactMarkdownFile, exportHtmlReport, exportPdfReport, exportPptxDeck } from '../lib/exports/actions';
import { generateAudioOverview, transcriptWithTimestamps, packAudioParts, type AudioOverviewResult } from '../lib/audio';
import { downloadBlob, downloadText, slugify } from '../lib/download';

const KINDS: { kind: ArtifactKind; label: string; description: string }[] = [
  { kind: 'overview', label: 'Overview', description: 'Themes and coverage' },
  { kind: 'study-guide', label: 'Study guide', description: 'Structured revision' },
  { kind: 'flashcards', label: 'Flashcards', description: 'Reveal + self-check' },
  { kind: 'quiz', label: 'Quiz', description: 'MCQ with explanations' },
  { kind: 'faq', label: 'FAQ', description: 'Common questions' },
  { kind: 'timeline', label: 'Timeline', description: 'Dated events' },
  { kind: 'comparison', label: 'Comparison', description: 'Source comparison table' },
  { kind: 'glossary', label: 'Glossary', description: 'Key terms' },
  { kind: 'quotes', label: 'Quotations', description: 'Verbatim evidence' },
  { kind: 'concept-map', label: 'Concept map', description: 'Nodes and relations' },
  { kind: 'notes', label: 'Notes', description: 'Structured notes' },
  { kind: 'audio-script', label: 'Audio script', description: 'Two-host dialogue' },
  { kind: 'storyboard', label: 'Storyboard', description: 'Video plan + captions' },
];

export function StudyPanel(props: { notebookId: string; notebookTitle: string; onAddSource: () => void }): ReactNode {
  const state = useAppState();
  const sources = notebookSources(state, props.notebookId);
  const chunks = notebookChunks(state, props.notebookId);
  const artifacts = notebookArtifacts(state, props.notebookId);
  const [busyKind, setBusyKind] = useState<ArtifactKind | null>(null);
  const [topic, setTopic] = useState('');
  const [count, setCount] = useState(10);
  const [error, setError] = useState<string | null>(null);
  const [openArtifactId, setOpenArtifactId] = useState<string | null>(null);
  const [forceLocal, setForceLocal] = useState(false);

  const studyProvider = providerForTask(state.settings, 'study');
  const selected = activeSourceIds(state, props.notebookId);
  const openArtifact = artifacts.find((a) => a.id === openArtifactId) ?? null;

  const generate = async (kind: ArtifactKind) => {
    if (!sources.length) {
      props.onAddSource();
      return;
    }
    setBusyKind(kind);
    setError(null);
    actions.setBusy({ generating: kind, label: `Generating ${kind.replace('-', ' ')}…` });
    try {
      const result = await generateArtifact(
        kind,
        {
          notebookId: props.notebookId,
          notebookTitle: props.notebookTitle,
          sources,
          chunks,
          settings: state.settings,
          activeSourceIds: selected,
        },
        { count, ...(topic.trim() ? { topic: topic.trim() } : {}), forceLocal },
      );
      await actions.saveArtifact(result.artifact);
      setOpenArtifactId(result.artifact.id);
      if (result.warning) actions.toast('warning', result.warning);
      else
        actions.toast(
          'success',
          `${result.artifact.mode === 'ai' ? 'AI-generated' : 'Offline-generated'} ${kind.replace('-', ' ')} ready.`,
          result.artifact.mode === 'local' && studyProvider
            ? 'Fell back to deterministic generation.'
            : undefined,
        );
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      actions.toast('error', `Could not generate ${kind}: ${message}`);
    } finally {
      setBusyKind(null);
      actions.setBusy({ generating: null, label: '' });
    }
  };

  if (!sources.length) {
    return (
      <EmptyState
        icon="🧪"
        title="Study tools need sources"
        action={
          <button type="button" className="btn btn--primary" onClick={props.onAddSource}>
            Add a source
          </button>
        }
      >
        <p>Once your notebook has sources, SourceForge can build study guides, flashcards, quizzes, glossaries and reports — offline or with AI.</p>
      </EmptyState>
    );
  }

  return (
    <div className="stack">
      <div className="card">
        <div className="card__head">
          <div>
            <div className="card__title">Generate study material</div>
            <div className="card__meta">
              {studyProvider && !forceLocal ? `AI via ${studyProvider.label}` : 'Deterministic offline generation'} · {selected === null ? `${sources.length} sources` : `${selected.length} selected`}
            </div>
          </div>
          <div className="row">
            <label className="checkbox" style={{ margin: 0 }}>
              <input type="checkbox" checked={forceLocal} onChange={(event) => setForceLocal(event.target.checked)} />
              <span className="small">Force offline</span>
            </label>
          </div>
        </div>
        <div className="row" style={{ marginTop: '0.7rem' }}>
          <div style={{ flex: 1, minWidth: '12rem' }}>
            <label htmlFor="study-topic">Focus topic (optional)</label>
            <input id="study-topic" className="input" value={topic} onChange={(event) => setTopic(event.target.value)} placeholder="e.g. pricing strategy" />
          </div>
          <div style={{ width: '8rem' }}>
            <label htmlFor="study-count">Items</label>
            <input
              id="study-count"
              className="input"
              type="number"
              min={3}
              max={30}
              value={count}
              onChange={(event) => setCount(Math.max(3, Math.min(30, Number(event.target.value) || 10)))}
            />
          </div>
        </div>
        <div className="kind-grid" style={{ marginTop: '0.7rem' }}>
          {KINDS.map((item) => (
            <button key={item.kind} type="button" className="kind-btn" onClick={() => void generate(item.kind)} disabled={Boolean(busyKind)}>
              <strong>{busyKind === item.kind ? 'Generating…' : item.label}</strong>
              <span>{item.description}</span>
            </button>
          ))}
        </div>
        {error ? (
          <div className="notice notice--error" role="alert" style={{ marginTop: '0.6rem' }}>
            <strong>Generation failed</strong>
            {error}
          </div>
        ) : null}
        {busyKind ? (
          <div style={{ marginTop: '0.6rem' }}>
            <Spinner label={`Building ${busyKind.replace('-', ' ')}…`} />
          </div>
        ) : null}
      </div>

      <AudioOverviewPanel notebookId={props.notebookId} notebookTitle={props.notebookTitle} />

      <div className="card">
        <div className="card__title">Notebook exports</div>
        <p className="card__meta">Real files generated in your browser, then validated before download.</p>
        <div className="btn-row" style={{ marginTop: '0.6rem' }}>
          <button type="button" className="btn btn--sm" onClick={() => actions.setView('settings')}>
            JSON backup (Settings)
          </button>
          <button type="button" className="btn btn--sm" onClick={() => exportPdfReport(props.notebookId)}>
            PDF report
          </button>
          <button type="button" className="btn btn--sm" onClick={() => exportPptxDeck(props.notebookId)}>
            PPTX deck
          </button>
          <button type="button" className="btn btn--sm" onClick={() => exportHtmlReport(props.notebookId)}>
            HTML report
          </button>
        </div>
      </div>

      <div>
        <h3 style={{ marginTop: '0.5rem' }}>Study library ({artifacts.length})</h3>
        {!artifacts.length ? (
          <p className="small faint">Nothing generated yet. Pick a type above.</p>
        ) : (
          <div className="stack">
            {artifacts.map((artifact) => (
              <article key={artifact.id} className="card">
                <div className="card__head">
                  <div>
                    <div className="card__title">{artifact.title}</div>
                    <div className="card__meta">
                      {artifact.mode === 'ai' ? `AI · ${artifact.model ?? 'model'}` : 'offline deterministic'} · {artifact.citations.length} citation(s) ·{' '}
                      {new Date(artifact.createdAt).toLocaleString()}
                    </div>
                  </div>
                  <div className="btn-row">
                    <button
                      type="button"
                      className="btn btn--sm"
                      onClick={() => setOpenArtifactId(openArtifactId === artifact.id ? null : artifact.id)}
                      aria-expanded={openArtifactId === artifact.id}
                    >
                      {openArtifactId === artifact.id ? 'Hide' : 'View'}
                    </button>
                    <button type="button" className="btn btn--sm" onClick={() => exportArtifactMarkdownFile(artifact)}>
                      Markdown
                    </button>
                    {artifact.kind === 'flashcards' || artifact.kind === 'quiz' ? (
                      <button type="button" className="btn btn--sm" onClick={() => exportArtifactCsv(artifact)}>
                        CSV
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="btn btn--sm btn--danger"
                      onClick={() => {
                        void actions.deleteArtifact(artifact.id);
                        if (openArtifactId === artifact.id) setOpenArtifactId(null);
                      }}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                {openArtifactId === artifact.id ? <ArtifactView artifact={artifact} /> : null}
              </article>
            ))}
          </div>
        )}
      </div>
      {openArtifact ? null : null}
    </div>
  );
}

function ArtifactView({ artifact }: { artifact: Artifact }): ReactNode {
  return (
    <div style={{ marginTop: '0.8rem' }}>
      {artifact.kind === 'flashcards' ? <Flashcards artifact={artifact} /> : null}
      {artifact.kind === 'quiz' ? <Quiz artifact={artifact} /> : null}
      {artifact.kind === 'glossary' ? <Glossary artifact={artifact} /> : null}
      {artifact.kind === 'timeline' ? <Timeline artifact={artifact} /> : null}
      {artifact.kind === 'concept-map' ? <ConceptMap artifact={artifact} /> : null}
      {artifact.kind === 'comparison' ? <Comparison artifact={artifact} /> : null}
      <details style={{ marginTop: '0.6rem' }}>
        <summary className="small muted" style={{ cursor: 'pointer' }}>
          Full Markdown
        </summary>
        <div className="artifact-preview" style={{ marginTop: '0.4rem' }}>
          <MarkdownView markdown={artifact.body} />
        </div>
      </details>
      {artifact.citations.length ? (
        <details style={{ marginTop: '0.5rem' }}>
          <summary className="small muted" style={{ cursor: 'pointer' }}>
            Citations ({artifact.citations.length})
          </summary>
          <ul className="citations small">
            {artifact.citations.map((c) => (
              <li key={`${c.marker}-${c.chunkId}`}>
                [{c.marker}] {c.sourceTitle} · {c.locator}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  );
}

function Flashcards({ artifact }: { artifact: Artifact }): ReactNode {
  const cards = (artifact.data as { cards?: Flashcard[] } | undefined)?.cards ?? [];
  const [revealed, setRevealed] = useState<Set<number>>(new Set());
  const [known, setKnown] = useState<Record<number, 'known' | 'review'>>({});
  if (!cards.length) return <p className="small faint">No cards in this artifact.</p>;
  return (
    <div>
      <div className="tiny faint" style={{ marginBottom: '0.4rem' }}>
        {Object.values(known).filter((v) => v === 'known').length} known · {Object.values(known).filter((v) => v === 'review').length} to review
      </div>
      {cards.map((card, index) => (
        <div key={`${card.front}-${index}`} className="flashcard">
          <div className="flashcard__front">{card.front}</div>
          {revealed.has(index) ? (
            <>
              <div className="flashcard__back">{card.back}</div>
              <div className="cite-list">
                {card.citation.map((marker) => (
                  <span key={marker} className="badge badge--accent">
                    [{marker}]
                  </span>
                ))}
              </div>
            </>
          ) : null}
          <div className="btn-row" style={{ marginTop: '0.5rem' }}>
            <button
              type="button"
              className="btn btn--sm"
              onClick={() =>
                setRevealed((current) => {
                  const next = new Set(current);
                  if (next.has(index)) next.delete(index);
                  else next.add(index);
                  return next;
                })
              }
            >
              {revealed.has(index) ? 'Hide answer' : 'Reveal'}
            </button>
            <button type="button" className="btn btn--sm" onClick={() => setKnown((k) => ({ ...k, [index]: 'known' }))}>
              I knew this
            </button>
            <button type="button" className="btn btn--sm" onClick={() => setKnown((k) => ({ ...k, [index]: 'review' }))}>
              Review again
            </button>
            {known[index] ? <span className="badge">{known[index]}</span> : null}
          </div>
        </div>
      ))}
    </div>
  );
}

function Quiz({ artifact }: { artifact: Artifact }): ReactNode {
  const questions = (artifact.data as { questions?: QuizQuestion[] } | undefined)?.questions ?? [];
  const [answers, setAnswers] = useState<Record<number, number>>({});
  if (!questions.length) return <p className="small faint">No questions in this quiz.</p>;
  const correct = questions.filter((q, i) => answers[i] === q.answerIndex).length;
  return (
    <div>
      <div className="small muted">
        Answered {Object.keys(answers).length}/{questions.length} · correct {correct}
      </div>
      {questions.map((question, index) => (
        <div key={`${question.question}-${index}`} className="flashcard">
          <div className="flashcard__front">
            {index + 1}. {question.question}
          </div>
          <div style={{ marginTop: '0.5rem' }}>
            {question.options.map((option, optionIndex) => {
              const chosen = answers[index] === optionIndex;
              const isCorrect = optionIndex === question.answerIndex;
              const className = chosen
                ? isCorrect
                  ? 'quiz-option quiz-option--correct'
                  : 'quiz-option quiz-option--wrong'
                : answers[index] !== undefined && isCorrect
                  ? 'quiz-option quiz-option--correct'
                  : 'quiz-option';
              return (
                <button
                  key={`${option}-${optionIndex}`}
                  type="button"
                  className={className}
                  onClick={() => setAnswers((current) => ({ ...current, [index]: optionIndex }))}
                >
                  <span className="mono small">{String.fromCharCode(65 + optionIndex)}</span>
                  <span>{option}</span>
                </button>
              );
            })}
          </div>
          {answers[index] !== undefined ? (
            <div className="small muted" style={{ marginTop: '0.4rem' }}>
              {answers[index] === question.answerIndex ? 'Correct. ' : `Incorrect — the answer is ${String.fromCharCode(65 + question.answerIndex)}. `}
              {question.explanation}
              {question.citation.map((marker) => (
                <span key={marker} className="badge badge--accent" style={{ marginLeft: '0.3rem' }}>
                  [{marker}]
                </span>
              ))}
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

function Glossary({ artifact }: { artifact: Artifact }): ReactNode {
  const terms = (artifact.data as { terms?: GlossaryTerm[] } | undefined)?.terms ?? [];
  return (
    <dl className="kv">
      {terms.map((term) => (
        <>
          <dt key={`${term.term}-dt`} style={{ fontWeight: 600 }}>
            {term.term} {term.citation.map((m) => `[${m}]`).join(' ')}
          </dt>
          <dd key={`${term.term}-dd`}>{term.definition}</dd>
        </>
      ))}
    </dl>
  );
}

function Timeline({ artifact }: { artifact: Artifact }): ReactNode {
  const events = (artifact.data as { events?: TimelineEvent[] } | undefined)?.events ?? [];
  if (!events.length) return <p className="small faint">No dated events were found in the evidence.</p>;
  return (
    <ol>
      {events.map((event, index) => (
        <li key={`${event.date}-${index}`}>
          <strong>{event.date}</strong> — {event.label} {event.citation.map((m) => `[${m}]`).join(' ')}
        </li>
      ))}
    </ol>
  );
}

function ConceptMap({ artifact }: { artifact: Artifact }): ReactNode {
  const data = artifact.data as { nodes?: ConceptNode[]; edges?: { from: string; to: string; label: string }[] } | undefined;
  if (!data?.nodes?.length) return <p className="small faint">No concept map data.</p>;
  return (
    <div className="small">
      <div className="row" style={{ gap: '0.4rem' }}>
        {data.nodes.map((node) => (
          <span key={node.id} className="badge badge--accent">
            {node.label}
          </span>
        ))}
      </div>
      <ul style={{ marginTop: '0.5rem' }}>
        {(data.edges ?? []).map((edge, index) => {
          const from = data.nodes?.find((n) => n.id === edge.from)?.label ?? edge.from;
          const to = data.nodes?.find((n) => n.id === edge.to)?.label ?? edge.to;
          return <li key={index}>{from} → {to} ({edge.label})</li>;
        })}
      </ul>
      <p className="tiny faint">A Mermaid graph is included in the Markdown export.</p>
    </div>
  );
}

function Comparison({ artifact }: { artifact: Artifact }): ReactNode {
  const table = artifact.data as ComparisonTable | undefined;
  if (!table?.columns?.length) return <p className="small faint">No comparison table data.</p>;
  return (
    <table>
      <thead>
        <tr>
          {table.columns.map((column) => (
            <th key={column}>{column}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {table.rows.map((row, index) => (
          <tr key={index}>
            {row.map((cell, cellIndex) => (
              <td key={cellIndex}>{cell}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function AudioOverviewPanel(props: { notebookId: string; notebookTitle: string }): ReactNode {
  const state = useAppState();
  const sources = notebookSources(state, props.notebookId);
  const chunks = notebookChunks(state, props.notebookId);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState('');
  const [result, setResult] = useState<AudioOverviewResult | null>(null);
  const ttsProvider = providerForTask(state.settings, 'tts');
  const selected = activeSourceIds(state, props.notebookId);

  const run = async () => {
    setBusy(true);
    setProgress('Building the source-grounded script…');
    actions.setBusy({ generating: 'audio', label: 'Creating audio overview…' });
    try {
      const output = await generateAudioOverview(
        {
          notebookId: props.notebookId,
          notebookTitle: props.notebookTitle,
          sources,
          chunks,
          settings: state.settings,
          activeSourceIds: selected,
        },
        { onProgress: (_done, _total, label) => setProgress(label) },
      );
      setResult(output);
      await actions.saveArtifact(output.script);
      if (output.available) actions.toast('success', `Audio overview ready (${output.parts.length} part(s)).`);
      else actions.toast('warning', 'Script ready, but speech synthesis is unavailable.', output.reason);
    } catch (err) {
      actions.toast('error', `Audio overview failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
      setProgress('');
      actions.setBusy({ generating: null, label: '' });
    }
  };

  return (
    <div className="card">
      <div className="card__head">
        <div>
          <div className="card__title">Audio overview</div>
          <div className="card__meta">
            {ttsProvider ? `Speech synthesis via ${ttsProvider.label}` : 'No TTS provider configured — script only'}
          </div>
        </div>
        <button type="button" className="btn btn--primary btn--sm" onClick={() => void run()} disabled={busy || !sources.length}>
          {busy ? 'Working…' : 'Generate audio overview'}
        </button>
      </div>

      {busy ? (
        <div style={{ marginTop: '0.6rem' }}>
          <Spinner label={progress || 'Working…'} />
        </div>
      ) : null}

      {result ? (
        <div style={{ marginTop: '0.8rem' }}>
          {result.available ? (
            <>
              <div className="notice notice--success">
                <strong>Audio generated</strong>
                {result.parts.length} part(s), about {Math.round(result.totalMs / 60000)} min. Each part is a real audio file from{' '}
                {result.providerId}.
              </div>
              {result.parts.map((part) => (
                <div key={part.index} className="audio-part">
                  <div className="small">
                    Part {part.index + 1} · starts ~{Math.round(part.startMs / 1000)}s · {(part.blob.size / 1024).toFixed(0)} KB
                  </div>
                  <audio controls src={part.url} preload="none">
                    <track kind="captions" />
                  </audio>
                  <div className="btn-row" style={{ marginTop: '0.35rem' }}>
                    <button
                      type="button"
                      className="btn btn--sm"
                      onClick={() => {
                        downloadBlob(part.blob, `${slugify(props.notebookTitle)}-part-${part.index + 1}.mp3`);
                      }}
                    >
                      Download part
                    </button>
                  </div>
                </div>
              ))}
              <div className="btn-row">
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={async () => downloadBlob(await packAudioParts(result, slugify(props.notebookTitle)), `${slugify(props.notebookTitle)}-audio.zip`)}
                >
                  Download all (zip)
                </button>
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => downloadText(transcriptWithTimestamps(result), `${slugify(props.notebookTitle)}-transcript.md`)}
                >
                  Transcript
                </button>
              </div>
            </>
          ) : (
            <div className="notice notice--warning">
              <strong>Speech synthesis unavailable</strong>
              {result.reason}
              <div className="btn-row" style={{ marginTop: '0.5rem' }}>
                <button type="button" className="btn btn--sm" onClick={() => downloadText(result.script.body, `${slugify(props.notebookTitle)}-audio-script.md`)}>
                  Download script
                </button>
                <button type="button" className="btn btn--sm" onClick={() => actions.setView('settings')}>
                  Configure TTS provider
                </button>
              </div>
            </div>
          )}
          <details>
            <summary className="small muted" style={{ cursor: 'pointer' }}>
              Show script
            </summary>
            <div className="artifact-preview" style={{ marginTop: '0.4rem' }}>
              <MarkdownView markdown={result.script.body} />
            </div>
          </details>
        </div>
      ) : (
        <p className="small faint" style={{ marginTop: '0.5rem' }}>
          Builds a citation-carrying two-host discussion from the selected sources, then renders it to audio when a TTS provider is configured.
          Without TTS you still get a downloadable, source-grounded script — never a fake audio player.
        </p>
      )}
    </div>
  );
}
