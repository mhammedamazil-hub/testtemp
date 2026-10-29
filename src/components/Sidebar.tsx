import { useRef, useState, type ChangeEvent, type ReactNode } from 'react';
import { actions, store, useAppState } from '../lib/store';
import { exportBackupFile, importBackupFromFile } from '../lib/exports/actions';

export interface SidebarProps {
  open: boolean;
  onClose: () => void;
}

export function Sidebar({ open, onClose }: SidebarProps): ReactNode {
  const state = useAppState();
  const fileInput = useRef<HTMLInputElement | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const query = filter.trim().toLowerCase();
  const notebooks = state.notebooks
    .filter((notebook) => !query || notebook.title.toLowerCase().includes(query))
    .sort((a, b) => b.updatedAt - a.updatedAt);

  const onImport = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    await importBackupFromFile(file);
    if (open) onClose();
  };

  const sourceCount = (notebookId: string) =>
    state.sources.filter((source) => source.notebookId === notebookId).length;

  return (
    <aside className={open ? 'sidebar sidebar--open' : 'sidebar'} aria-label="Workspace">
      <div className="sidebar__brand">
        <div className="row">
          <span className="brand-mark" aria-hidden="true" />
          <div>
            <p className="brand-name">SourceForge</p>
            <p className="brand-sub">Cited answers, locally stored</p>
          </div>
          <div className="spacer" />
          <button type="button" className="btn btn--icon" onClick={onClose} aria-label="Close navigation">
            ✕
          </button>
        </div>
      </div>

      <div className="sidebar__actions">
        <button
          type="button"
          className="btn btn--primary btn--block"
          onClick={async () => {
            await actions.createNotebook();
            if (open) onClose();
          }}
        >
          + New notebook
        </button>
        <label htmlFor="notebook-filter" className="sr-only">
          Filter notebooks by title
        </label>
        <input
          id="notebook-filter"
          className="input"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder="Filter notebooks…"
        />
      </div>

      <div className="sidebar__scroll">
        <p className="hint" style={{ margin: '0 0 0.4rem' }}>
          Notebooks · {state.notebooks.length}
        </p>
        {notebooks.length === 0 && (
          <p className="small muted">
            {state.notebooks.length === 0 ? 'Create a notebook to begin.' : `No notebook matches “${filter}”.`}
          </p>
        )}
        <nav aria-label="Notebooks" className="stack" style={{ gap: '0.3rem' }}>
          {notebooks.map((notebook) => {
            const active = notebook.id === state.activeNotebookId && state.view === 'notebook';
            return (
              <div key={notebook.id} className={active ? 'notebook-item notebook-item--active' : 'notebook-item'}>
                {renaming === notebook.id ? (
                  <form
                    style={{ padding: '0.35rem' }}
                    onSubmit={async (event) => {
                      event.preventDefault();
                      const title = draft.trim();
                      if (title) await actions.updateNotebook(notebook.id, { title });
                      setRenaming(null);
                    }}
                  >
                    <input
                      className="input"
                      aria-label="Notebook title"
                      autoFocus
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Escape') setRenaming(null);
                      }}
                    />
                    <div className="btn-row" style={{ marginTop: '0.35rem' }}>
                      <button type="submit" className="btn btn--sm btn--primary">
                        Save
                      </button>
                      <button type="button" className="btn btn--sm" onClick={() => setRenaming(null)}>
                        Cancel
                      </button>
                    </div>
                  </form>
                ) : (
                  <>
                    <button
                      type="button"
                      className="notebook-item__main"
                      onClick={() => {
                        actions.selectNotebook(notebook.id);
                        if (open) onClose();
                      }}
                      aria-current={active ? 'page' : undefined}
                    >
                      <span aria-hidden="true">{notebook.emoji}</span>
                      <span className="notebook-item__title">{notebook.title}</span>
                      <span className="notebook-item__meta">
                        {sourceCount(notebook.id)} source{sourceCount(notebook.id) === 1 ? '' : 's'} ·{' '}
                        {new Date(notebook.updatedAt).toLocaleDateString()}
                      </span>
                    </button>
                    <div className="notebook-item__menu">
                      <button
                        type="button"
                        className="btn btn--icon"
                        aria-label={`Rename ${notebook.title}`}
                        title="Rename"
                        onClick={() => {
                          setRenaming(notebook.id);
                          setDraft(notebook.title);
                        }}
                      >
                        ✎
                      </button>
                      <button
                        type="button"
                        className="btn btn--icon"
                        aria-label={`Duplicate ${notebook.title}`}
                        title="Duplicate"
                        onClick={() => void actions.duplicateNotebook(notebook.id)}
                      >
                        ⧉
                      </button>
                      <button
                        type="button"
                        className="btn btn--icon"
                        aria-label={`Delete ${notebook.title}`}
                        title="Delete"
                        onClick={() => setConfirmDelete(notebook.id)}
                      >
                        🗑
                      </button>
                    </div>
                    {confirmDelete === notebook.id && (
                      <div className="notice notice--warning" style={{ margin: '0.35rem' }} role="alertdialog" aria-label={`Delete ${notebook.title}?`}>
                        <p className="small" style={{ marginTop: 0 }}>
                          Delete “{notebook.title}” with {sourceCount(notebook.id)} source
                          {sourceCount(notebook.id) === 1 ? '' : 's'}? Export a backup first if you may need it.
                        </p>
                        <div className="btn-row">
                          <button
                            type="button"
                            className="btn btn--sm btn--danger"
                            onClick={async () => {
                              await actions.deleteNotebook(notebook.id);
                              setConfirmDelete(null);
                            }}
                          >
                            Delete
                          </button>
                          <button type="button" className="btn btn--sm" onClick={() => setConfirmDelete(null)}>
                            Cancel
                          </button>
                        </div>
                      </div>
                    )}
                  </>
                )}
              </div>
            );
          })}
        </nav>
      </div>

      <div className="sidebar__footer">
        <button
          type="button"
          className={state.view === 'search' ? 'btn btn--block btn--active' : 'btn btn--block'}
          onClick={() => {
            actions.setView('search');
            if (open) onClose();
          }}
          aria-current={state.view === 'search' ? 'page' : undefined}
        >
          🔍 Search everything
        </button>
        <button
          type="button"
          className={state.view === 'settings' ? 'btn btn--block btn--active' : 'btn btn--block'}
          onClick={() => {
            actions.setView('settings');
            if (open) onClose();
          }}
          aria-current={state.view === 'settings' ? 'page' : undefined}
        >
          ⚙ Settings
        </button>
        <div className="btn-row">
          <button type="button" className="btn btn--sm" onClick={() => void exportBackupFile()}>
            Export backup
          </button>
          <button type="button" className="btn btn--sm" onClick={() => fileInput.current?.click()}>
            Import backup
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            className="sr-only"
            aria-label="Import a SourceForge backup JSON file"
            onChange={(event) => void onImport(event)}
          />
        </div>
        <dl className="kv" style={{ margin: '0.5rem 0 0' }}>
          <dt>Storage</dt>
          <dd>{state.storage.message}</dd>
          <dt>Engine</dt>
          <dd>{state.engine === 'rust-wasm' ? 'Rust / WebAssembly' : 'TypeScript fallback'}</dd>
          <dt>Provider</dt>
          <dd>
            {state.settings.providers.length === 0
              ? 'Offline (no key)'
              : state.settings.providers.find((p) => p.id === state.settings.activeProviderId)?.label ?? 'Not configured'}
          </dd>
        </dl>
        {state.storageWarning && <p className="notice notice--warning small">{state.storageWarning}</p>}
        {state.storage.backend === 'memory' && (
          <p className="hint">
            IndexedDB is unavailable, so data lives in memory for this tab only. Export a backup before closing.
          </p>
        )}
      </div>
      {state.storage.backend === 'memory' && (
        <span className="sr-only" role="status">
          Memory-only storage mode
        </span>
      )}
    </aside>
  );
}

/** Read-only helper used by tests and diagnostics. */
export function sidebarSummary(): { notebooks: number; engine: string } {
  const state = store.get();
  return { notebooks: state.notebooks.length, engine: state.engine };
}
