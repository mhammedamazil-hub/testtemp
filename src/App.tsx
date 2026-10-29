import { Component, useEffect, useState, type ErrorInfo, type ReactNode } from 'react';
import { actions, useAppState, useNotebook } from './lib/store';
import { initEngine } from './lib/engine';
import { registerServiceWorker, type PwaUpdate } from './lib/pwa';
import { Sidebar } from './components/Sidebar';
import { NotebookView } from './components/NotebookView';
import { SettingsView } from './components/SettingsView';
import { SearchView } from './components/SearchView';
import { Toasts } from './components/ui';

class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('SourceForge crashed', error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="container container--narrow" style={{ padding: '3rem 1rem' }}>
          <h1>Something went wrong</h1>
          <p className="muted">
            The interface crashed. Your notebooks are still in local storage; reload to continue. If the crash repeats, export a backup and
            clear local data.
          </p>
          <pre>{this.state.error.message}</pre>
          <div className="btn-row">
            <button type="button" className="btn btn--primary" onClick={() => window.location.reload()}>
              Reload
            </button>
            <button type="button" className="btn" onClick={() => this.setState({ error: null })}>
              Try to continue
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

function AppShell(): ReactNode {
  const state = useAppState();
  const notebook = useNotebook();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [update, setUpdate] = useState<PwaUpdate | null>(null);

  useEffect(() => {
    void actions.init();
    void initEngine().then((status) => {
      actions.setEngine(status.engine);
      if (status.engine === 'typescript') {
        console.info('[SourceForge] engine:', status.detail);
      }
    });
    void registerServiceWorker((available) => setUpdate(available));
    // Manifest shortcuts: ./#new and ./#search
    if (window.location.hash === '#new') {
      void actions.createNotebook();
      window.history.replaceState(null, '', window.location.pathname);
    } else if (window.location.hash === '#search') {
      actions.setView('search');
    }
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = state.settings.theme;
  }, [state.settings.theme]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        actions.setView('search');
        setTimeout(() => document.getElementById('global-search')?.focus(), 30);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    if (!state.activeNotebookId && state.ready && state.notebooks.length === 0) {
      // First run: show a friendly empty state rather than auto-creating data.
    }
  }, [state.activeNotebookId, state.ready, state.notebooks.length]);

  if (!state.ready) {
    return (
      <div className="empty" style={{ paddingTop: '20vh' }}>
        <span className="spinner" aria-hidden="true" />
        <p className="muted" style={{ marginTop: '0.8rem' }}>
          Opening your local workspace…
        </p>
      </div>
    );
  }

  return (
    <div className="app">
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>
      <Sidebar open={sidebarOpen} onClose={() => setSidebarOpen(false)} />
      {sidebarOpen ? <button type="button" className="scrim" aria-label="Close navigation" onClick={() => setSidebarOpen(false)} /> : null}
      <div className="main">
        {update?.available ? (
          <div className="notice notice--info" style={{ borderRadius: 0, marginBottom: 0 }}>
            <div className="row">
              <span style={{ flex: 1 }}>A new version of SourceForge is available.</span>
              <button type="button" className="btn btn--sm btn--primary" onClick={update.apply}>
                Reload to update
              </button>
            </div>
          </div>
        ) : null}
        <main id="main-content" className="content" aria-label="Workspace">
          {state.view === 'settings' ? (
            <SettingsView />
          ) : state.view === 'search' ? (
            <SearchView />
          ) : notebook ? (
            <NotebookView notebook={notebook} onOpenSidebar={() => setSidebarOpen(true)} />
          ) : (
            <div className="container container--narrow">
              <div className="empty" style={{ paddingTop: '8vh' }}>
                <div className="empty__icon" aria-hidden="true">🔭</div>
                <h1>Welcome to SourceForge</h1>
                <p>
                  A local-first research workspace. Create a notebook, add sources (files, GitHub, web pages, transcripts), then ask
                  questions and get answers with citations that resolve to exact passages.
                </p>
                <div className="btn-row" style={{ justifyContent: 'center' }}>
                  <button type="button" className="btn btn--primary" onClick={() => void actions.createNotebook('My first notebook', '📓')}>
                    Create a notebook
                  </button>
                  <button type="button" className="btn" onClick={() => actions.setView('settings')}>
                    Configure AI provider
                  </button>
                </div>
                <p className="tiny faint" style={{ marginTop: '1.5rem' }}>
                  No account, no server. Everything stays in this browser. AI features are optional and use your own keys.
                </p>
              </div>
            </div>
          )}
        </main>
      </div>
      <Toasts />
    </div>
  );
}

export default function App(): ReactNode {
  return (
    <ErrorBoundary>
      <AppShell />
    </ErrorBoundary>
  );
}
