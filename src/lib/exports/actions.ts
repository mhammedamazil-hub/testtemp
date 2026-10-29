/**
 * User-facing export/import actions. Every artifact is generated, validated
 * and only then offered for download; failures surface as toasts.
 */

import { downloadBytes, downloadText, slugify } from '../download';
import { actions, notebookArtifacts, notebookSources, store } from '../store';
import { buildBackup, parseBackup, remapBackup, serializeBackup, summarizeBackup, BackupError } from '../backup';
import type { Artifact } from '../types';
import { isValidPdf, buildPdf } from './pdf';
import { buildPptx, isValidPptx } from './pptx';
import { artifactMarkdown, notebookMarkdown, studyReportHtml } from './report';
import { flashcardsCsv, quizCsv } from './csv';

function notebookContext(notebookId: string) {
  const state = store.get();
  const notebook = state.notebooks.find((n) => n.id === notebookId);
  if (!notebook) throw new Error('Notebook not found');
  return {
    notebook,
    sources: notebookSources(state, notebookId),
    artifacts: notebookArtifacts(state, notebookId),
  };
}

export async function exportBackupFile(): Promise<void> {
  const state = store.get();
  const backup = buildBackup(state);
  const name = `sourceforge-backup-${new Date().toISOString().slice(0, 10)}.json`;
  downloadText(serializeBackup(backup), name, 'application/json');
  actions.toast('success', `Backup exported (${summarizeBackup(backup)}).`, 'Keys and tokens are never included in backups.');
}

export async function importBackupFromFile(file: File): Promise<void> {
  try {
    const text = await file.text();
    const backup = parseBackup(text);
    const existing = new Set(store.get().notebooks.map((n) => n.id));
    const payload = remapBackup(backup, existing);
    await actions.importData(payload);
    actions.toast(
      'success',
      `Restored ${summarizeBackup(backup)}.`,
      payload.renamed ? `${payload.renamed} notebook id(s) were remapped to avoid collisions.` : undefined,
    );
  } catch (err) {
    if (err instanceof BackupError) {
      actions.toast('error', err.message, err.detail);
    } else {
      actions.toast('error', `Could not import the backup: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
}

export function exportNotebookMarkdown(notebookId: string): void {
  const ctx = notebookContext(notebookId);
  const markdown = notebookMarkdown({ ...ctx, exportedAt: new Date().toISOString() });
  downloadText(markdown, `${slugify(ctx.notebook.title)}.md`, 'text/markdown;charset=utf-8');
  actions.toast('success', 'Markdown export downloaded.');
}

export function exportHtmlReport(notebookId: string): void {
  const ctx = notebookContext(notebookId);
  const html = studyReportHtml({ ...ctx, exportedAt: new Date().toISOString() });
  downloadText(html, `${slugify(ctx.notebook.title)}-study-report.html`, 'text/html;charset=utf-8');
  actions.toast('success', 'Printable HTML study report downloaded.');
}

export async function exportPdfReport(notebookId: string): Promise<void> {
  try {
    const ctx = notebookContext(notebookId);
    const bytes = await buildPdf({
      title: ctx.notebook.title,
      subtitle: `${ctx.sources.length} sources · ${ctx.artifacts.length} study items · exported ${new Date().toISOString().slice(0, 10)}`,
      artifacts: ctx.artifacts,
      sources: ctx.sources.map((s) => ({ title: s.title, text: s.text })),
    });
    if (!isValidPdf(bytes)) throw new Error('Generated PDF failed validation');
    downloadBytes(bytes, `${slugify(ctx.notebook.title)}.pdf`, 'application/pdf');
    actions.toast('success', `PDF report downloaded (${(bytes.length / 1024).toFixed(0)} KB).`);
  } catch (err) {
    actions.toast('error', `PDF export failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export async function exportPptxDeck(notebookId: string): Promise<void> {
  try {
    const ctx = notebookContext(notebookId);
    if (!ctx.artifacts.length) {
      actions.toast('warning', 'Generate study material first — the deck is built from notebook artifacts.');
      return;
    }
    const bytes = await buildPptx({
      title: ctx.notebook.title,
      subtitle: `${ctx.sources.length} sources · citations included`,
      artifacts: ctx.artifacts,
    });
    if (!(await isValidPptx(bytes))) throw new Error('Generated PPTX failed validation');
    downloadBytes(bytes, `${slugify(ctx.notebook.title)}.pptx`, 'application/vnd.openxmlformats-officedocument.presentationml.presentation');
    actions.toast('success', `Presentation downloaded (${(bytes.length / 1024).toFixed(0)} KB, valid PPTX).`);
  } catch (err) {
    actions.toast('error', `PPTX export failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export function exportArtifactMarkdownFile(artifact: Artifact): void {
  downloadText(artifactMarkdown(artifact), `${slugify(artifact.title)}.md`, 'text/markdown;charset=utf-8');
  actions.toast('success', 'Study material downloaded as Markdown.');
}

export function exportArtifactCsv(artifact: Artifact): void {
  if (artifact.kind === 'flashcards') {
    const csv = flashcardsCsv(artifact);
    downloadText(csv, `${slugify(artifact.title)}.csv`, 'text/csv;charset=utf-8');
    actions.toast('success', 'Flashcards exported as CSV.');
    return;
  }
  if (artifact.kind === 'quiz') {
    const csv = quizCsv(artifact);
    downloadText(csv, `${slugify(artifact.title)}.csv`, 'text/csv;charset=utf-8');
    actions.toast('success', 'Quiz exported as CSV.');
    return;
  }
  downloadText(artifactMarkdown(artifact), `${slugify(artifact.title)}.md`, 'text/markdown;charset=utf-8');
  actions.toast('info', 'CSV is available for flashcards and quizzes; Markdown exported instead.');
}
