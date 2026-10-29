import { useState, type ReactNode } from 'react';
import { actions, activeSourceIds, notebookChunks, notebookSources, useAppState } from '../lib/store';
import type { Source } from '../lib/types';
import { ConfirmDialog, EmptyState, StatusBadge } from './ui';
import { formatClock } from '../lib/text';

export function SourcePanel(props: {
  notebookId: string;
  onAddSource: () => void;
  onOpenSource: (sourceId: string, chunkId?: string) => void;
}): ReactNode {
  const state = useAppState();
  const sources = notebookSources(state, props.notebookId);
  const chunks = notebookChunks(state, props.notebookId);
  const selected = activeSourceIds(state, props.notebookId);
  const [renaming, setRenaming] = useState<{ id: string; value: string } | null>(null);
  const [pendingRemove, setPendingRemove] = useState<Source | null>(null);

  const isSelected = (id: string) => selected === null || selected.includes(id);

  if (!sources.length) {
    return (
      <EmptyState
        icon="📥"
        title="No sources yet"
        action={
          <button type="button" className="btn btn--primary" onClick={props.onAddSource}>
            Add your first source
          </button>
        }
      >
        <p>
          Import PDFs, DOCX, PPTX, Markdown, CSV, source code, GitHub files, web pages or transcripts. Everything is parsed and indexed
          locally — nothing is uploaded.
        </p>
      </EmptyState>
    );
  }

  return (
    <div>
      <div className="row" style={{ marginBottom: '0.75rem' }}>
        <div className="small muted">
          {sources.length} source{sources.length === 1 ? '' : 's'} · {chunks.length} indexed passage{chunks.length === 1 ? '' : 's'}
        </div>
        <div className="spacer" />
        <div className="tiny faint">
          {selected === null ? 'All sources used for retrieval' : `${selected.length} of ${sources.length} selected`}
        </div>
        <button type="button" className="btn btn--sm" onClick={() => actions.setSelection(props.notebookId, null)} disabled={selected === null}>
          Select all
        </button>
        <button type="button" className="btn btn--sm" onClick={() => actions.setSelection(props.notebookId, [])} disabled={selected?.length === 0}>
          Select none
        </button>
      </div>

      {selected?.length === 0 ? (
        <div className="notice notice--warning">
          <strong>No sources selected</strong>
          Retrieval and study generation need at least one selected source. Select notebook sources above.
        </div>
      ) : null}

      <div className="stack">
        {sources.map((source) => {
          const sourceChunks = chunks.filter((c) => c.sourceId === source.id);
          const timed = source.segments.some((s) => s.startMs !== undefined);
          return (
            <article key={source.id} className="card">
              <div className="source-row">
                <input
                  type="checkbox"
                  checked={isSelected(source.id)}
                  onChange={() => actions.toggleSourceSelection(props.notebookId, source.id)}
                  aria-label={`Use ${source.title} for retrieval`}
                />
                <div className="source-row__body">
                  <div className="card__head">
                    <div style={{ minWidth: 0 }}>
                      {renaming?.id === source.id ? (
                        <form
                          onSubmit={(event) => {
                            event.preventDefault();
                            void actions.renameSource(source.id, renaming.value);
                            setRenaming(null);
                          }}
                          className="row"
                        >
                          <input
                            className="input"
                            value={renaming.value}
                            onChange={(event) => setRenaming({ id: source.id, value: event.target.value })}
                            aria-label="Source title"
                            autoFocus
                          />
                          <button type="submit" className="btn btn--primary btn--sm">
                            Save
                          </button>
                          <button type="button" className="btn btn--sm" onClick={() => setRenaming(null)}>
                            Cancel
                          </button>
                        </form>
                      ) : (
                        <div className="row" style={{ gap: '0.4rem' }}>
                          <span className="source-row__title">{source.title}</span>
                          <StatusBadge status={source.status} />
                          <span className="badge">{source.kind}</span>
                        </div>
                      )}
                      <div className="card__meta">
                        {source.text.length.toLocaleString()} chars · {sourceChunks.length} passages
                        {timed ? ` · ${formatClock(source.segments[source.segments.length - 1]?.endMs ?? 0)} transcript` : ''}
                        {source.origin.repo ? ` · ${source.origin.repo}${source.origin.path ? `/${source.origin.path}` : ''}` : ''}
                      </div>
                    </div>
                    <div className="btn-row">
                      <button type="button" className="btn btn--sm" onClick={() => props.onOpenSource(source.id)}>
                        Inspect
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm"
                        onClick={() => setRenaming({ id: source.id, value: source.title })}
                        aria-label={`Rename ${source.title}`}
                      >
                        Rename
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm btn--danger"
                        onClick={() => setPendingRemove(source)}
                        aria-label={`Remove ${source.title}`}
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                  <p className="small muted clamp-2" style={{ margin: '0.45rem 0 0' }}>
                    {source.statusDetail}
                  </p>
                  <p className="small faint clamp-2" style={{ margin: '0.2rem 0 0' }}>
                    {source.text.slice(0, 260).replace(/\s+/g, ' ')}
                  </p>
                  {source.warnings.length ? (
                    <ul className="warnings">
                      {source.warnings.slice(0, 3).map((w) => (
                        <li key={w}>{w}</li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              </div>
            </article>
          );
        })}
      </div>

      {pendingRemove ? (
        <ConfirmDialog
          title="Remove source?"
          message={`“${pendingRemove.title}” and its indexed passages will be deleted from this notebook.`}
          confirmLabel="Remove source"
          danger
          onCancel={() => setPendingRemove(null)}
          onConfirm={() => {
            void actions.removeSource(pendingRemove.id);
            setPendingRemove(null);
          }}
        />
      ) : null}
    </div>
  );
}
