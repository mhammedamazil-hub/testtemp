import { describe, expect, it } from 'vitest';
import { BackupError, buildBackup, parseBackup, remapBackup, serializeBackup, summarizeBackup } from '../src/lib/backup';
import { BACKUP_FORMAT, BACKUP_VERSION, type Artifact, type ChatMessage, type Notebook, type Source } from '../src/lib/types';

const notebook: Notebook = { id: 'nb-1', title: 'Research', description: 'desc', emoji: '📓', createdAt: 10, updatedAt: 20 };

const source: Source = {
  id: 'src-1',
  notebookId: 'nb-1',
  title: 'Report',
  kind: 'text',
  origin: { fileName: 'report.txt' },
  text: 'Some grounded content about renewal rates.',
  segments: [{ index: 0, text: 'Some grounded content about renewal rates.', startLine: 1, endLine: 1 }],
  status: 'ready',
  statusDetail: 'ok',
  warnings: [],
  byteSize: 42,
  checksum: 'abc',
  createdAt: 1,
  updatedAt: 1,
};

const artifact: Artifact = {
  id: 'art-1', notebookId: 'nb-1', kind: 'notes', title: 'Notes', body: 'Body text', citations: [],
  mode: 'local', createdAt: 2, updatedAt: 2,
};

const message: ChatMessage = {
  id: 'm-1', notebookId: 'nb-1', role: 'assistant', content: 'Answer [1]', createdAt: 3, mode: 'local',
};

describe('backup export and restore', () => {
  it('round-trips notebooks, sources, artifacts and messages', () => {
    const backup = buildBackup({ notebooks: [notebook], sources: [source], artifacts: [artifact], messages: [message] });
    expect(backup.format).toBe(BACKUP_FORMAT);
    expect(backup.version).toBe(BACKUP_VERSION);
    expect(backup.meta).toMatchObject({ notebookCount: 1, sourceCount: 1 });

    const parsed = parseBackup(serializeBackup(backup));
    expect(parsed.notebooks).toHaveLength(1);
    expect(parsed.notebooks[0].sources[0]).toMatchObject({ id: 'src-1', title: 'Report', text: source.text });
    expect(parsed.notebooks[0].messages[0].content).toBe('Answer [1]');
    expect(summarizeBackup(parsed)).toMatch(/1 notebook, 1 source/);
  });

  it('rejects invalid JSON, wrong formats, future versions and malformed notebooks', () => {
    expect(() => parseBackup('{')).toThrow(BackupError);
    expect(() => parseBackup('{"format":"other","version":1,"notebooks":[]}')).toThrow(/Unrecognised file format/);
    expect(() => parseBackup(JSON.stringify({ format: BACKUP_FORMAT, version: 99, notebooks: [] }))).toThrow(/newer version/);
    expect(() => parseBackup(JSON.stringify({ format: BACKUP_FORMAT, version: 1 }))).toThrow(/no notebooks array/);
    expect(() => parseBackup(JSON.stringify({ format: BACKUP_FORMAT, version: 1, notebooks: [{}] }))).toThrow(/missing its metadata/);
    expect(() => parseBackup(JSON.stringify({ format: BACKUP_FORMAT, version: 1, notebooks: [{ notebook: { title: 'no id' } }] }))).toThrow(/no id/);
  });

  it('tolerates missing optional fields and repairs bad kinds', () => {
    const minimal = {
      format: BACKUP_FORMAT,
      version: 1,
      notebooks: [
        {
          notebook: { id: 'nb-min', title: '' },
          sources: [{ id: 's', text: 'hello', kind: 'not-a-kind' }],
          artifacts: [{ body: 'loose artifact' }],
          messages: [{ content: 'loose message' }],
        },
      ],
    };
    const parsed = parseBackup(JSON.stringify(minimal));
    expect(parsed.notebooks[0].notebook.title).toBe('Imported notebook');
    expect(parsed.notebooks[0].sources[0].kind).toBe('text');
    expect(parsed.notebooks[0].sources[0].segments).toHaveLength(1);
    expect(parsed.notebooks[0].artifacts[0].id).toMatch(/art-imported/);
    expect(parsed.notebooks[0].messages[0].role).toBe('assistant');
  });

  it('remaps ids to avoid collisions and keeps notebook relationships', () => {
    const backup = buildBackup({ notebooks: [notebook], sources: [source], artifacts: [artifact], messages: [message] });
    const payload = remapBackup(backup, new Set(['nb-1']));
    expect(payload.renamed).toBe(1);
    const newNotebookId = payload.notebooks[0].id;
    expect(newNotebookId).not.toBe('nb-1');
    expect(payload.sources[0].notebookId).toBe(newNotebookId);
    expect(payload.artifacts[0].notebookId).toBe(newNotebookId);
    expect(payload.messages[0].notebookId).toBe(newNotebookId);
    expect(payload.sources[0].id).not.toBe('src-1');
  });

  it('imports a backup without collisions unchanged', () => {
    const backup = buildBackup({ notebooks: [notebook], sources: [source], artifacts: [], messages: [] });
    const payload = remapBackup(backup, new Set());
    expect(payload.renamed).toBe(0);
    expect(payload.notebooks[0].id).toBe('nb-1');
  });
});
