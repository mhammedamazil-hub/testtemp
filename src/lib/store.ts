/**
 * Application store: a tiny observable state container with explicit async
 * actions. Components subscribe through `useAppState` (useSyncExternalStore).
 *
 * Every mutation that changes user data is persisted immediately. Storage
 * failures are surfaced as toasts instead of being swallowed.
 */

import { useSyncExternalStore } from 'react';
import { buildChunks } from './text';
import {
  clearAll,
  deleteByIndex,
  deleteItem,
  loadSnapshot,
  putMany,
  putItem,
  resetStorageForTests,
  saveSettings,
  StorageQuotaError,
  storageStatus,
  type StorageStatus,
} from './storage';
import {
  DEFAULT_SETTINGS,
  type Artifact,
  type ChatMessage,
  type Chunk,
  type Notebook,
  type Settings,
  type Source,
} from './types';

export interface Toast {
  id: string;
  kind: 'info' | 'success' | 'error' | 'warning';
  message: string;
  detail?: string;
}

export interface BusyState {
  importing: boolean;
  thinking: boolean;
  generating: string | null;
  label: string;
}

export interface AppState {
  ready: boolean;
  storage: StorageStatus;
  notebooks: Notebook[];
  sources: Source[];
  chunks: Chunk[];
  messages: ChatMessage[];
  artifacts: Artifact[];
  settings: Settings;
  activeNotebookId: string | null;
  view: 'notebook' | 'search' | 'settings';
  selection: Record<string, string[] | null>;
  toasts: Toast[];
  busy: BusyState;
  storageWarning: string | null;
  /** Whether the Rust/WASM core is active or the TypeScript fallback is used. */
  engine: 'rust-wasm' | 'typescript';
}

function rid(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return `${prefix}-${crypto.randomUUID()}`;
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

const initialState: AppState = {
  ready: false,
  storage: { backend: 'memory', available: false, message: 'Checking storage…' },
  notebooks: [],
  sources: [],
  chunks: [],
  messages: [],
  artifacts: [],
  settings: { ...DEFAULT_SETTINGS },
  activeNotebookId: null,
  view: 'notebook',
  selection: {},
  toasts: [],
  busy: { importing: false, thinking: false, generating: null, label: '' },
  storageWarning: null,
  engine: 'typescript',
};

type Listener = () => void;

class Store {
  private state: AppState = initialState;
  private listeners = new Set<Listener>();

  getSnapshot = (): AppState => this.state;

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  set(partial: Partial<AppState> | ((state: AppState) => Partial<AppState>)): void {
    const next = typeof partial === 'function' ? partial(this.state) : partial;
    this.state = { ...this.state, ...next };
    for (const listener of this.listeners) listener();
  }

  get(): AppState {
    return this.state;
  }
}

export const store = new Store();

export function useAppState(): AppState {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

export function useNotebook(): Notebook | undefined {
  const state = useAppState();
  return state.notebooks.find((n) => n.id === state.activeNotebookId);
}

const SELECTION_KEY = 'sourceforge.selection.v1';

function readSelection(): Record<string, string[] | null> {
  try {
    const raw = localStorage.getItem(SELECTION_KEY);
    return raw ? (JSON.parse(raw) as Record<string, string[] | null>) : {};
  } catch {
    return {};
  }
}

function writeSelection(selection: Record<string, string[] | null>): void {
  try {
    localStorage.setItem(SELECTION_KEY, JSON.stringify(selection));
  } catch {
    /* selection is a convenience, not user data */
  }
}

let pendingWrites = 0;

function noteWriteFailure(err: unknown): void {
  const message =
    err instanceof StorageQuotaError
      ? err.message
      : `Could not save to local storage: ${err instanceof Error ? err.message : String(err)}`;
  store.set({
    storageWarning: message,
    storage: { backend: 'memory', available: false, message },
  });
  actions.toast('error', message, 'Export a JSON backup now to avoid losing work.');
  pendingWrites = 0;
}

async function persist(task: () => Promise<unknown>): Promise<void> {
  pendingWrites += 1;
  try {
    await task();
    store.set({ storageWarning: null });
  } catch (err) {
    noteWriteFailure(err);
  } finally {
    pendingWrites = Math.max(0, pendingWrites - 1);
  }
}

export function hasPendingWrites(): boolean {
  return pendingWrites > 0;
}

let initPromise: Promise<void> | null = null;

function mergeById<T extends { id: string }>(primary: T[], secondary: T[]): T[] {
  const seen = new Set(primary.map((item) => item.id));
  return [...primary, ...secondary.filter((item) => !seen.has(item.id))];
}

export const actions = {
  /**
   * Load the persisted snapshot. Re-entrant calls share one promise, and data
   * created while storage was still opening is merged rather than replaced —
   * a fast click on "New notebook" must never be silently discarded.
   */
  async init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = (async () => {
      try {
        const snapshot = await loadSnapshot();
        const selection = readSelection();
        const current = store.get();
        const userActed =
          current.notebooks.length > 0 || current.sources.length > 0 || current.chunks.length > 0;
        const notebooks = userActed ? mergeById(current.notebooks, snapshot.notebooks) : snapshot.notebooks;
        store.set({
          notebooks,
          sources: userActed ? mergeById(current.sources, snapshot.sources) : snapshot.sources,
          chunks: userActed ? mergeById(current.chunks, snapshot.chunks) : snapshot.chunks,
          messages: userActed ? mergeById(current.messages, snapshot.messages) : snapshot.messages,
          artifacts: userActed ? mergeById(current.artifacts, snapshot.artifacts) : snapshot.artifacts,
          settings: userActed ? current.settings : snapshot.settings,
          storage: storageStatus(),
          selection,
          ready: true,
          activeNotebookId: current.activeNotebookId ?? snapshot.notebooks[0]?.id ?? null,
        });
      } catch (err) {
        store.set({
          ready: true,
          storage: { backend: 'memory', available: false, message: 'Storage could not be read. Starting empty.' },
        });
        actions.toast('error', `Local storage could not be read: ${err instanceof Error ? err.message : String(err)}`);
      }
    })();
    try {
      await initPromise;
    } finally {
      initPromise = null;
    }
  },

  async reload(): Promise<void> {
    resetStorageForTests();
    store.set({ ready: false });
    await actions.init();
  },

  setEngine(engine: 'rust-wasm' | 'typescript'): void {
    store.set({ engine });
  },

  setView(view: AppState['view']): void {
    store.set({ view });
  },

  selectNotebook(id: string | null): void {
    store.set({ activeNotebookId: id, view: 'notebook' });
  },

  toast(kind: Toast['kind'], message: string, detail?: string): string {
    const id = rid('toast');
    const toast: Toast = { id, kind, message, ...(detail ? { detail } : {}) };
    store.set((state) => ({ toasts: [...state.toasts.slice(-4), toast] }));
    if (kind !== 'error') {
      setTimeout(() => actions.dismissToast(id), 6000);
    }
    return id;
  },

  dismissToast(id: string): void {
    store.set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
  },

  setBusy(partial: Partial<BusyState>): void {
    store.set((state) => ({ busy: { ...state.busy, ...partial } }));
  },

  async createNotebook(title = 'Untitled notebook', emoji = '📓'): Promise<string> {
    const now = Date.now();
    const notebook: Notebook = {
      id: rid('nb'),
      title,
      description: '',
      emoji,
      createdAt: now,
      updatedAt: now,
    };
    store.set((state) => ({
      notebooks: [...state.notebooks, notebook],
      activeNotebookId: notebook.id,
      view: 'notebook',
    }));
    await persist(() => putItem('notebooks', notebook));
    return notebook.id;
  },

  async updateNotebook(id: string, patch: Partial<Omit<Notebook, 'id' | 'createdAt'>>): Promise<void> {
    const state = store.get();
    const existing = state.notebooks.find((n) => n.id === id);
    if (!existing) return;
    const updated: Notebook = { ...existing, ...patch, updatedAt: Date.now() };
    store.set((s) => ({ notebooks: s.notebooks.map((n) => (n.id === id ? updated : n)) }));
    await persist(() => putItem('notebooks', updated));
  },

  async deleteNotebook(id: string): Promise<void> {
    store.set((s) => {
      const notebooks = s.notebooks.filter((n) => n.id !== id);
      return {
        notebooks,
        sources: s.sources.filter((src) => src.notebookId !== id),
        chunks: s.chunks.filter((c) => c.notebookId !== id),
        messages: s.messages.filter((m) => m.notebookId !== id),
        artifacts: s.artifacts.filter((a) => a.notebookId !== id),
        activeNotebookId: s.activeNotebookId === id ? notebooks[0]?.id ?? null : s.activeNotebookId,
      };
    });
    await persist(async () => {
      await Promise.all([
        deleteItem('notebooks', id),
        deleteByIndex('sources', 'notebookId', id),
        deleteByIndex('chunks', 'notebookId', id),
        deleteByIndex('messages', 'notebookId', id),
        deleteByIndex('artifacts', 'notebookId', id),
      ]);
    });
  },

  async duplicateNotebook(id: string): Promise<string | null> {
    const state = store.get();
    const notebook = state.notebooks.find((n) => n.id === id);
    if (!notebook) return null;
    const now = Date.now();
    const copy: Notebook = { ...notebook, id: rid('nb'), title: `${notebook.title} (copy)`, createdAt: now, updatedAt: now };
    const sources = state.sources
      .filter((s) => s.notebookId === id)
      .map((source) => ({ ...source, id: rid('src'), notebookId: copy.id }));
    const artifacts = state.artifacts
      .filter((a) => a.notebookId === id)
      .map((artifact) => ({ ...artifact, id: rid('art'), notebookId: copy.id }));
    const { chunkSize, chunkOverlap } = state.settings;
    const chunks = sources.flatMap((source) =>
      buildChunks(source.id, copy.id, source.text, { chunkSize, chunkOverlap }, source.segments),
    );
    store.set((s) => ({
      notebooks: [...s.notebooks, copy],
      sources: [...s.sources, ...sources],
      chunks: [...s.chunks, ...chunks],
      artifacts: [...s.artifacts, ...artifacts],
      activeNotebookId: copy.id,
      view: 'notebook',
    }));
    await persist(async () => {
      await putItem('notebooks', copy);
      await putMany('sources', sources);
      await putMany('chunks', chunks);
      await putMany('artifacts', artifacts);
    });
    actions.toast('success', `Duplicated “${notebook.title}”.`);
    return copy.id;
  },

  /** Chunk and persist new sources; yields to the event loop for large imports. */
  async addSources(input: Source[]): Promise<Source[]> {
    if (!input.length) return [];
    const state = store.get();
    const { chunkSize, chunkOverlap } = state.settings;
    const added: Source[] = [];
    const allChunks: Chunk[] = [];
    store.set((s) => ({ busy: { ...s.busy, importing: true, label: `Indexing ${input.length} source(s)…` } }));
    try {
      for (const source of input) {
        const chunks = buildChunks(source.id, source.notebookId, source.text, { chunkSize, chunkOverlap }, source.segments);
        added.push(source);
        allChunks.push(...chunks);
        await putMany('sources', [source]);
        await putMany('chunks', chunks);
        // Keep the UI responsive during large imports.
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
      store.set((s) => ({
        sources: [...s.sources, ...added],
        chunks: [...s.chunks, ...allChunks],
        storageWarning: null,
      }));
      store.set((s) => ({ selection: { ...s.selection, [added[0].notebookId]: null } }));
      return added;
    } catch (err) {
      noteWriteFailure(err);
      store.set((s) => ({ sources: [...s.sources, ...added], chunks: [...s.chunks, ...allChunks] }));
      return added;
    } finally {
      store.set((s) => ({ busy: { ...s.busy, importing: false, label: '' } }));
    }
  },

  async renameSource(id: string, title: string): Promise<void> {
    const source = store.get().sources.find((s) => s.id === id);
    if (!source) return;
    const updated: Source = { ...source, title: title.trim() || source.title, updatedAt: Date.now() };
    store.set((s) => ({ sources: s.sources.map((src) => (src.id === id ? updated : src)) }));
    await persist(() => putItem('sources', updated));
  },

  async updateSource(id: string, patch: Partial<Source>): Promise<void> {
    const source = store.get().sources.find((s) => s.id === id);
    if (!source) return;
    const updated: Source = { ...source, ...patch, id: source.id, updatedAt: Date.now() };
    store.set((s) => ({ sources: s.sources.map((src) => (src.id === id ? updated : src)) }));
    await persist(() => putItem('sources', updated));
  },

  async removeSource(id: string): Promise<void> {
    const source = store.get().sources.find((s) => s.id === id);
    store.set((s) => ({
      sources: s.sources.filter((src) => src.id !== id),
      chunks: s.chunks.filter((c) => c.sourceId !== id),
    }));
    await persist(async () => {
      await deleteItem('sources', id);
      await deleteByIndex('chunks', 'sourceId', id);
    });
    if (source) actions.toast('info', `Removed “${source.title}”.`);
  },

  async reindexNotebook(notebookId: string): Promise<void> {
    const state = store.get();
    const sources = state.sources.filter((s) => s.notebookId === notebookId);
    const { chunkSize, chunkOverlap } = state.settings;
    const chunks = sources.flatMap((source) =>
      buildChunks(source.id, notebookId, source.text, { chunkSize, chunkOverlap }, source.segments),
    );
    store.set((s) => ({ chunks: [...s.chunks.filter((c) => c.notebookId !== notebookId), ...chunks] }));
    await persist(async () => {
      await deleteByIndex('chunks', 'notebookId', notebookId);
      await putMany('chunks', chunks);
    });
  },

  setSelection(notebookId: string, sourceIds: string[] | null): void {
    const selection = { ...store.get().selection, [notebookId]: sourceIds };
    writeSelection(selection);
    store.set({ selection });
  },

  toggleSourceSelection(notebookId: string, sourceId: string): void {
    const state = store.get();
    const current = state.selection[notebookId];
    const all = state.sources.filter((s) => s.notebookId === notebookId).map((s) => s.id);
    const active = current ?? all;
    const next = active.includes(sourceId) ? active.filter((id) => id !== sourceId) : [...active, sourceId];
    actions.setSelection(notebookId, next.length === all.length ? null : next);
  },

  async applyChunkEmbeddings(chunks: Chunk[]): Promise<void> {
    const byId = new Map(chunks.map((c) => [c.id, c]));
    store.set((s) => ({ chunks: s.chunks.map((c) => byId.get(c.id) ?? c) }));
    await persist(() => putMany('chunks', chunks));
  },

  async appendMessage(message: ChatMessage): Promise<void> {
    store.set((s) => ({ messages: [...s.messages, message] }));
    await persist(() => putItem('messages', message));
  },

  async replaceMessage(id: string, patch: Partial<ChatMessage>): Promise<void> {
    const existing = store.get().messages.find((m) => m.id === id);
    if (!existing) return;
    const updated = { ...existing, ...patch };
    store.set((s) => ({ messages: s.messages.map((m) => (m.id === id ? updated : m)) }));
    await persist(() => putItem('messages', updated));
  },

  async clearConversation(notebookId: string): Promise<void> {
    store.set((s) => ({ messages: s.messages.filter((m) => m.notebookId !== notebookId) }));
    await persist(() => deleteByIndex('messages', 'notebookId', notebookId));
  },

  async saveArtifact(artifact: Artifact): Promise<void> {
    const exists = store.get().artifacts.some((a) => a.id === artifact.id);
    store.set((s) => ({
      artifacts: exists ? s.artifacts.map((a) => (a.id === artifact.id ? artifact : a)) : [...s.artifacts, artifact],
    }));
    await persist(() => putItem('artifacts', artifact));
  },

  async deleteArtifact(id: string): Promise<void> {
    store.set((s) => ({ artifacts: s.artifacts.filter((a) => a.id !== id) }));
    await persist(() => deleteItem('artifacts', id));
  },

  async updateSettings(patch: Partial<Settings>): Promise<void> {
    const settings = { ...store.get().settings, ...patch };
    store.set({ settings });
    await persist(() => saveSettings(settings));
    if (patch.chunkSize !== undefined || patch.chunkOverlap !== undefined) {
      const state = store.get();
      for (const notebook of state.notebooks) {
        await actions.reindexNotebook(notebook.id);
      }
    }
  },

  async importData(payload: { notebooks: Notebook[]; sources: Source[]; artifacts: Artifact[]; messages: ChatMessage[] }): Promise<void> {
    const { chunkSize, chunkOverlap } = store.get().settings;
    const chunks = payload.sources.flatMap((source) =>
      buildChunks(source.id, source.notebookId, source.text, { chunkSize, chunkOverlap }, source.segments),
    );
    store.set((s) => ({
      notebooks: [...s.notebooks, ...payload.notebooks],
      sources: [...s.sources, ...payload.sources],
      chunks: [...s.chunks, ...chunks],
      artifacts: [...s.artifacts, ...payload.artifacts],
      messages: [...s.messages, ...payload.messages],
      activeNotebookId: payload.notebooks[0]?.id ?? s.activeNotebookId,
      view: 'notebook',
    }));
    await persist(async () => {
      await putMany('notebooks', payload.notebooks);
      await putMany('sources', payload.sources);
      await putMany('chunks', chunks);
      await putMany('artifacts', payload.artifacts);
      await putMany('messages', payload.messages);
      await putItem('settings', { id: 'app', value: { ...store.get().settings, providers: [], githubToken: '' } });
    });
  },

  async wipeAll(): Promise<void> {
    store.set({ ...initialState, ready: true, settings: { ...store.get().settings } });
    await clearAll();
    await persist(() => saveSettings(store.get().settings));
    actions.toast('info', 'All local data was deleted.');
  },

  async refreshStorageStatus(): Promise<void> {
    store.set({ storage: storageStatus() });
  },
};

export function notebookSources(state: AppState, notebookId: string | null): Source[] {
  if (!notebookId) return [];
  return state.sources.filter((s) => s.notebookId === notebookId);
}

export function notebookChunks(state: AppState, notebookId: string | null): Chunk[] {
  if (!notebookId) return [];
  return state.chunks.filter((c) => c.notebookId === notebookId);
}

export function activeSourceIds(state: AppState, notebookId: string): string[] | null {
  const all = state.sources.filter((s) => s.notebookId === notebookId).map((s) => s.id);
  const selection = state.selection[notebookId];
  if (!selection) return null;
  const filtered = selection.filter((id) => all.includes(id));
  return filtered.length === all.length ? null : filtered;
}

export function notebookMessages(state: AppState, notebookId: string | null): ChatMessage[] {
  if (!notebookId) return [];
  return state.messages.filter((m) => m.notebookId === notebookId).sort((a, b) => a.createdAt - b.createdAt);
}

export function notebookArtifacts(state: AppState, notebookId: string | null): Artifact[] {
  if (!notebookId) return [];
  return state.artifacts.filter((a) => a.notebookId === notebookId).sort((a, b) => b.createdAt - a.createdAt);
}

export interface SearchResult {
  notebooks: Notebook[];
  sources: { source: Source; snippet: string }[];
}

export function searchWorkspace(state: AppState, query: string): SearchResult {
  const q = query.trim().toLowerCase();
  if (!q) return { notebooks: [], sources: [] };
  const terms = q.split(/\s+/).filter(Boolean);
  const notebooks = state.notebooks.filter((n) =>
    terms.every((t) => `${n.title} ${n.description}`.toLowerCase().includes(t)),
  );
  const sources: { source: Source; snippet: string }[] = [];
  for (const source of state.sources) {
    const haystack = `${source.title}\n${source.text}`.toLowerCase();
    if (!terms.every((t) => haystack.includes(t))) continue;
    const idx = source.text.toLowerCase().indexOf(terms[0]);
    const start = Math.max(0, idx - 80);
    sources.push({
      source,
      snippet: source.text.slice(start, start + 240).replace(/\s+/g, ' ').trim(),
    });
    if (sources.length >= 60) break;
  }
  return { notebooks, sources };
}

/** Used by tests and by the admin panel to force a clean slate. */
export async function resetAppForTests(): Promise<void> {
  await clearAll();
  resetStorageForTests();
  store.set({ ...initialState, ready: true });
}
