/**
 * SourceForge data model.
 *
 * Everything persisted in IndexedDB is versioned so that backups can be
 * validated and migrated. Keep this file dependency-free: it is imported by
 * the UI, the storage layer, the exporters and the tests.
 */

export const APP_NAME = 'SourceForge';
export const SCHEMA_VERSION = 2;
export const BACKUP_FORMAT = 'sourceforge.backup';
export const BACKUP_VERSION = 2;

export type SourceKind =
  | 'text'
  | 'markdown'
  | 'csv'
  | 'json'
  | 'html'
  | 'code'
  | 'pdf'
  | 'docx'
  | 'pptx'
  | 'github-file'
  | 'github-repo'
  | 'web'
  | 'youtube'
  | 'instagram'
  | 'media-transcript'
  | 'image-analysis';

/** A citation-addressable region of a source (page, line range, or time range). */
export interface Segment {
  index: number;
  text: string;
  startLine: number;
  endLine: number;
  page?: number;
  startMs?: number;
  endMs?: number;
  section?: string;
}

export type ExtractionStatus = 'ready' | 'partial' | 'error';

export interface SourceOrigin {
  url?: string;
  fileName?: string;
  mime?: string;
  repo?: string;
  path?: string;
  ref?: string;
  commit?: string;
  /** For GitHub imports: the API/html URL the file was read from. */
  htmlUrl?: string;
  /** Author / channel / account when known. */
  author?: string;
  /** Video id for YouTube, shortcode for Instagram. */
  externalId?: string;
}

export interface Source {
  id: string;
  notebookId: string;
  title: string;
  kind: SourceKind;
  origin: SourceOrigin;
  text: string;
  segments: Segment[];
  status: ExtractionStatus;
  /** Human readable explanation of what was and was not extracted. */
  statusDetail: string;
  warnings: string[];
  byteSize: number;
  checksum: string;
  createdAt: number;
  updatedAt: number;
}

export interface Chunk {
  id: string;
  notebookId: string;
  sourceId: string;
  ordinal: number;
  text: string;
  startLine: number;
  endLine: number;
  page?: number;
  startMs?: number;
  endMs?: number;
  section?: string;
  /** Number of tokens approximated by the chunker. */
  tokens: number;
  /** Optional embedding vector, stored only when a provider produced one. */
  embedding?: number[];
}

export interface Notebook {
  id: string;
  title: string;
  description: string;
  emoji: string;
  createdAt: number;
  updatedAt: number;
}

export type ChatRole = 'user' | 'assistant' | 'system';

export interface Citation {
  /** 1-based marker as it appears in the answer text, e.g. [2]. */
  marker: number;
  chunkId: string;
  sourceId: string;
  sourceTitle: string;
  quote: string;
  locator: string;
  score: number;
}

export interface ChatMessage {
  id: string;
  notebookId: string;
  role: ChatRole;
  content: string;
  createdAt: number;
  citations?: Citation[];
  insufficientEvidence?: boolean;
  conflicting?: boolean;
  model?: string;
  providerId?: string;
  error?: string;
  /** Set when the message was produced without an AI provider. */
  mode?: 'ai' | 'local';
}

export type ArtifactKind =
  | 'study-guide'
  | 'flashcards'
  | 'quiz'
  | 'faq'
  | 'timeline'
  | 'comparison'
  | 'glossary'
  | 'quotes'
  | 'overview'
  | 'audio-script'
  | 'storyboard'
  | 'notes'
  | 'concept-map';

export interface Artifact {
  id: string;
  notebookId: string;
  kind: ArtifactKind;
  title: string;
  /** Markdown body (always populated, even for structured kinds). */
  body: string;
  /** Structured payload for interactive rendering/export. */
  data?: unknown;
  citations: Citation[];
  mode: 'ai' | 'local';
  model?: string;
  createdAt: number;
  updatedAt: number;
}

export interface ProviderProfile {
  id: string;
  label: string;
  /** Adapter protocol. */
  protocol: 'openai' | 'anthropic' | 'gemini' | 'ollama';
  baseUrl: string;
  apiKey: string;
  /** Per-task model routing. Missing entries fall back to `defaultModel`. */
  defaultModel: string;
  models: Partial<Record<TaskCategory, string>>;
  capabilities: ProviderCapabilities;
  /** Extra query params / headers, kept explicit and inspectable. */
  headers?: Record<string, string>;
  /** Only persisted when the user explicitly opts in. */
  persist?: boolean;
}

export type TaskCategory =
  | 'chat'
  | 'summary'
  | 'study'
  | 'audioScript'
  | 'vision'
  | 'embedding'
  | 'tts'
  | 'transcription';

export interface ProviderCapabilities {
  text: boolean;
  vision: boolean;
  embeddings: boolean;
  tts: boolean;
  transcription: boolean;
  streaming: boolean;
}

export interface Settings {
  providers: ProviderProfile[];
  activeProviderId: string | null;
  /** Reader proxy used for CORS-blocked pages (e.g. a self-hosted reader). */
  readerEndpoint: string;
  githubToken: string;
  persistKeys: boolean;
  /** Retrieval knobs. */
  chunkSize: number;
  chunkOverlap: number;
  topK: number;
  contextBudget: number;
  insufficientEvidenceThreshold: number;
  embeddingEnabled: boolean;
  theme: 'dark' | 'light';
}

export interface BackupNotebook {
  notebook: Notebook;
  sources: Source[];
  artifacts: Artifact[];
  messages: ChatMessage[];
}

export interface BackupFile {
  format: typeof BACKUP_FORMAT;
  version: number;
  schemaVersion: number;
  exportedAt: string;
  app: string;
  notebooks: BackupNotebook[];
  /** Optional per-chunk cache is deliberately excluded: it is rebuilt. */
  meta?: { sourceCount?: number; notebookCount?: number };
}

export interface RetrievalHit {
  chunk: Chunk;
  source: Source;
  score: number;
  lexicalScore: number;
  vectorScore?: number;
  locator: string;
}

export interface Evidence {
  marker: number;
  chunkId: string;
  sourceId: string;
  sourceTitle: string;
  quote: string;
  locator: string;
  score: number;
}

export interface RetrievalResult {
  query: string;
  hits: RetrievalHit[];
  evidence: Evidence[];
  insufficient: boolean;
  reason: string;
  usedSources: string[];
  conflicts: Conflict[];
  diagnostics: {
    candidateChunks: number;
    lexicalCandidates: number;
    vectorCandidates: number;
    deduped: number;
    budgetUsed: number;
    budgetLimit: number;
  };
}

export interface Conflict {
  sourceA: string;
  sourceB: string;
  note: string;
  numbersA: number[];
  numbersB: number[];
}

export interface WebSearchDiagnostics {
  totalChunks: number;
  matched: number;
}

export const DEFAULT_SETTINGS: Settings = {
  providers: [],
  activeProviderId: null,
  readerEndpoint: 'https://r.jina.ai/',
  githubToken: '',
  persistKeys: false,
  chunkSize: 900,
  chunkOverlap: 150,
  topK: 8,
  contextBudget: 6000,
  insufficientEvidenceThreshold: 0.18,
  embeddingEnabled: false,
  theme: 'dark',
};

export const TASK_CATEGORIES: { id: TaskCategory; label: string; description: string }[] = [
  { id: 'chat', label: 'Research chat', description: 'Grounded answers over retrieved excerpts' },
  { id: 'summary', label: 'Summaries', description: 'Source and notebook summaries' },
  { id: 'study', label: 'Study material', description: 'Flashcards, quizzes, guides, glossaries' },
  { id: 'audioScript', label: 'Audio script', description: 'Two-host discussion script' },
  { id: 'vision', label: 'Vision', description: 'Video frame / image analysis' },
  { id: 'embedding', label: 'Embeddings', description: 'Optional semantic retrieval' },
  { id: 'tts', label: 'Speech synthesis', description: 'Audio overview narration' },
  { id: 'transcription', label: 'Transcription', description: 'Speech-to-text for media' },
];
