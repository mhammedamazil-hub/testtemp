/**
 * Versioned notebook backups: export, validation, migration and restore.
 *
 * Validation is strict about shape and version, generous about optional
 * fields, and never writes partial data: a backup either imports completely
 * or fails with a readable explanation.
 */

import { hashString } from './text';
import {
  BACKUP_FORMAT,
  BACKUP_VERSION,
  SCHEMA_VERSION,
  type Artifact,
  type BackupFile,
  type BackupNotebook,
  type ChatMessage,
  type Notebook,
  type Source,
  type SourceKind,
} from './types';

export class BackupError extends Error {
  constructor(message: string, public detail?: string) {
    super(message);
    this.name = 'BackupError';
  }
}

const SOURCE_KINDS: SourceKind[] = [
  'text', 'markdown', 'csv', 'json', 'html', 'code', 'pdf', 'docx', 'pptx', 'github-file', 'github-repo',
  'web', 'youtube', 'instagram', 'media-transcript', 'image-analysis',
];

export function buildBackup(input: {
  notebooks: Notebook[];
  sources: Source[];
  artifacts: Artifact[];
  messages: ChatMessage[];
}): BackupFile {
  const notebooks: BackupNotebook[] = input.notebooks.map((notebook) => ({
    notebook,
    sources: input.sources.filter((s) => s.notebookId === notebook.id),
    artifacts: input.artifacts.filter((a) => a.notebookId === notebook.id),
    messages: input.messages.filter((m) => m.notebookId === notebook.id),
  }));
  return {
    format: BACKUP_FORMAT,
    version: BACKUP_VERSION,
    schemaVersion: SCHEMA_VERSION,
    exportedAt: new Date().toISOString(),
    app: 'SourceForge',
    notebooks,
    meta: {
      notebookCount: notebooks.length,
      sourceCount: input.sources.length,
    },
  };
}

export function serializeBackup(backup: BackupFile): string {
  return JSON.stringify(backup, null, 2);
}

/** Accept either the current shape or a bare array of notebooks (v0 imports). */
export function parseBackup(text: string): BackupFile {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (err) {
    throw new BackupError('This file is not valid JSON.', err instanceof Error ? err.message : undefined);
  }
  if (typeof json !== 'object' || json === null) throw new BackupError('The backup root must be a JSON object.');
  const record = json as Record<string, unknown>;
  if (record.format !== BACKUP_FORMAT) {
    throw new BackupError(
      `Unrecognised file format “${String(record.format ?? 'missing')}”. SourceForge backups must have format “${BACKUP_FORMAT}”.`,
      'If this is a notebook export from another app it cannot be restored directly. Paste its text as a source instead.',
    );
  }
  const version = Number(record.version);
  if (!Number.isFinite(version)) throw new BackupError('The backup has no numeric version.');
  if (version > BACKUP_VERSION) {
    throw new BackupError(
      `This backup was created by a newer version of SourceForge (schema v${version}); this build supports up to v${BACKUP_VERSION}.`,
      'Update the app, then retry the import.',
    );
  }
  if (!Array.isArray(record.notebooks)) throw new BackupError('The backup has no notebooks array.');
  if (record.notebooks.length > 500) throw new BackupError('The backup contains more than 500 notebooks and was rejected.');
  const notebooks: BackupNotebook[] = record.notebooks.map((entry, index) => validateNotebook(entry, index));
  return {
    format: BACKUP_FORMAT,
    version,
    schemaVersion: Number(record.schemaVersion ?? 1),
    exportedAt: typeof record.exportedAt === 'string' ? record.exportedAt : new Date().toISOString(),
    app: typeof record.app === 'string' ? record.app : 'SourceForge',
    notebooks,
  };
}

function validateNotebook(entry: unknown, index: number): BackupNotebook {
  if (typeof entry !== 'object' || entry === null) throw new BackupError(`Notebook #${index + 1} is not an object.`);
  const record = entry as Record<string, unknown>;
  const notebook = validateNotebookMeta(record.notebook, index);
  const sources = Array.isArray(record.sources) ? record.sources.map((s, i) => validateSource(s, notebook.id, i)) : [];
  const artifacts = Array.isArray(record.artifacts) ? record.artifacts.map((a, i) => validateArtifact(a, notebook.id, i)) : [];
  const messages = Array.isArray(record.messages) ? record.messages.map((m, i) => validateMessage(m, notebook.id, i)) : [];
  if (sources.length > 2000) throw new BackupError(`Notebook “${notebook.title}” has too many sources (${sources.length}).`);
  return { notebook, sources, artifacts, messages };
}

function str(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function num(value: unknown, fallback = Date.now()): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function validateNotebookMeta(value: unknown, index: number): Notebook {
  if (typeof value !== 'object' || value === null) throw new BackupError(`Notebook #${index + 1} is missing its metadata.`);
  const record = value as Record<string, unknown>;
  const id = str(record.id).trim();
  if (!id) throw new BackupError(`Notebook #${index + 1} has no id.`);
  const title = str(record.title).trim() || 'Imported notebook';
  return {
    id,
    title,
    description: str(record.description),
    emoji: str(record.emoji, '📓') || '📓',
    createdAt: num(record.createdAt),
    updatedAt: num(record.updatedAt),
  };
}

function validateSource(value: unknown, notebookId: string, index: number): Source {
  if (typeof value !== 'object' || value === null) throw new BackupError(`Source #${index + 1} is not an object.`);
  const record = value as Record<string, unknown>;
  const id = str(record.id).trim();
  if (!id) throw new BackupError(`Source #${index + 1} has no id.`);
  const text = str(record.text);
  const kindRaw = str(record.kind, 'text') as SourceKind;
  const kind = SOURCE_KINDS.includes(kindRaw) ? kindRaw : 'text';
  const segments = Array.isArray(record.segments)
    ? record.segments
        .filter((s): s is Record<string, unknown> => typeof s === 'object' && s !== null)
        .map((s, i) => ({
          index: num(s.index, i),
          text: str(s.text),
          startLine: num(s.startLine, 1),
          endLine: num(s.endLine, 1),
          ...(typeof s.page === 'number' ? { page: s.page } : {}),
          ...(typeof s.startMs === 'number' ? { startMs: s.startMs } : {}),
          ...(typeof s.endMs === 'number' ? { endMs: s.endMs } : {}),
          ...(typeof s.section === 'string' ? { section: s.section } : {}),
        }))
    : [];
  const statusRaw = str(record.status, 'ready');
  return {
    id,
    notebookId,
    title: str(record.title).trim() || 'Imported source',
    kind,
    origin: typeof record.origin === 'object' && record.origin !== null ? (record.origin as Source['origin']) : {},
    text,
    segments: segments.length ? segments : [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }],
    status: statusRaw === 'partial' || statusRaw === 'error' ? statusRaw : 'ready',
    statusDetail: str(record.statusDetail, 'Imported from backup.'),
    warnings: Array.isArray(record.warnings) ? record.warnings.filter((w): w is string => typeof w === 'string').slice(0, 20) : [],
    byteSize: num(record.byteSize, text.length),
    checksum: str(record.checksum) || hashString(text),
    createdAt: num(record.createdAt),
    updatedAt: num(record.updatedAt),
  };
}

function validateArtifact(value: unknown, notebookId: string, index: number): Artifact {
  const record = (typeof value === 'object' && value !== null ? value : {}) as Record<string, unknown>;
  const id = str(record.id).trim() || `art-imported-${index}-${hashString(JSON.stringify(value)).slice(0, 6)}`;
  return {
    id,
    notebookId,
    kind: (str(record.kind, 'notes') as Artifact['kind']),
    title: str(record.title, 'Imported artifact'),
    body: str(record.body),
    data: record.data,
    citations: Array.isArray(record.citations) ? (record.citations as Artifact['citations']) : [],
    mode: record.mode === 'ai' ? 'ai' : 'local',
    ...(typeof record.model === 'string' ? { model: record.model } : {}),
    createdAt: num(record.createdAt),
    updatedAt: num(record.updatedAt),
  };
}

function validateMessage(value: unknown, notebookId: string, index: number): ChatMessage {
  const record = (typeof value === 'object' && value !== null ? value : {}) as Record<string, unknown>;
  const role = str(record.role, 'assistant');
  return {
    id: str(record.id).trim() || `msg-imported-${index}-${hashString(JSON.stringify(value)).slice(0, 6)}`,
    notebookId,
    role: role === 'user' || role === 'system' ? role : 'assistant',
    content: str(record.content),
    createdAt: num(record.createdAt),
    ...(Array.isArray(record.citations) ? { citations: record.citations as ChatMessage['citations'] } : {}),
    ...(record.insufficientEvidence === true ? { insufficientEvidence: true } : {}),
    ...(record.conflicting === true ? { conflicting: true } : {}),
    ...(typeof record.model === 'string' ? { model: record.model } : {}),
    ...(typeof record.providerId === 'string' ? { providerId: record.providerId } : {}),
    ...(typeof record.error === 'string' ? { error: record.error } : {}),
    ...(record.mode === 'ai' || record.mode === 'local' ? { mode: record.mode } : {}),
  };
}

/** Restore a backup into a fresh id space, avoiding collisions with existing data. */
export function remapBackup(
  backup: BackupFile,
  existingNotebookIds: Set<string>,
): { notebooks: Notebook[]; sources: Source[]; artifacts: Artifact[]; messages: ChatMessage[]; renamed: number } {
  const notebooks: Notebook[] = [];
  const sources: Source[] = [];
  const artifacts: Artifact[] = [];
  const messages: ChatMessage[] = [];
  let renamed = 0;
  for (const entry of backup.notebooks) {
    let notebookId = entry.notebook.id;
    if (existingNotebookIds.has(notebookId)) {
      notebookId = `${notebookId}-r${Math.random().toString(36).slice(2, 6)}`;
      renamed += 1;
    }
    existingNotebookIds.add(notebookId);
    notebooks.push({ ...entry.notebook, id: notebookId, updatedAt: Date.now() });
    for (const source of entry.sources) {
      sources.push({ ...source, id: `${source.id}-${notebookId.slice(-4)}`, notebookId });
    }
    for (const artifact of entry.artifacts) {
      artifacts.push({ ...artifact, id: `${artifact.id}-${notebookId.slice(-4)}`, notebookId });
    }
    for (const message of entry.messages) {
      messages.push({ ...message, id: `${message.id}-${notebookId.slice(-4)}`, notebookId });
    }
  }
  return { notebooks, sources, artifacts, messages, renamed };
}

export function summarizeBackup(backup: BackupFile): string {
  const sources = backup.notebooks.reduce((sum, n) => sum + n.sources.length, 0);
  return `${backup.notebooks.length} notebook${backup.notebooks.length === 1 ? '' : 's'}, ${sources} source${sources === 1 ? '' : 's'} (schema v${backup.version})`;
}
