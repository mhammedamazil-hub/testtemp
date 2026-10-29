/** CSV export for flashcards and quizzes. */

import type { Artifact } from '../types';
import type { Flashcard, QuizQuestion } from '../study';

export const CSV_INJECTION_GUARD = /^[=+\-@\t\r]/;

/** Escape a CSV cell and neutralise spreadsheet formula injection. */
export function csvCell(value: unknown): string {
  let text = value === null || value === undefined ? '' : String(value);
  if (CSV_INJECTION_GUARD.test(text)) text = `'${text}`;
  if (/[",\n\r]/.test(text)) return `"${text.replace(/"/g, '""')}"`;
  return text;
}

export function toCsv(rows: unknown[][]): string {
  return rows.map((row) => row.map(csvCell).join(',')).join('\r\n') + '\r\n';
}

export function flashcardsCsv(artifact: Artifact): string {
  const cards = (artifact.data as { cards?: Flashcard[] } | undefined)?.cards ?? [];
  const rows: unknown[][] = [['front', 'back', 'citations', 'sources']];
  for (const card of cards) {
    const citations = card.citation ?? [];
    const sources = citations
      .map((m) => artifact.citations.find((c) => c.marker === m))
      .filter((c): c is NonNullable<typeof c> => Boolean(c))
      .map((c) => `${c.sourceTitle} (${c.locator})`)
      .join(' | ');
    rows.push([card.front, card.back, citations.map((c) => `[${c}]`).join(' '), sources]);
  }
  return toCsv(rows);
}

export function quizCsv(artifact: Artifact): string {
  const questions = (artifact.data as { questions?: QuizQuestion[] } | undefined)?.questions ?? [];
  const rows: unknown[][] = [['question', 'option_a', 'option_b', 'option_c', 'option_d', 'answer', 'explanation', 'citations']];
  for (const q of questions) {
    const options = [...q.options];
    while (options.length < 4) options.push('');
    rows.push([
      q.question,
      options[0],
      options[1],
      options[2],
      options[3],
      String.fromCharCode(65 + q.answerIndex),
      q.explanation,
      (q.citation ?? []).map((c) => `[${c}]`).join(' '),
    ]);
  }
  return toCsv(rows);
}

export function validateCsv(text: string): { ok: boolean; rows: number; columns: number } {
  const lines = text.split(/\r?\n/).filter((l) => l.length);
  if (!lines.length) return { ok: false, rows: 0, columns: 0 };
  let columns = 0;
  let inQuotes = false;
  for (const ch of lines[0]) {
    if (ch === '"') inQuotes = !inQuotes;
    else if (ch === ',' && !inQuotes) columns += 1;
  }
  columns += 1;
  const ok = lines.every((line) => {
    let quotes = 0;
    for (const ch of line) if (ch === '"') quotes += 1;
    return quotes % 2 === 0;
  });
  return { ok, rows: lines.length, columns };
}
