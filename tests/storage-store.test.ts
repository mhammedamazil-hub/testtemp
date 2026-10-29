import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  deleteItem,
  getAllByIndex,
  getItem,
  loadSnapshot,
  putItem,
  resetStorageForTests,
  storageStatus,
  StorageQuotaError,
  saveSettings,
} from '../src/lib/storage';
import { actions, notebookMessages, resetAppForTests, searchWorkspace, store } from '../src/lib/store';
import { importPastedText, makeSource } from '../src/lib/ingest';
import { buildBackup, remapBackup } from '../src/lib/backup';
import { DEFAULT_SETTINGS, type Notebook, type Source } from '../src/lib/types';

beforeEach(async () => {
  await resetAppForTests();
  await actions.init();
});

describe('storage layer', () => {
  it('reports IndexedDB availability', () => {
    const status = storageStatus();
    expect(status.backend).toBe('indexeddb');
    expect(status.available).toBe(true);
  });

  it('stores and retrieves records by key and index', async () => {
    const notebook: Notebook = { id: 'nb-store', title: 'T', description: '', emoji: '📓', createdAt: 1, updatedAt: 1 };
    await putItem('notebooks', notebook);
    expect(await getItem<Notebook>('notebooks', 'nb-store')).toMatchObject({ title: 'T' });
    const source: Source = makeSource({ notebookId: 'nb-store', title: 'S', kind: 'text', text: 'body' });
    await putItem('sources', source);
    const byIndex = await getAllByIndex<Source>('sources', 'notebookId', 'nb-store');
    expect(byIndex.map((s) => s.id)).toContain(source.id);
    await deleteItem('sources', source.id);
    expect(await getAllByIndex<Source>('sources', 'notebookId', 'nb-store')).toHaveLength(0);
  });

  it('never persists API keys or the GitHub token', async () => {
    await saveSettings({
      ...DEFAULT_SETTINGS,
      githubToken: 'ghp_secret_super_secret',
      providers: [
        {
          id: 'p', label: 'x', protocol: 'openai', baseUrl: 'https://api.example.com/v1', apiKey: 'sk-secret',
          defaultModel: 'm', models: {}, capabilities: { text: true, vision: false, embeddings: false, tts: false, transcription: false, streaming: false },
        },
      ],
    });
    const snapshot = await loadSnapshot();
    expect(snapshot.settings.githubToken).toBe('');
    expect(snapshot.settings.providers[0].apiKey).toBe('');
  });

  it('maps quota failures to an actionable error type', async () => {
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function throwingPut() {
      throw new DOMException('quota exceeded', 'QuotaExceededError');
    } as unknown as typeof original;
    try {
      const error = await putItem('settings', { id: 'q', value: {} }).catch((err: unknown) => err);
      expect(error).toBeInstanceOf(StorageQuotaError);
      expect((error as Error).message).toMatch(/Storage is full/);
    } finally {
      IDBObjectStore.prototype.put = original;
    }
  });

  it('falls back to memory when IndexedDB cannot be opened', async () => {
    resetStorageForTests();
    const originalOpen = indexedDB.open.bind(indexedDB);
    const spy = vi.spyOn(indexedDB, 'open').mockImplementation(() => {
      throw new Error('private mode');
    });
    const status = storageStatus();
    expect(status.backend).toBe('indexeddb');
    const snapshot = await loadSnapshot();
    expect(snapshot.notebooks).toEqual([]);
    expect(storageStatus().backend).toBe('memory');
    expect(storageStatus().message).toMatch(/memory-only/i);
    spy.mockRestore();
    indexedDB.open = originalOpen;
    resetStorageForTests();
    await actions.init();
  });
});

describe('store workflows', () => {
  it('creates, renames, duplicates and deletes notebooks', async () => {
    const id = await actions.createNotebook('First notebook', '📓');
    expect(store.get().notebooks).toHaveLength(1);
    await actions.updateNotebook(id, { title: 'Renamed' });
    expect(store.get().notebooks[0].title).toBe('Renamed');
    const copyId = await actions.duplicateNotebook(id);
    expect(copyId).toBeTruthy();
    expect(store.get().notebooks).toHaveLength(2);
    await actions.deleteNotebook(id);
    expect(store.get().notebooks).toHaveLength(1);
    expect(store.get().activeNotebookId).toBe(copyId);
  });

  it('adds sources, chunks them, and removes them cleanly', async () => {
    const id = await actions.createNotebook('Sources');
    const source = importPastedText(id, 'Note', 'Alpha beta gamma. '.repeat(40));
    await actions.addSources([source]);
    expect(store.get().sources).toHaveLength(1);
    expect(store.get().chunks.length).toBeGreaterThan(0);
    await actions.renameSource(source.id, 'Renamed note');
    expect(store.get().sources[0].title).toBe('Renamed note');
    await actions.removeSource(source.id);
    expect(store.get().sources).toHaveLength(0);
    expect(store.get().chunks).toHaveLength(0);
  });

  it('persists across a simulated reload', async () => {
    const id = await actions.createNotebook('Persistent');
    const source = importPastedText(id, 'Saved source', 'The renewal rate was 92 percent.');
    await actions.addSources([source]);
    await actions.appendMessage({ id: 'm1', notebookId: id, role: 'user', content: 'hi', createdAt: Date.now() });
    const snapshot = await loadSnapshot();
    expect(snapshot.notebooks.map((n) => n.title)).toContain('Persistent');
    expect(snapshot.sources[0].title).toBe('Saved source');
    expect(snapshot.messages).toHaveLength(1);
    expect(snapshot.chunks.length).toBeGreaterThan(0);
  });

  it('persists study artifacts and deletes them', async () => {
    const id = await actions.createNotebook('Studio');
    await actions.saveArtifact({
      id: 'art-test', notebookId: id, kind: 'notes', title: 'Notes', body: 'Body', citations: [], mode: 'local',
      createdAt: Date.now(), updatedAt: Date.now(),
    });
    expect(store.get().artifacts).toHaveLength(1);
    const snapshot = await loadSnapshot();
    expect(snapshot.artifacts[0].id).toBe('art-test');
    await actions.deleteArtifact('art-test');
    expect(store.get().artifacts).toHaveLength(0);
  });

  it('tracks source selection per notebook', async () => {
    const id = await actions.createNotebook('Selection');
    const a = importPastedText(id, 'A', 'alpha');
    const b = importPastedText(id, 'B', 'beta');
    await actions.addSources([a, b]);
    expect(store.get().selection[id]).toBeNull();
    actions.toggleSourceSelection(id, a.id);
    expect(store.get().selection[id]).toEqual([b.id]);
    actions.setSelection(id, []);
    expect(store.get().selection[id]).toEqual([]);
  });

  it('searches notebooks and source contents', async () => {
    const id = await actions.createNotebook('Alpha notebook');
    await actions.addSources([importPastedText(id, 'Beta source', 'The renewal rate was 92 percent.')]);
    const byTitle = searchWorkspace(store.get(), 'alpha');
    expect(byTitle.notebooks).toHaveLength(1);
    const byContent = searchWorkspace(store.get(), 'renewal');
    expect(byContent.sources).toHaveLength(1);
    expect(byContent.sources[0].snippet).toMatch(/renewal/i);
    expect(searchWorkspace(store.get(), 'zzz').sources).toHaveLength(0);
  });

  it('restores a backup into the store', async () => {
    const id = await actions.createNotebook('Original');
    await actions.addSources([importPastedText(id, 'Src', 'text body')]);
    const backup = buildBackup(store.get());
    await resetAppForTests();
    await actions.init();
    const payload = remapBackup(backup, new Set(store.get().notebooks.map((n) => n.id)));
    await actions.importData(payload);
    expect(store.get().notebooks.map((n) => n.title)).toContain('Original');
    expect(store.get().sources).toHaveLength(1);
    expect(store.get().chunks.length).toBeGreaterThan(0);
    const snapshot = await loadSnapshot();
    expect(snapshot.notebooks).toHaveLength(1);
  });

  it('clears conversation history for one notebook only', async () => {
    const a = await actions.createNotebook('A');
    const b = await actions.createNotebook('B');
    await actions.appendMessage({ id: 'm1', notebookId: a, role: 'user', content: 'q', createdAt: 1 });
    await actions.appendMessage({ id: 'm2', notebookId: b, role: 'user', content: 'q', createdAt: 2 });
    await actions.clearConversation(a);
    expect(notebookMessages(store.get(), a)).toHaveLength(0);
    expect(notebookMessages(store.get(), b)).toHaveLength(1);
  });
});
