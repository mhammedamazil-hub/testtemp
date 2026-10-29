import { useEffect, useState, type ReactNode } from 'react';
import { actions, notebookArtifacts, notebookChunks, notebookSources, useAppState } from '../lib/store';
import type { Notebook } from '../lib/types';
import { SourcePanel } from './SourcePanel';
import { ChatPanel } from './ChatPanel';
import { StudyPanel } from './StudyPanel';
import { AddSourceDialog } from './AddSourceDialog';
import { SourceViewer, type SourceViewerTarget } from './SourceViewer';
import { ConfirmDialog } from './ui';
import { exportBackupFile, exportHtmlReport, exportNotebookMarkdown, exportPdfReport, exportPptxDeck } from '../lib/exports/actions';

type Tab = 'chat' | 'sources' | 'studio';

export function NotebookView(props: { notebook: Notebook; onOpenSidebar: () => void }): ReactNode {
  const state = useAppState();
  const [tab, setTab] = useState<Tab>('chat');
  const [adding, setAdding] = useState(false);
  const [viewer, setViewer] = useState<SourceViewerTarget | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const sources = notebookSources(state, props.notebook.id);
  const chunks = notebookChunks(state, props.notebook.id);
  const artifacts = notebookArtifacts(state, props.notebook.id);

  useEffect(() => {
    setTab(sources.length ? 'chat' : 'sources');
    // Only when switching notebooks.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.notebook.id]);

  useEffect(() => {
    if (!menuOpen) return;
    const close = () => setMenuOpen(false);
    window.addEventListener('click', close);
    return () => window.removeEventListener('click', close);
  }, [menuOpen]);

  return (
    <>
      <header className="topbar">
        <button type="button" className="btn btn--ghost btn--icon menu-toggle" aria-label="Open navigation" onClick={props.onOpenSidebar}>
          ☰
        </button>
        <div className="topbar__title">
          <span aria-hidden="true" style={{ fontSize: '1.2rem' }}>
            {props.notebook.emoji}
          </span>
          {renaming !== null ? (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void actions.updateNotebook(props.notebook.id, { title: renaming.trim() || props.notebook.title });
                setRenaming(null);
              }}
              className="row"
              style={{ flex: 1 }}
            >
              <input
                className="input"
                value={renaming}
                onChange={(event) => setRenaming(event.target.value)}
                aria-label="Notebook title"
                autoFocus
                onBlur={() => {
                  void actions.updateNotebook(props.notebook.id, { title: renaming.trim() || props.notebook.title });
                  setRenaming(null);
                }}
              />
            </form>
          ) : (
            <div style={{ minWidth: 0 }}>
              <h1>{props.notebook.title}</h1>
              <div className="topbar__meta">
                {sources.length} sources · {chunks.length} passages · {artifacts.length} study items
                {props.notebook.description ? ` · ${props.notebook.description}` : ''}
              </div>
            </div>
          )}
        </div>
        <div className="topbar__actions">
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => setRenaming(props.notebook.title)} aria-label="Rename notebook">
            ✏️ <span className="label">Rename</span>
          </button>
          <button type="button" className="btn btn--primary btn--sm" onClick={() => setAdding(true)}>
            + <span className="label">Add source</span>
          </button>
          <div style={{ position: 'relative' }}>
            <button
              type="button"
              className="btn btn--sm"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              onClick={(event) => {
                event.stopPropagation();
                setMenuOpen((v) => !v);
              }}
            >
              ⋯ <span className="label">Export</span>
            </button>
            {menuOpen ? (
              <div
                role="menu"
                className="card"
                style={{ position: 'absolute', right: 0, top: '2.4rem', zIndex: 30, width: '15rem', padding: '0.35rem' }}
                onClick={(event) => event.stopPropagation()}
              >
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); void exportBackupFile(); }}>
                  JSON backup (whole app)
                </button>
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); exportNotebookMarkdown(props.notebook.id); }}>
                  Notebook as Markdown
                </button>
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); exportHtmlReport(props.notebook.id); }}>
                  HTML study report
                </button>
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); void exportPdfReport(props.notebook.id); }}>
                  PDF report
                </button>
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); void exportPptxDeck(props.notebook.id); }}>
                  PPTX presentation
                </button>
                <div style={{ height: 1, background: 'var(--border)', margin: '0.3rem 0' }} />
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); void actions.duplicateNotebook(props.notebook.id); }}>
                  Duplicate notebook
                </button>
                <button type="button" role="menuitem" className="nav-item" onClick={() => { setMenuOpen(false); setConfirmDelete(true); }}>
                  Delete notebook…
                </button>
              </div>
            ) : null}
          </div>
        </div>
      </header>

      <div className="content">
        <div className="container">
          <div className="tabs" role="tablist" aria-label="Notebook sections">
            {(
              [
                { id: 'chat', label: `Chat (${state.messages.filter((m) => m.notebookId === props.notebook.id).length})` },
                { id: 'sources', label: `Sources (${sources.length})` },
                { id: 'studio', label: `Studio (${artifacts.length})` },
              ] as { id: Tab; label: string }[]
            ).map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                className="tab"
                aria-selected={tab === item.id}
                aria-controls={`panel-${item.id}`}
                id={`tab-${item.id}`}
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>

          <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
            {tab === 'chat' ? (
              <ChatPanel
                notebookId={props.notebook.id}
                notebookTitle={props.notebook.title}
                onOpenSource={setViewer}
                onAddSource={() => setAdding(true)}
              />
            ) : null}
            {tab === 'sources' ? (
              <SourcePanel notebookId={props.notebook.id} onAddSource={() => setAdding(true)} onOpenSource={(sourceId, chunkId) => setViewer({ sourceId, chunkId })} />
            ) : null}
            {tab === 'studio' ? (
              <StudyPanel notebookId={props.notebook.id} notebookTitle={props.notebook.title} onAddSource={() => setAdding(true)} />
            ) : null}
          </div>
        </div>
      </div>

      {adding ? <AddSourceDialog notebookId={props.notebook.id} onClose={() => setAdding(false)} /> : null}
      {viewer ? <SourceViewer target={viewer} onClose={() => setViewer(null)} /> : null}
      {confirmDelete ? (
        <ConfirmDialog
          title="Delete notebook?"
          message={`“${props.notebook.title}” and everything in it will be deleted from this browser. Export a backup first if you may need it.`}
          confirmLabel="Delete notebook"
          danger
          onCancel={() => setConfirmDelete(false)}
          onConfirm={() => {
            void actions.deleteNotebook(props.notebook.id);
            setConfirmDelete(false);
          }}
        />
      ) : null}
    </>
  );
}
