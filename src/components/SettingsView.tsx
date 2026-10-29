import { useState, type ReactNode } from 'react';
import { actions, useAppState } from '../lib/store';
import {
  ADAPTERS,
  OPENAI_COMPATIBLE_PRESETS,
  effectiveCapabilities,
  getAdapter,
  providerForTask,
  resolveModel,
  testConnection,
  validateProfile,
  embedTexts,
  NO_CAPABILITIES,
} from '../lib/providers';
import { TASK_CATEGORIES, type ProviderCapabilities, type ProviderProfile, type TaskCategory } from '../lib/types';
import { exportBackupFile, importBackupFromFile } from '../lib/exports/actions';
import { ConfirmDialog, Field, Spinner } from './ui';
import { resetStorageForTests } from '../lib/storage';

function blankProfile(protocol: ProviderProfile['protocol'] = 'openai'): ProviderProfile {
  const adapter = getAdapter(protocol);
  return {
    id: `prov-${Math.random().toString(36).slice(2, 9)}`,
    label: adapter.label,
    protocol,
    baseUrl: adapter.defaultBaseUrl,
    apiKey: '',
    defaultModel: '',
    models: {},
    capabilities: {
      text: true,
      vision: false,
      embeddings: false,
      tts: false,
      transcription: false,
      streaming: true,
    },
  };
}

export function SettingsView(): ReactNode {
  const state = useAppState();
  const settings = state.settings;
  const [editing, setEditing] = useState<ProviderProfile | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [testing, setTesting] = useState(false);
  const [confirmWipe, setConfirmWipe] = useState(false);
  const [embeddingProgress, setEmbeddingProgress] = useState<string | null>(null);

  const saveProvider = async () => {
    if (!editing) return;
    const problems = validateProfile(editing);
    setErrors(problems);
    if (problems.length) return;
    await actions.updateSettings({
      providers: settings.providers.some((p) => p.id === editing.id)
        ? settings.providers.map((p) => (p.id === editing.id ? editing : p))
        : [...settings.providers, editing],
      activeProviderId: settings.activeProviderId ?? editing.id,
    });
    actions.toast('success', `Saved provider “${editing.label}”.`);
    setEditing(null);
    setTestResult(null);
  };

  const buildEmbeddings = async () => {
    const profile = providerForTask(settings, 'embedding');
    if (!profile) {
      actions.toast('warning', 'Add a provider with the embeddings capability first.');
      return;
    }
    const chunks = state.chunks;
    if (!chunks.length) {
      actions.toast('warning', 'No chunks to embed yet.');
      return;
    }
    if (chunks.length > 3000) {
      actions.toast('error', 'Too many chunks to embed (limit 3000 for a browser session).');
      return;
    }
    setEmbeddingProgress('Starting…');
    try {
      const updated = [...chunks];
      const batchSize = 24;
      for (let i = 0; i < chunks.length; i += batchSize) {
        const batch = chunks.slice(i, i + batchSize);
        setEmbeddingProgress(`Embedding ${Math.min(i + batchSize, chunks.length)} / ${chunks.length}`);
        const vectors = await embedTexts(profile, batch.map((c) => c.text.slice(0, 6000)));
        vectors.forEach((vector, j) => {
          updated[i + j] = { ...batch[j], embedding: vector };
        });
      }
      await actions.applyChunkEmbeddings(updated);
      actions.toast('success', `Embeddings stored for ${chunks.length} chunks. Hybrid retrieval is now active.`);
    } catch (err) {
      actions.toast('error', `Embedding failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setEmbeddingProgress(null);
    }
  };

  const embeddingCount = state.chunks.filter((c) => c.embedding?.length).length;

  return (
    <div className="container container--narrow stack">
      <div className="row">
        <h1 style={{ margin: 0 }}>Settings</h1>
        <div className="spacer" />
        <button type="button" className="btn btn--sm" onClick={() => actions.setView('notebook')}>
          Back to workspace
        </button>
      </div>

      <section className="card" aria-labelledby="providers-heading">
        <h2 id="providers-heading">AI providers</h2>
        <p className="small muted">
          SourceForge never ships a key and never proxies your content through a server. Requests go directly from this browser to the
          endpoint you configure. Provider billing, quotas, retention and privacy are governed by that provider. Keys are held in memory
          only, unless you explicitly enable saving them.
        </p>

        <div className="btn-row" style={{ marginBottom: '0.8rem' }}>
          <select
            className="select"
            aria-label="Add provider from preset"
            value=""
            onChange={(event) => {
              const preset = OPENAI_COMPATIBLE_PRESETS.find((p) => p.id === event.target.value);
              if (preset) {
                const profile = blankProfile('openai');
                profile.label = preset.label;
                profile.baseUrl = preset.baseUrl;
                profile.capabilities = { ...profile.capabilities, ...preset.caps };
                setEditing(profile);
              } else if (event.target.value) {
                setEditing(blankProfile(event.target.value as ProviderProfile['protocol']));
              }
              event.target.value = '';
            }}
            style={{ maxWidth: '18rem' }}
          >
            <option value="">Add provider…</option>
            <optgroup label="OpenAI-compatible presets">
              {OPENAI_COMPATIBLE_PRESETS.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.label}
                </option>
              ))}
            </optgroup>
            <optgroup label="Other protocols">
              {ADAPTERS.filter((a) => a.id !== 'openai').map((adapter) => (
                <option key={adapter.id} value={adapter.id}>
                  {adapter.label}
                </option>
              ))}
            </optgroup>
          </select>
        </div>

        {settings.providers.length === 0 ? (
          <div className="notice">
            <strong>No provider configured</strong>
            The app runs fully in offline mode: imports, search, retrieval, citations, study tools and exports all work. Add a provider only if
            you want synthesised answers, AI study material, embeddings, transcription or speech.
          </div>
        ) : (
          <div className="stack">
            {settings.providers.map((profile) => {
              const caps = effectiveCapabilities(profile);
              const active = settings.activeProviderId === profile.id;
              return (
                <div key={profile.id} className="provider-card">
                  <div className="card__head">
                    <div>
                      <div className="row">
                        <strong>{profile.label}</strong>
                        {active ? <span className="badge badge--ready">active</span> : null}
                        <span className="badge">{profile.protocol}</span>
                      </div>
                      <div className="card__meta mono">{profile.baseUrl}</div>
                      <div className="card__meta">
                        default model: {profile.defaultModel || '(none)'} ·{' '}
                        {TASK_CATEGORIES.filter((t) => profile.models[t.id]).length} task-specific route(s)
                      </div>
                    </div>
                    <div className="btn-row">
                      <button type="button" className="btn btn--sm" onClick={() => { setEditing({ ...profile }); setErrors([]); setTestResult(null); }}>
                        Edit
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm"
                        disabled={active}
                        onClick={() => void actions.updateSettings({ activeProviderId: profile.id })}
                      >
                        Use for tasks
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm btn--danger"
                        onClick={() => {
                          void actions.updateSettings({
                            providers: settings.providers.filter((p) => p.id !== profile.id),
                            activeProviderId: settings.activeProviderId === profile.id ? null : settings.activeProviderId,
                          });
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                  <div className="cap-grid" style={{ marginTop: '0.5rem' }}>
                    {(['text', 'vision', 'embeddings', 'tts', 'transcription', 'streaming'] as (keyof ProviderCapabilities)[]).map((cap) => (
                      <div key={cap} className="cap-item">
                        <span className={`badge ${caps[cap] ? 'badge--ready' : ''}`}>{caps[cap] ? '✓' : '—'}</span>
                        <span>{cap}</span>
                      </div>
                    ))}
                  </div>
                  {!caps.text ? (
                    <div className="notice notice--warning" style={{ marginTop: '0.5rem' }}>
                      This profile cannot do text tasks ({getAdapter(profile.protocol).label} does not support them or the capability is off).
                      It will not be used for chat or study generation.
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}

        {editing ? (
          <div className="provider-card" style={{ marginTop: '0.75rem', borderColor: 'var(--accent)' }}>
            <h3>{settings.providers.some((p) => p.id === editing.id) ? 'Edit provider' : 'New provider'}</h3>
            <Field label="Label" htmlFor="prov-label">
              <input id="prov-label" className="input" value={editing.label} onChange={(e) => setEditing({ ...editing, label: e.target.value })} />
            </Field>
            <div className="row">
              <div style={{ flex: 1, minWidth: '10rem' }}>
                <Field label="Protocol" htmlFor="prov-protocol" hint={getAdapter(editing.protocol).description}>
                  <select
                    id="prov-protocol"
                    className="select"
                    value={editing.protocol}
                    onChange={(e) => {
                      const protocol = e.target.value as ProviderProfile['protocol'];
                      const adapter = getAdapter(protocol);
                      setEditing({
                        ...editing,
                        protocol,
                        baseUrl: adapter.defaultBaseUrl,
                        capabilities: { ...NO_CAPABILITIES, text: true, streaming: adapter.capabilities.streaming, vision: false, embeddings: false, tts: false, transcription: false },
                      });
                    }}
                  >
                    {ADAPTERS.map((adapter) => (
                      <option key={adapter.id} value={adapter.id}>
                        {adapter.label}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
              <div style={{ flex: 2, minWidth: '14rem' }}>
                <Field label="Base URL" htmlFor="prov-url" hint="HTTPS required. HTTP is only accepted for localhost.">
                  <input id="prov-url" className="input mono" value={editing.baseUrl} onChange={(e) => setEditing({ ...editing, baseUrl: e.target.value })} />
                </Field>
              </div>
            </div>
            <Field label="API key" htmlFor="prov-key" hint={settings.persistKeys ? 'Saved in this browser because you enabled key persistence.' : 'Kept in memory for this tab only.'}>
              <input
                id="prov-key"
                className="input mono"
                type="password"
                autoComplete="off"
                value={editing.apiKey}
                onChange={(e) => setEditing({ ...editing, apiKey: e.target.value })}
                placeholder={editing.protocol === 'ollama' ? 'not needed for local Ollama' : 'sk-…'}
              />
            </Field>
            <Field label="Default model" htmlFor="prov-model">
              <input id="prov-model" className="input mono" value={editing.defaultModel} onChange={(e) => setEditing({ ...editing, defaultModel: e.target.value })} placeholder="e.g. gpt-4o-mini / llama3.1 / gemini-1.5-flash" />
            </Field>

            <fieldset style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '0.6rem', marginBottom: '0.8rem' }}>
              <legend className="small muted">Per-task model routing (optional)</legend>
              <div className="stack" style={{ gap: '0.4rem' }}>
                {TASK_CATEGORIES.map((task) => (
                  <div key={task.id} className="row">
                    <label htmlFor={`task-${task.id}`} style={{ flex: '0 0 10rem', marginBottom: 0 }} title={task.description}>
                      {task.label}
                    </label>
                    <input
                      id={`task-${task.id}`}
                      className="input mono"
                      style={{ flex: 1 }}
                      value={editing.models[task.id] ?? ''}
                      placeholder={`default (${editing.defaultModel || 'unset'})`}
                      onChange={(e) =>
                        setEditing({
                          ...editing,
                          models: { ...editing.models, [task.id]: e.target.value || undefined } as Partial<Record<TaskCategory, string>>,
                        })
                      }
                    />
                    <span className={`badge ${effectiveCapabilities(editing)[capabilityKey(task.id)] ? 'badge--ready' : ''}`}>
                      {capabilityKey(task.id)}
                    </span>
                  </div>
                ))}
              </div>
            </fieldset>

            <fieldset style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '0.6rem', marginBottom: '0.8rem' }}>
              <legend className="small muted">Capabilities of the selected model</legend>
              <p className="tiny faint">Only enable what the model actually supports. A text-only model must stay text-only.</p>
              <div className="cap-grid">
                {(['text', 'vision', 'embeddings', 'tts', 'transcription', 'streaming'] as (keyof ProviderCapabilities)[]).map((cap) => {
                  const supported = getAdapter(editing.protocol).capabilities[cap];
                  return (
                    <label key={cap} className="checkbox" style={{ opacity: supported ? 1 : 0.45 }}>
                      <input
                        type="checkbox"
                        checked={editing.capabilities[cap] && supported}
                        disabled={!supported}
                        onChange={(e) =>
                          setEditing({ ...editing, capabilities: { ...editing.capabilities, [cap]: e.target.checked } })
                        }
                      />
                      <span>{cap}{supported ? '' : ' (protocol unsupported)'}</span>
                    </label>
                  );
                })}
              </div>
            </fieldset>

            {errors.length ? (
              <div className="notice notice--error" role="alert">
                <strong>Fix these before saving</strong>
                <ul style={{ marginBottom: 0 }}>
                  {errors.map((error) => (
                    <li key={error}>{error}</li>
                  ))}
                </ul>
              </div>
            ) : null}

            {testResult ? (
              <div className={`notice ${testResult.ok ? 'notice--success' : 'notice--error'}`}>
                <strong>{testResult.ok ? 'Connection OK' : 'Connection failed'}</strong>
                {testResult.message}
              </div>
            ) : null}

            <div className="btn-row">
              <button type="button" className="btn btn--primary" onClick={() => void saveProvider()}>
                Save provider
              </button>
              <button
                type="button"
                className="btn"
                disabled={testing}
                onClick={async () => {
                  setTesting(true);
                  setTestResult(null);
                  const result = await testConnection(editing);
                  setTestResult(result);
                  setTesting(false);
                }}
              >
                {testing ? 'Testing…' : 'Test connection'}
              </button>
              <button type="button" className="btn btn--ghost" onClick={() => { setEditing(null); setErrors([]); setTestResult(null); }}>
                Cancel
              </button>
              <span className="tiny faint" style={{ alignSelf: 'center' }}>
                Sends one small “ok” prompt to {editing.baseUrl || 'the endpoint'}.
              </span>
            </div>
          </div>
        ) : null}

        <label className="checkbox" style={{ marginTop: '0.8rem' }}>
          <input
            type="checkbox"
            checked={settings.persistKeys}
            onChange={(event) => void actions.updateSettings({ persistKeys: event.target.checked })}
          />
          <span>
            <strong>Save API keys and GitHub token in this browser</strong>
            <span className="hint" style={{ marginTop: 0 }}>
              Off by default. When enabled, keys are stored in IndexedDB on this device only and are never included in exports. Anyone with
              access to this browser profile could read them.
            </span>
          </span>
        </label>
      </section>

      <section className="card" aria-labelledby="ingest-heading">
        <h2 id="ingest-heading">Ingestion</h2>
        <Field
          label="Reader endpoint (optional)"
          htmlFor="reader-endpoint"
          hint="Used for web pages, YouTube and Instagram where direct browser requests are blocked by CORS. The reader service sees the URL you import."
        >
          <input
            id="reader-endpoint"
            className="input mono"
            value={settings.readerEndpoint}
            onChange={(event) => void actions.updateSettings({ readerEndpoint: event.target.value })}
            placeholder="https://r.jina.ai/"
          />
        </Field>
        <Field
          label="GitHub token (optional)"
          htmlFor="github-token"
          hint="Raises the GitHub API limit from ~60 to ~5000 requests/hour and enables private repos you have access to. It is never sent anywhere except api.github.com."
        >
          <input
            id="github-token"
            className="input mono"
            type="password"
            autoComplete="off"
            value={settings.githubToken}
            onChange={(event) => void actions.updateSettings({ githubToken: event.target.value })}
            placeholder="ghp_…"
          />
        </Field>
      </section>

      <section className="card" aria-labelledby="retrieval-heading">
        <h2 id="retrieval-heading">Retrieval</h2>
        <div className="row">
          <div style={{ flex: 1, minWidth: '9rem' }}>
            <Field label="Chunk size (characters)" htmlFor="chunk-size" hint="Changing this re-indexes every notebook.">
              <input
                id="chunk-size"
                className="input"
                type="number"
                min={200}
                max={4000}
                value={settings.chunkSize}
                onChange={(event) => void actions.updateSettings({ chunkSize: Math.max(200, Math.min(4000, Number(event.target.value) || 900)) })}
              />
            </Field>
          </div>
          <div style={{ flex: 1, minWidth: '9rem' }}>
            <Field label="Chunk overlap (characters)" htmlFor="chunk-overlap">
              <input
                id="chunk-overlap"
                className="input"
                type="number"
                min={0}
                max={1000}
                value={settings.chunkOverlap}
                onChange={(event) => void actions.updateSettings({ chunkOverlap: Math.max(0, Math.min(1000, Number(event.target.value) || 0)) })}
              />
            </Field>
          </div>
          <div style={{ flex: 1, minWidth: '9rem' }}>
            <Field label="Top K passages" htmlFor="top-k">
              <input
                id="top-k"
                className="input"
                type="number"
                min={1}
                max={20}
                value={settings.topK}
                onChange={(event) => void actions.updateSettings({ topK: Math.max(1, Math.min(20, Number(event.target.value) || 8)) })}
              />
            </Field>
          </div>
        </div>
        <Field
          label={`Insufficient-evidence threshold: ${(settings.insufficientEvidenceThreshold * 100).toFixed(0)}%`}
          htmlFor="threshold"
          hint="Higher values make the assistant refuse more readily instead of answering from weak matches."
        >
          <input
            id="threshold"
            type="range"
            min={5}
            max={60}
            value={Math.round(settings.insufficientEvidenceThreshold * 100)}
            onChange={(event) => void actions.updateSettings({ insufficientEvidenceThreshold: Number(event.target.value) / 100 })}
            style={{ width: '100%' }}
          />
        </Field>
        <div className="row">
          <div>
            <div className="small">
              Embeddings: <strong>{embeddingCount}</strong> of {state.chunks.length} chunks
            </div>
            <div className="hint">Optional semantic retrieval. Without embeddings, BM25 lexical ranking is used (fully offline).</div>
          </div>
          <div className="spacer" />
          <button type="button" className="btn btn--sm" onClick={() => void buildEmbeddings()} disabled={Boolean(embeddingProgress) || !providerForTask(settings, 'embedding')}>
            {embeddingProgress ?? 'Compute embeddings'}
          </button>
        </div>
      </section>

      <section className="card" aria-labelledby="data-heading">
        <h2 id="data-heading">Data & backup</h2>
        <dl className="kv">
          <dt>Storage backend</dt>
          <dd>{state.storage.message}</dd>
          <dt>Notebooks</dt>
          <dd>{state.notebooks.length}</dd>
          <dt>Sources / chunks</dt>
          <dd>{state.sources.length} / {state.chunks.length}</dd>
          <dt>Chat messages</dt>
          <dd>{state.messages.length}</dd>
          <dt>Study items</dt>
          <dd>{state.artifacts.length}</dd>
          <dt>Core engine</dt>
          <dd>{state.engine === 'rust-wasm' ? 'Rust core via WebAssembly' : 'TypeScript fallback (WASM build not present)'}</dd>
        </dl>
        <div className="btn-row" style={{ marginTop: '0.7rem' }}>
          <button type="button" className="btn btn--primary btn--sm" onClick={() => void exportBackupFile()}>
            Export JSON backup
          </button>
          <label className="btn btn--sm" style={{ cursor: 'pointer' }}>
            Import backup
            <input
              type="file"
              accept="application/json,.json"
              style={{ display: 'none' }}
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void importBackupFromFile(file);
                event.target.value = '';
              }}
            />
          </label>
          <button type="button" className="btn btn--sm" onClick={() => void actions.updateSettings({ theme: settings.theme === 'dark' ? 'light' : 'dark' })}>
            {settings.theme === 'dark' ? 'Light theme' : 'Dark theme'}
          </button>
          <button type="button" className="btn btn--sm btn--danger" onClick={() => setConfirmWipe(true)}>
            Delete all local data
          </button>
        </div>
        <p className="tiny faint" style={{ marginTop: '0.6rem' }}>
          Backups contain notebooks, sources, chat history and study material — never API keys or tokens. Restores are validated and remapped to
          avoid id collisions.
        </p>
      </section>

      <section className="card" aria-labelledby="privacy-heading">
        <h2 id="privacy-heading">What leaves your browser</h2>
        <ul className="small muted">
          <li>
            <strong>Nothing</strong> for imports of local files, pasted text, retrieval, study generation in offline mode, exports and storage.
          </li>
          <li>
            <strong>Your question and the retrieved excerpts</strong> are sent to the configured provider when you ask for an AI answer or
            generate AI study material.
          </li>
          <li>
            <strong>Audio or sampled frames</strong> are sent to your configured provider only when you run transcription or frame analysis.
          </li>
          <li>
            <strong>URLs you import</strong> are requested directly from that site, or through the reader endpoint you configured if direct
            requests are blocked.
          </li>
          <li>Source content is treated as untrusted data; instructions inside documents are neutralised before prompting.</li>
        </ul>
        <p className="tiny faint">
          Providers have their own billing, quota, logging and retention policies. This app cannot make a third-party service free or unlimited.
        </p>
      </section>

      {confirmWipe ? (
        <ConfirmDialog
          title="Delete everything?"
          message="All notebooks, sources, chat history and study material stored in this browser will be permanently deleted. Export a backup first if you are unsure."
          confirmLabel="Delete all data"
          danger
          onCancel={() => setConfirmWipe(false)}
          onConfirm={() => {
            void actions.wipeAll();
            resetStorageForTests();
            setConfirmWipe(false);
          }}
        />
      ) : null}

      {embeddingProgress ? <div className="notice"><Spinner label={embeddingProgress} /></div> : null}

      <p className="tiny faint" style={{ paddingBottom: '2rem' }}>
        SourceForge v0.1.0 · local-first research workspace · model routing: {settings.providers.length ? `${settings.providers.length} provider profile(s); active model for chat: ${(() => {
          const p = providerForTask(settings, 'chat');
          return p ? resolveModel(p, 'chat') || '(unset)' : 'offline';
        })()}` : 'offline only'}
      </p>
    </div>
  );
}

function capabilityKey(task: TaskCategory): keyof ProviderCapabilities {
  if (task === 'vision') return 'vision';
  if (task === 'embedding') return 'embeddings';
  if (task === 'tts') return 'tts';
  if (task === 'transcription') return 'transcription';
  return 'text';
}
