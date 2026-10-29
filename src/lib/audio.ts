/**
 * Audio overview workflow.
 *
 * 1. Build a source-grounded script (AI two-host discussion, or deterministic
 *    dialogue when no provider is configured).
 * 2. Split the script into speech chunks at line boundaries.
 * 3. Synthesize each chunk with a TTS-capable provider.
 *
 * If no TTS provider is configured, the script is still produced and the UI
 * clearly reports that audio generation is unavailable — we never present a
 * script as an audio file.
 */

import { ProviderError, providerForTask, synthesizeSpeech } from './providers';
import { generateArtifact, type StudyContext } from './study';
import type { Artifact } from './types';

export interface AudioPart {
  index: number;
  blob: Blob;
  url: string;
  text: string;
  startMs: number;
  endMs: number;
  mime: string;
}

export interface AudioOverviewResult {
  script: Artifact;
  parts: AudioPart[];
  available: boolean;
  reason?: string;
  totalMs: number;
  providerId?: string;
}

export function estimateSpeechMs(text: string, wordsPerMinute = 155): number {
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  return Math.round((words / wordsPerMinute) * 60000);
}

/** Split speaker lines into TTS-safe chunks without breaking sentences. */
export function splitForSpeech(script: string, maxChars = 3000): string[] {
  const lines = script.split('\n');
  const chunks: string[] = [];
  let current = '';
  for (const line of lines) {
    const clean = line.trim();
    if (!clean) continue;
    if (current.length + clean.length + 1 > maxChars && current) {
      chunks.push(current.trim());
      current = '';
    }
    if (clean.length > maxChars) {
      for (const sentence of clean.split(/(?<=[.!?])\s+/)) {
        if (current.length + sentence.length + 1 > maxChars && current) {
          chunks.push(current.trim());
          current = '';
        }
        current += `${sentence} `;
      }
      continue;
    }
    current += `${clean}\n`;
  }
  if (current.trim()) chunks.push(current.trim());
  return chunks;
}

export interface AudioOptions {
  style?: 'discussion' | 'briefing';
  minutes?: number;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number, label: string) => void;
}

export async function generateAudioOverview(ctx: StudyContext, options: AudioOptions = {}): Promise<AudioOverviewResult> {
  const generated = await generateArtifact('audio-script', ctx, { topic: ctx.notebookTitle });
  const script = generated.artifact;
  const profile = providerForTask(ctx.settings, 'tts');
  const estimate = estimateSpeechMs(script.body);
  if (!profile) {
    return {
      script,
      parts: [],
      available: false,
      reason:
        'Speech synthesis needs a provider with a TTS endpoint (e.g. OpenAI /audio/speech or Groq PlayAI). The script is ready and downloadable; configure a TTS provider in Settings to render audio.',
      totalMs: estimate,
    };
  }
  const plain = script.body
    .replace(/\[(\d+)\]/g, '')
    .replace(/^#+\s*/gm, '')
    .replace(/[*_`>]/g, '')
    .trim();
  const chunks = splitForSpeech(plain);
  const parts: AudioPart[] = [];
  let elapsed = 0;
  try {
    for (const [index, text] of chunks.entries()) {
      options.onProgress?.(index, chunks.length, `Synthesizing part ${index + 1} of ${chunks.length}`);
      const blob = await synthesizeSpeech(profile, text, options.signal);
      const duration = estimateSpeechMs(text);
      parts.push({
        index,
        blob,
        url: URL.createObjectURL(blob),
        text,
        startMs: elapsed,
        endMs: elapsed + duration,
        mime: blob.type || 'audio/mpeg',
      });
      elapsed += duration;
    }
    return { script, parts, available: true, totalMs: elapsed, providerId: profile.id };
  } catch (err) {
    const reason =
      err instanceof ProviderError ? `${err.code}: ${err.message}` : err instanceof Error ? err.message : String(err);
    return {
      script,
      parts,
      available: parts.length > 0,
      reason: `Speech synthesis failed: ${reason}`,
      totalMs: elapsed || estimate,
      providerId: profile.id,
    };
  }
}

export function transcriptWithTimestamps(result: AudioOverviewResult): string {
  if (!result.parts.length) return `(estimated ${Math.round(result.totalMs / 1000)}s)\n\n${result.script.body}`;
  const clock = (ms: number) => {
    const total = Math.floor(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  };
  return result.parts.map((p) => `[${clock(p.startMs)}–${clock(p.endMs)}]\n${p.text}`).join('\n\n');
}

export async function packAudioParts(result: AudioOverviewResult, filenameBase: string): Promise<Blob> {
  const JSZip = (await import('jszip')).default;
  const zip = new JSZip();
  result.parts.forEach((part) => {
    zip.file(`${filenameBase}-part${String(part.index + 1).padStart(2, '0')}.mp3`, part.blob);
  });
  zip.file(`${filenameBase}-transcript.md`, transcriptWithTimestamps(result));
  zip.file(`${filenameBase}-script.md`, result.script.body);
  return zip.generateAsync({ type: 'blob' });
}
