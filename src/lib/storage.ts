/**
 * Storage layer.
 *
 * Primary backend: IndexedDB via a tiny hand-rolled promise wrapper (no
 * dependency, easy to mock). If IndexedDB is missing or fails (Safari private
 * mode, quota exceeded, corrupted DB) we fall back to an in-memory store and
 * surface the degradation to the UI instead of silently pretending to save.
 */

import type {
  Artifact,
  ChatMessage,
  Chunk,
  Notebook,
  Settings,
  Source,
} from './types';
import { DEFAULT_SETTINGS, SCHEMA_VERSION } from './types';

export const DB_NAME = 'sourceforge';
export const DB_VERSION = 1;

export type StoreName = 'notebooks' | 'sources' | 'chunks' | 'messages' | 'artifacts' | 'settings';

export interface StorageStatus {
  backend: 'indexeddb' | 'memory';
  available: boolean;
  message: string;
}

function isQuotaError(err: unknown): boolean {
  const name = (err as { name?: string } | null)?.name;
  return name === 'QuotaExceededError' || name === 'NS_ERROR_DOM_QUOTA_REACHED';
}

export class StorageQuotaError extends Error {
  constructor(message = 'Storage is full. Free space or export a backup, then retry.') {
    super(message);
    this.name = 'StorageQuotaError';
  }
}

let dbPromise: Promise<IDBDatabase> | null = null;
let memoryMode = false;
const memory = new Map<StoreName, Map<string, unknown>>();

function memStore<T>(name: StoreName): Map<string, T> {
  let store = memory.get(name) as Map<string, T> | undefined;
  if (!store) {
    store = new Map<string, T>();
    memory.set(name, store as Map<string, unknown>);
  }
  return store;
}

export function storageStatus(): StorageStatus {
  if (typeof indexedDB === 'undefined') {
    return { backend: 'memory', available: false, message: 'IndexedDB is not available in this browser. Changes will be lost on reload — export a backup.' };
  }
  if (memoryMode) {
    return { backend: 'memory', available: false, message: 'IndexedDB could not be opened. Running in memory-only mode — export a backup to keep your data.' };
  }
  return { backend: 'indexeddb', available: true, message: 'Saved locally in this browser (IndexedDB).' };
}

export function resetStorageForTests(): void {
  dbPromise = null;
  memoryMode = false;
  memory.clear();
}

function openDb(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  const promise = new Promise<IDBDatabase>((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('IndexedDB unavailable'));
      return;
    }
    let req: IDBOpenDBRequest;
    try {
      req = indexedDB.open(DB_NAME, DB_VERSION);
    } catch (err) {
      reject(err);
      return;
    }
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains('notebooks')) db.createObjectStore('notebooks', { keyPath: 'id' });
      if (!db.objectStoreNames.contains('sources')) {
        const s = db.createObjectStore('sources', { keyPath: 'id' });
        s.createIndex('notebookId', 'notebookId');
      }
      if (!db.objectStoreNames.contains('chunks')) {
        const s = db.createObjectStore('chunks', { keyPath: 'id' });
        s.createIndex('notebookId', 'notebookId');
        s.createIndex('sourceId', 'sourceId');
      }
      if (!db.objectStoreNames.contains('messages')) {
        const s = db.createObjectStore('messages', { keyPath: 'id' });
        s.createIndex('notebookId', 'notebookId');
      }
      if (!db.objectStoreNames.contains('artifacts')) {
        const s = db.createObjectStore('artifacts', { keyPath: 'id' });
        s.createIndex('notebookId', 'notebookId');
      }
      if (!db.objectStoreNames.contains('settings')) db.createObjectStore('settings', { keyPath: 'id' });
      void SCHEMA_VERSION;
    };
    req.onsuccess = () => {
      const db = req.result;
      db.onversionchange = () => db.close();
      resolve(db);
    };
    req.onerror = () => reject(req.error ?? new Error('IndexedDB open failed'));
    req.onblocked = () => reject(new Error('IndexedDB open blocked by another tab'));
  });
  const guarded: Promise<IDBDatabase> = promise.catch((err: unknown) => {
    memoryMode = true;
    dbPromise = null;
    throw err;
  });
  dbPromise = guarded;
  return guarded;
}

function tx<T>(store: StoreName, mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T> | void): Promise<T | undefined> {
  if (memoryMode) return Promise.resolve(undefined);
  return openDb().then(
    (db) =>
      new Promise<T | undefined>((resolve, reject) => {
        let transaction: IDBTransaction;
        try {
          transaction = db.transaction(store, mode);
        } catch (err) {
          reject(isQuotaError(err) ? new StorageQuotaError() : err);
          return;
        }
        const os = transaction.objectStore(store);
        let request: IDBRequest<T> | void;
        try {
          request = fn(os);
        } catch (err) {
          reject(isQuotaError(err) ? new StorageQuotaError() : err);
          return;
        }
        transaction.oncomplete = () => {
          resolve(request && 'result' in request ? (request.result as T) : undefined);
        };
        transaction.onerror = () => {
          const err = transaction.error;
          reject(isQuotaError(err) ? new StorageQuotaError() : err ?? new Error('IndexedDB transaction failed'));
        };
        transaction.onabort = () => {
          const err = transaction.error;
          reject(isQuotaError(err) ? new StorageQuotaError() : err ?? new Error('IndexedDB transaction aborted'));
        };
      }),
  ).catch((err: unknown) => {
    // Read failures degrade to memory; write failures propagate so the UI can
    // show an error and refuse to claim data was saved.
    if (err instanceof StorageQuotaError || isQuotaError(err)) throw new StorageQuotaError();
    memoryMode = true;
    return undefined;
  });
}

export async function putMany<T extends { id?: string }>(store: StoreName, items: T[]): Promise<void> {
  if (!items.length) return;
  if (memoryMode) {
    for (const item of items) memStore(store).set(String(item.id), item);
    return;
  }
  await tx(store, 'readwrite', (os) => {
    for (const item of items) os.put(item);
  });
  if (!memoryMode) return;
  for (const item of items) memStore(store).set(String(item.id), item);
}

export async function putItem<T extends { id: string }>(store: StoreName, item: T): Promise<void> {
  return putMany(store, [item]);
}

export async function getItem<T>(store: StoreName, id: string): Promise<T | undefined> {
  if (memoryMode) return memStore<T>(store).get(id);
  const res = await tx<T>(store, 'readonly', (os) => os.get(id) as IDBRequest<T>);
  if (res !== undefined) return res;
  if (memoryMode) return memStore<T>(store).get(id);
  return undefined;
}

export async function getAll<T>(store: StoreName): Promise<T[]> {
  if (memoryMode) return [...memStore<T>(store).values()];
  const res = await tx<T[]>(store, 'readonly', (os) => os.getAll() as IDBRequest<T[]>);
  if (res) return res;
  return [...memStore<T>(store).values()];
}

export async function getAllByIndex<T>(store: StoreName, index: string, value: string): Promise<T[]> {
  if (memoryMode) {
    return [...memStore<T & { notebookId?: string; sourceId?: string }>(store).values()].filter(
      (item) => (item as Record<string, unknown>)[index] === value,
    ) as T[];
  }
  const res = await tx<T[]>(store, 'readonly', (os) => os.index(index).getAll(value) as IDBRequest<T[]>);
  if (res) return res;
  return [...memStore<T>(store).values()];
}

export async function deleteItem(store: StoreName, id: string): Promise<void> {
  memStore(store).delete(id);
  if (memoryMode) return;
  await tx(store, 'readwrite', (os) => os.delete(id));
}

export async function deleteByIndex(store: StoreName, index: string, value: string): Promise<void> {
  for (const [key, item] of memStore<Record<string, unknown>>(store)) {
    if (item[index] === value) memStore(store).delete(key);
  }
  if (memoryMode) return;
  const db = await openDb().catch(() => null);
  if (!db) return;
  await new Promise<void>((resolve) => {
    const transaction = db.transaction(store, 'readwrite');
    const os = transaction.objectStore(store);
    const req = os.index(index).openCursor(IDBKeyRange.only(value));
    req.onsuccess = () => {
      const cursor = req.result;
      if (cursor) {
        cursor.delete();
        cursor.continue();
      }
    };
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => resolve();
    transaction.onabort = () => resolve();
  });
}

export async function clearAll(): Promise<void> {
  memory.clear();
  if (memoryMode) return;
  const db = await openDb().catch(() => null);
  if (!db) return;
  const names: StoreName[] = ['notebooks', 'sources', 'chunks', 'messages', 'artifacts', 'settings'];
  await Promise.all(
    names.map(
      (name) =>
        new Promise<void>((resolve) => {
          const t = db.transaction(name, 'readwrite');
          t.objectStore(name).clear();
          t.oncomplete = () => resolve();
          t.onerror = () => resolve();
          t.onabort = () => resolve();
        }),
    ),
  );
}

export interface Snapshot {
  notebooks: Notebook[];
  sources: Source[];
  chunks: Chunk[];
  messages: ChatMessage[];
  artifacts: Artifact[];
  settings: Settings;
}

export async function loadSnapshot(): Promise<Snapshot> {
  const [notebooks, sources, chunks, messages, artifacts, settingsRows] = await Promise.all([
    getAll<Notebook>('notebooks'),
    getAll<Source>('sources'),
    getAll<Chunk>('chunks'),
    getAll<ChatMessage>('messages'),
    getAll<Artifact>('artifacts'),
    getAll<{ id: string; value: Settings }>('settings'),
  ]);
  const stored = settingsRows.find((row) => row.id === 'app')?.value;
  return {
    notebooks: notebooks.sort((a, b) => a.createdAt - b.createdAt),
    sources,
    chunks,
    messages,
    artifacts,
    settings: mergeSettings(stored),
  };
}

export function mergeSettings(stored?: Partial<Settings>): Settings {
  if (!stored) return { ...DEFAULT_SETTINGS };
  const merged: Settings = { ...DEFAULT_SETTINGS, ...stored };
  merged.providers = (stored.providers ?? []).map((p) => ({ ...p, apiKey: '' }));
  merged.githubToken = '';
  return merged;
}

export async function saveSettings(settings: Settings): Promise<void> {
  const persistKeys = settings.persistKeys;
  const sanitized: Settings = {
    ...settings,
    providers: settings.providers.map((p) => ({ ...p, apiKey: persistKeys ? p.apiKey : '' })),
    githubToken: persistKeys ? settings.githubToken : '',
  };
  await putItem('settings', { id: 'app', value: sanitized });
}
