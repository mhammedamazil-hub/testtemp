import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { actions, searchWorkspace, useAppState } from '../lib/store';
import { SourceViewer, type SourceViewerTarget } from './SourceViewer';

export function SearchView(): ReactNode {
  const state = useAppState();
  const [query, setQuery] = useState('');
  const [target, setTarget] = useState<SourceViewerTarget | null>(null);
  const results = useMemo(() => searchWorkspace(state, query), [state, query]);

  useEffect(() => {
    document.getElementById('global-search')?.focus();
  }, []);

  return (
    <div className="container">
      <div className="row" style={{ marginBottom: '1rem' }}>
        <h1 style={{ margin: 0 }}>Search</h1>
        <div className="spacer" />
        <button type="button" className="btn btn--sm" onClick={() => actions.setView('notebook')}>
          Back to workspace
        </button>
      </div>

      <label htmlFor="global-search" className="sr-only">
        Search notebooks, source titles and source contents
      </label>
      <input
        id="global-search"
        className="input"
        placeholder="Search notebooks, source titles and full text…"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        style={{ fontSize: '1rem', padding: '0.7rem 0.9rem' }}
        aria-describedby="search-hint"
      />
      <div id="search-hint" className="hint">
        Searches locally across every notebook and the full text of every source. No network request.
      </div>

      {!query.trim() ? (
        <p className="muted small" style={{ marginTop: '1.5rem' }}>
          Type to search. Results show the notebook and a snippet from the matching source.
        </p>
      ) : (
        <div className="stack" style={{ marginTop: '1rem' }}>
          <section>
            <h2 className="small" style={{ textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--fg-faint)' }}>
              Notebooks ({results.notebooks.length})
            </h2>
            {results.notebooks.length === 0 ? (
              <p className="small faint">No notebook titles match.</p>
            ) : (
              <div className="stack" style={{ gap: '0.35rem' }}>
                {results.notebooks.map((notebook) => (
                  <button
                    key={notebook.id}
                    type="button"
                    className="nav-item"
                    onClick={() => {
                      actions.selectNotebook(notebook.id);
                    }}
                  >
                    <span aria-hidden="true">{notebook.emoji}</span>
                    <span className="notebook-item__main">
                      <span className="notebook-item__title">{notebook.title}</span>
                      <span className="notebook-item__meta">
                        {state.sources.filter((s) => s.notebookId === notebook.id).length} sources ·{' '}
                        {state.sources
                          .filter((s) => s.notebookId === notebook.id)
                          .slice(0, 3)
                          .map((s) => s.title)
                          .join(', ')}
                      </span>
                    </span>
                  </button>
                ))}
              </div>
            )}
          </section>

          <section>
            <h2 className="small" style={{ textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--fg-faint)' }}>
              Sources ({results.sources.length})
            </h2>
            {results.sources.length === 0 ? (
              <p className="small faint">No source titles or contents match.</p>
            ) : (
              <div className="stack">
                {results.sources.map(({ source, snippet }) => {
                  const notebook = state.notebooks.find((n) => n.id === source.notebookId);
                  return (
                    <div key={source.id} className="card">
                      <div className="row">
                        <strong className="small">{source.title}</strong>
                        <span className="badge">{source.kind}</span>
                        <span className="tiny faint">{notebook?.title}</span>
                        <div className="spacer" />
                        <button type="button" className="btn btn--sm" onClick={() => setTarget({ sourceId: source.id })}>
                          Open passage
                        </button>
                        <button
                          type="button"
                          className="btn btn--sm"
                          onClick={() => {
                            actions.selectNotebook(source.notebookId);
                          }}
                        >
                          Go to notebook
                        </button>
                      </div>
                      <div className="snippet">{snippet || '(match in title only)'}</div>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      )}

      {target ? <SourceViewer target={target} onClose={() => setTarget(null)} /> : null}
    </div>
  );
}
