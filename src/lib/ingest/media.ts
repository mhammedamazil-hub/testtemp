/**
 * Browser media pipeline.
 *
 * Feasible in a browser: decode a user-supplied audio/video file, sample
 * representative frames on a canvas, and send audio to a configured
 * transcription provider. NOT feasible: invoking ffmpeg/yt-dlp or downloading
 * YouTube media — those need a native helper that this app does not ship.
 */

import type { ProviderProfile, Segment } from '../types';
import { transcribeAudio, ProviderError } from '../providers';
import { IngestionError } from './files';

export const AUDIO_EXTENSIONS = ['mp3', 'm4a', 'wav', 'ogg', 'oga', 'opus', 'flac', 'aac', 'webm'];
export const VIDEO_EXTENSIONS = ['mp4', 'mov', 'mkv', 'webm', 'avi', 'm4v', 'ogv'];

export function mediaTypeOf(name: string): 'audio' | 'video' | null {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  if (AUDIO_EXTENSIONS.includes(ext)) return 'audio';
  if (VIDEO_EXTENSIONS.includes(ext)) return 'video';
  return null;
}

export interface ExtractedFrame {
  timeMs: number;
  dataUrl: string;
  width: number;
  height: number;
}

export interface FrameOptions {
  intervalSeconds?: number;
  maxFrames?: number;
  maxWidth?: number;
  signal?: AbortSignal;
  onProgress?: (done: number, total: number) => void;
}

/**
 * Sample frames at a fixed interval (bounded) instead of decoding every frame.
 * Requires a real browser; returns a typed error when the platform cannot.
 */
export async function extractFrames(file: File, options: FrameOptions = {}): Promise<ExtractedFrame[]> {
  const interval = options.intervalSeconds ?? 45;
  const maxFrames = options.maxFrames ?? 6;
  const maxWidth = options.maxWidth ?? 640;
  if (typeof document === 'undefined' || typeof URL.createObjectURL !== 'function') {
    throw new IngestionError('unsupported', 'Frame extraction needs a browser with HTMLVideoElement support.');
  }
  const url = URL.createObjectURL(file);
  const video = document.createElement('video');
  video.preload = 'metadata';
  video.muted = true;
  video.playsInline = true;
  video.src = url;
  const cleanup = () => {
    video.removeAttribute('src');
    video.load();
    URL.revokeObjectURL(url);
  };
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new IngestionError('corrupt', 'The video metadata did not load (unsupported codec?).')), 20000);
      video.onloadedmetadata = () => {
        clearTimeout(timer);
        resolve();
      };
      video.onerror = () => {
        clearTimeout(timer);
        reject(new IngestionError('corrupt', 'The browser could not decode this video file.'));
      };
    });
    const duration = Number.isFinite(video.duration) ? video.duration : 0;
    if (!duration) throw new IngestionError('corrupt', 'The video has no readable duration.');
    const times: number[] = [];
    for (let t = Math.min(interval / 2, duration / 2); t < duration && times.length < maxFrames; t += interval) times.push(t);
    if (!times.length) times.push(0);
    const canvas = document.createElement('canvas');
    const frames: ExtractedFrame[] = [];
    for (const [i, t] of times.entries()) {
      if (options.signal?.aborted) throw new IngestionError('blocked', 'Frame extraction cancelled.');
      await seek(video, t);
      const width = video.videoWidth || 640;
      const height = video.videoHeight || 360;
      const scale = Math.min(1, maxWidth / width);
      canvas.width = Math.round(width * scale);
      canvas.height = Math.round(height * scale);
      const ctx = canvas.getContext('2d');
      if (!ctx) break;
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      frames.push({ timeMs: Math.round(t * 1000), dataUrl: canvas.toDataURL('image/jpeg', 0.6), width: canvas.width, height: canvas.height });
      options.onProgress?.(i + 1, times.length);
    }
    return frames;
  } finally {
    cleanup();
  }
}

function seek(video: HTMLVideoElement, time: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new IngestionError('corrupt', 'Seeking the video timed out.')), 15000);
    const done = () => {
      clearTimeout(timer);
      video.removeEventListener('seeked', done);
      resolve();
    };
    video.addEventListener('seeked', done, { once: true });
    video.currentTime = time;
  });
}

export function segmentsFromTranscription(
  segments: { start: number; end: number; text: string }[] | undefined,
  text: string,
): Segment[] {
  if (segments?.length) {
    return segments.map((s, i) => ({
      index: i,
      text: s.text.trim(),
      startMs: s.start,
      endMs: s.end,
      startLine: 0,
      endLine: 0,
    })).map((s) => ({ ...s, startLine: s.index + 1, endLine: s.index + 1 }));
  }
  return [{ index: 0, text, startLine: 1, endLine: Math.max(1, text.split('\n').length) }];
}

export async function transcribeMediaFile(
  file: File,
  profile: ProviderProfile,
  options: { signal?: AbortSignal } = {},
): Promise<{ text: string; segments: Segment[]; language?: string }> {
  if (file.size > 24 * 1024 * 1024) {
    throw new IngestionError('too-large', 'Audio files above 24 MB are rejected by most provider endpoints. Trim the file first.');
  }
  try {
    const result = await transcribeAudio(profile, file, options.signal);
    if (!result.text.trim()) throw new IngestionError('empty', 'The transcription provider returned no text.');
    const out: { text: string; segments: Segment[]; language?: string } = {
      text: result.text,
      segments: segmentsFromTranscription(result.segments, result.text),
    };
    if (result.language) out.language = result.language;
    return out;
  } catch (err) {
    if (err instanceof IngestionError) throw err;
    if (err instanceof ProviderError) {
      throw new IngestionError('blocked', `Transcription failed: ${err.message}`, 'Check the provider key, CORS support and the audio format.');
    }
    throw new IngestionError('blocked', `Transcription failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export interface FrameAnalysis {
  timeMs: number;
  description: string;
}

/** Optional vision pass over sampled frames. Requires a vision-capable model. */
export async function analyzeFrames(
  frames: ExtractedFrame[],
  profile: ProviderProfile,
  options: { signal?: AbortSignal; model?: string; context?: string } = {},
): Promise<FrameAnalysis[]> {
  const { runChat, resolveModel } = await import('../providers');
  const results: FrameAnalysis[] = [];
  for (const frame of frames) {
    if (options.signal?.aborted) break;
    const result = await runChat(profile, {
      task: 'vision',
      request: {
        model: options.model ?? resolveModel(profile, 'vision'),
        messages: [
          {
            role: 'system',
            content: 'You describe video frames factually for a research notebook. Only describe what is visible. No speculation about people\'s identities.',
          },
          { role: 'user', content: `Describe this frame in <=60 words. Context: ${options.context ?? 'video frame'}` },
        ],
        images: [frame.dataUrl],
        maxTokens: 160,
        ...(options.signal ? { signal: options.signal } : {}),
      },
    });
    results.push({ timeMs: frame.timeMs, description: result.text.trim() });
  }
  return results;
}
