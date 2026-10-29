import { useMemo, useState, type ReactNode } from 'react';
import { actions, useAppState } from '../lib/store';
import { Modal, Spinner } from './ui';
import {
  IngestionError,
  detectImportRoute,
  importFile,
  importInstagram,
  importPastedText,
  importPastedTranscript,
  importTranscriptFile,
  importWebUrl,
  importYouTube,
  importGithubFiles,
  listGithubTarget,
} from '../lib/ingest';
import type { GithubEntry, GithubTarget } from '../lib/ingest/github';
import { parseYouTubeUrl } from '../lib/ingest/youtube';
import { parseInstagramUrl } from '../lib/ingest/instagram';
import { importBackupFromFile } from '../lib/exports/actions';
import { MediaPanel } from './MediaPanel';

type Tab = 'files' | 'paste' | 'web' | 'github' | 'media';

const TABS: { id: Tab; label: string }[] = [
  { id: 'files', label: 'Files' },
  { id: 'paste', label: 'Paste' },
  { id: 'web', label: 'URL / YouTube / Instagram' },
  { id: 'github', label: 'GitHub' },
  { id: 'media', label: 'Media' },
];

export function AddSourceDialog(props: { notebookId: string; onClose: () => void }): ReactNode {
  const state = useAppState();
  const [tab, setTab] = useState<Tab>('files');
  const [error, setError] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);

  // paste
  const [pasteTitle, setPasteTitle] = useState('');
  const [pasteText, setPasteText] = useState('');
  const [pasteUrl, setPasteUrl] = useState('');

  // web / youtube / instagram
  const [url, setUrl] = useState('');
  const [captionFile, setCaptionFile] = useState<File | null>(null);
  const [transcriptPaste, setTranscriptPaste] = useState('');

  // github
  const [ghListing, setGhListing] = useState<{
    target: GithubTarget;
    ref: string;
    entries: GithubEntry[];
    truncated: boolean;
    selected: Set<string>;
  } | null>(null);

  const settings = state.settings;
  const route = useMemo(() => (url.trim() ? detectImportRoute(url) : 'invalid'), [url]);

  const finish = (count: number, label: string) => {
    actions.toast('success', `${label} (${count} source${count === 1 ? '' : 's'} added).`);
    props.onClose();
  };

  const handleError = (err: unknown) => {
    const message = err instanceof IngestionError ? err.message : err instanceof Error ? err.message : String(err);
    setError(message);
    setHint(err instanceof IngestionError && err.hint ? err.hint : null);
    actions.toast('error', 'Import failed', message);
  };

  const importFiles = async (files: FileList | null) => {
    if (!files?.length) return;
    setError(null);
    setHint(null);
    const list = [...files];
    const sources = [];
    const failures: string[] = [];
    setProgress({ done: 0, total: list.length });
    for (const [index, file] of list.entries()) {
      setBusy(`Parsing ${file.name}…`);
      try {
        if (/\.json$/i.test(file.name) && file.size > 0) {
          const text = await file.text();
          if (/"format"\s*:\s*"sourceforge\.backup"/.test(text)) {
            await importBackupFromFile(file);
            continue;
          }
        }
        sources.push(await importFile(file, props.notebookId));
      } catch (err) {
        failures.push(`${file.name}: ${err instanceof Error ? err.message : String(err)}`);
      }
      setProgress({ done: index + 1, total: list.length });
    }
    setBusy(null);
    setProgress(null);
    if (sources.length) await actions.addSources(sources);
    if (failures.length) {
      setError(failures.join('\n'));
      setHint('Supported: PDF, DOCX, PPTX, TXT, MD, CSV/TSV, JSON, HTML, VTT/SRT, and source code up to 25 MB.');
    }
    if (sources.length && !failures.length) finish(sources.length, 'Imported');
    else if (sources.length) actions.toast('warning', `Imported ${sources.length} file(s); ${failures.length} failed.`);
  };

  const submitPaste = async () => {
    setError(null);
    try {
      if (!pasteText.trim()) throw new IngestionError('empty', 'Paste some text first.');
      const source = pasteUrl.trim()
        ? importPastedTranscript(props.notebookId, pasteTitle.trim() || 'Pasted transcript', pasteText, pasteUrl.trim())
        : importPastedText(props.notebookId, pasteTitle.trim() || 'Pasted note', pasteText, { kind: 'text' });
      await actions.addSources([source]);
      finish(1, 'Pasted source added');
    } catch (err) {
      handleError(err);
    }
  };

  const submitUrl = async () => {
    setError(null);
    setHint(null);
    setBusy('Fetching…');
    try {
      if (!url.trim()) throw new IngestionError('invalid-input', 'Enter a URL first.');
      if (route === 'github') {
        setTab('github');
        await loadGithub();
        return;
      }
      if (route === 'youtube') {
        const target = parseYouTubeUrl(url);
        if (!target.videoId) {
          throw new IngestionError(
            'invalid-input',
            'Playlist URLs cannot be imported as a single source.',
            'Open a specific video and import it, or paste the playlist and pick videos individually.',
          );
        }
        if (captionFile) {
          const text = await captionFile.text();
          const source = importTranscriptFile(props.notebookId, captionFile.name, text, { videoUrl: url });
          await actions.addSources([source]);
          finish(1, 'Caption file imported with video provenance');
          return;
        }
        if (transcriptPaste.trim()) {
          const source = importPastedTranscript(props.notebookId, `YouTube transcript ${target.videoId}`, transcriptPaste, url);
          await actions.addSources([source]);
          finish(1, 'Pasted transcript imported');
          return;
        }
        const source = await importYouTube(url, props.notebookId, settings);
        await actions.addSources([source]);
        actions.toast(
          source.status === 'ready' ? 'success' : 'warning',
          source.status === 'ready' ? 'YouTube source imported with transcript timestamps.' : 'YouTube metadata imported without a transcript.',
          source.status === 'ready' ? undefined : 'Upload a .vtt/.srt caption file, paste the transcript, or transcribe a downloaded file.',
        );
        props.onClose();
        return;
      }
      if (route === 'instagram') {
        parseInstagramUrl(url);
        if (transcriptPaste.trim()) {
          const source = importPastedText(props.notebookId, `Instagram caption`, transcriptPaste, {
            kind: 'instagram',
            origin: { url },
          });
          await actions.addSources([source]);
          finish(1, 'Pasted Instagram content added');
          return;
        }
        const source = await importInstagram(url, props.notebookId, settings);
        await actions.addSources([source]);
        actions.toast('warning', 'Instagram content imported with limits.', 'Only public data exposed without login is available; captions are often missing.');
        props.onClose();
        return;
      }
      if (route === 'web') {
        const source = await importWebUrl(url, props.notebookId, settings);
        await actions.addSources([source]);
        finish(1, 'Web page imported');
        return;
      }
      throw new IngestionError('invalid-input', 'Only http(s) URLs can be imported.');
    } catch (err) {
      handleError(err);
    } finally {
      setBusy(null);
    }
  };

  const loadGithub = async () => {
    setError(null);
    setHint(null);
    setBusy('Loading repository listing…');
    try {
      const listing = await listGithubTarget(url, settings);
      const importable = listing.entries.filter((e) => e.importable);
      if (!importable.length) {
        throw new IngestionError(
          'invalid-input',
          'No importable text files were found at that path.',
          'GitHub import supports text-like files under 512 KB. Try a directory with Markdown or source files.',
        );
      }
      setGhListing({
        target: listing.target,
        ref: listing.ref,
        entries: listing.entries,
        truncated: listing.truncated,
        selected: new Set(listing.suggested.map((e) => e.path)),
      });
    } catch (err) {
      handleError(err);
    } finally {
      setBusy(null);
    }
  };

  const importGithubSelection = async () => {
    if (!ghListing) return;
    setBusy('Downloading selected files…');
    setError(null);
    try {
      const selected = ghListing.entries.filter((e) => ghListing.selected.has(e.path));
      const result = await importGithubFiles(ghListing.target, selected, ghListing.ref, props.notebookId, settings);
      await actions.addSources(result.sources);
      if (result.warnings.length) {
        actions.toast('warning', `Imported ${result.sources.length} file(s) with ${result.warnings.length} warning(s).`, result.warnings[0]);
      }
      finish(result.sources.length, 'GitHub files imported');
    } catch (err) {
      handleError(err);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Modal
      title="Add sources"
      onClose={props.onClose}
      footer={
        <>
          <span className="tiny faint" style={{ flex: 1 }}>
            Parsing happens locally. Network imports only contact the hosts you specify.
          </span>
          <button type="button" className="btn" onClick={props.onClose}>
            Close
          </button>
        </>
      }
    >
      <div className="tabs" role="tablist" aria-label="Source type">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            id={`tab-${item.id}`}
            aria-selected={tab === item.id}
            aria-controls={`panel-${item.id}`}
            className="tab"
            onClick={() => {
              setTab(item.id);
              setError(null);
              setHint(null);
            }}
          >
            {item.label}
          </button>
        ))}
      </div>

      {error ? (
        <div className="notice notice--error" role="alert">
          <strong>Could not import</strong>
          <span style={{ whiteSpace: 'pre-wrap' }}>{error}</span>
          {hint ? <div className="hint">{hint}</div> : null}
        </div>
      ) : null}

      {busy ? (
        <div style={{ marginBottom: '0.75rem' }}>
          <Spinner label={busy} />
          {progress ? (
            <div className="progress">
              <div className="progress__bar" style={{ width: `${Math.round((progress.done / Math.max(1, progress.total)) * 100)}%` }} />
            </div>
          ) : null}
        </div>
      ) : null}

      {tab === 'files' ? (
        <section id="panel-files" role="tabpanel" aria-labelledby="tab-files">
          <div className="field">
            <label htmlFor="file-input">Choose files</label>
            <input
              id="file-input"
              className="input"
              type="file"
              multiple
              accept=".pdf,.docx,.pptx,.txt,.md,.markdown,.csv,.tsv,.json,.html,.htm,.vtt,.srt,.json3,.xml,.yaml,.yml,.js,.jsx,.ts,.tsx,.py,.rs,.go,.java,.c,.cpp,.h,.css,.sql,.sh,.rb,.php,.swift,.kt,.toml,.ini,.log,.tex,.rst"
              onChange={(event) => void importFiles(event.target.files)}
            />
            <div className="hint">
              PDF (with page references), DOCX, PPTX (slide references), Markdown, plain text, CSV, JSON, HTML, captions and source code.
              Up to 25 MB per file. Scanned/image-only PDFs have no text layer and are rejected with an explanation.
            </div>
          </div>
          <div className="notice">
            <strong>Restoring a backup?</strong>
            A SourceForge backup is a <span className="mono">.json</span> file. Choose it above — it is detected automatically — or use Settings →
            Data.
          </div>
        </section>
      ) : null}

      {tab === 'paste' ? (
        <section id="panel-paste" role="tabpanel" aria-labelledby="tab-paste">
          <div className="field">
            <label htmlFor="paste-title">Title</label>
            <input id="paste-title" className="input" value={pasteTitle} onChange={(e) => setPasteTitle(e.target.value)} placeholder="e.g. Interview transcript — 12 March" />
          </div>
          <div className="field">
            <label htmlFor="paste-url">Original URL (optional — used for provenance)</label>
            <input id="paste-url" className="input" value={pasteUrl} onChange={(e) => setPasteUrl(e.target.value)} placeholder="https://…" />
          </div>
          <div className="field">
            <label htmlFor="paste-text">Content</label>
            <textarea id="paste-text" className="textarea" value={pasteText} onChange={(e) => setPasteText(e.target.value)} placeholder="Paste notes, a transcript, a table, an excerpt or code…" />
            <div className="hint">
              {pasteText.length.toLocaleString()} characters. If the text contains timestamps (VTT/SRT style), they are preserved for timestamp citations.
            </div>
          </div>
          <button type="button" className="btn btn--primary" onClick={() => void submitPaste()} disabled={Boolean(busy)}>
            Add pasted source
          </button>
        </section>
      ) : null}

      {tab === 'web' ? (
        <section id="panel-web" role="tabpanel" aria-labelledby="tab-web">
          <div className="field">
            <label htmlFor="url-input">URL</label>
            <input
              id="url-input"
              className="input"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/article · https://youtube.com/watch?v=… · https://instagram.com/reel/…"
            />
            <div className="hint">
              Detected: <strong>{route === 'invalid' ? 'unknown' : route}</strong>. GitHub links switch to the GitHub tab automatically.
            </div>
          </div>

          {route === 'youtube' ? (
            <div className="notice">
              <strong>YouTube needs a transcript path</strong>
              Browsers cannot fetch YouTube pages or media directly (CORS and terms of service). Options, in order of reliability:
              <ol style={{ margin: '0.4rem 0 0' }}>
                <li>Upload the caption file (.vtt/.srt) you downloaded from YouTube.</li>
                <li>Paste the transcript below.</li>
                <li>Let SourceForge try the reader service to fetch public metadata and any available transcript.</li>
              </ol>
              <div className="field" style={{ marginTop: '0.6rem' }}>
                <label htmlFor="yt-caption">Caption file (optional)</label>
                <input id="yt-caption" className="input" type="file" accept=".vtt,.srt,.json3,.txt" onChange={(e) => setCaptionFile(e.target.files?.[0] ?? null)} />
              </div>
              <div className="field">
                <label htmlFor="yt-transcript">Paste transcript (optional)</label>
                <textarea id="yt-transcript" className="textarea" value={transcriptPaste} onChange={(e) => setTranscriptPaste(e.target.value)} placeholder={'0:00 intro text\n0:14 next line…'} />
              </div>
            </div>
          ) : null}

          {route === 'instagram' ? (
            <div className="notice notice--warning">
              <strong>Instagram limits</strong>
              Only public data exposed without login can be read, through the configured reader service. SourceForge does not bypass
              authentication, anti-bot systems or DRM. If the caption is missing, paste it below.
              <div className="field" style={{ marginTop: '0.6rem' }}>
                <label htmlFor="ig-caption">Paste caption or transcript (optional)</label>
                <textarea id="ig-caption" className="textarea" value={transcriptPaste} onChange={(e) => setTranscriptPaste(e.target.value)} />
              </div>
            </div>
          ) : null}

          {route === 'web' ? (
            <div className="notice">
              <strong>Web pages and CORS</strong>
              A browser can only read pages that allow cross-origin requests. SourceForge first uses the reader endpoint from Settings
              ({settings.readerEndpoint || 'none configured'}), then tries a direct request. If both fail, save the page and upload the HTML, or
              paste the text.
            </div>
          ) : null}

          <button type="button" className="btn btn--primary" onClick={() => void submitUrl()} disabled={Boolean(busy) || !url.trim()}>
            Import from URL
          </button>
        </section>
      ) : null}

      {tab === 'github' ? (
        <section id="panel-github" role="tabpanel" aria-labelledby="tab-github">
          <div className="field">
            <label htmlFor="gh-url">Repository, directory or file URL</label>
            <div className="row">
              <input
                id="gh-url"
                className="input"
                style={{ flex: 1 }}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://github.com/owner/repo or /tree/main/docs or /blob/main/README.md"
              />
              <button type="button" className="btn" onClick={() => void loadGithub()} disabled={Boolean(busy) || !url.trim()}>
                Load files
              </button>
            </div>
            <div className="hint">
              Public repositories work without a token. Unauthenticated GitHub API limits are ~60 requests/hour; with a token (Settings) it is
              ~5000/hour. Contents are then downloaded from raw.githubusercontent.com, which is not API-rate-limited.
              {!settings.githubToken ? ' No token configured.' : ' Token configured.'}
            </div>
          </div>

          {ghListing ? (
            <>
              <div className="row" style={{ marginBottom: '0.5rem' }}>
                <div className="small muted">
                  {ghListing.target.owner}/{ghListing.target.repo} @ {ghListing.ref} · {ghListing.entries.filter((e) => e.importable).length} importable file(s)
                  {ghListing.truncated ? ' · listing truncated' : ''}
                </div>
                <div className="spacer" />
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() =>
                    setGhListing({
                      ...ghListing,
                      selected: new Set(ghListing.entries.filter((e) => e.importable).map((e) => e.path)),
                    })
                  }
                >
                  Select all
                </button>
                <button type="button" className="btn btn--sm" onClick={() => setGhListing({ ...ghListing, selected: new Set() })}>
                  Clear
                </button>
              </div>
              <div style={{ maxHeight: '20rem', overflow: 'auto', border: '1px solid var(--border)', borderRadius: 8, padding: '0.5rem' }}>
                {ghListing.entries.map((entry) => (
                  <label key={entry.path} className="checkbox" style={{ padding: '0.15rem 0', opacity: entry.importable ? 1 : 0.5 }}>
                    <input
                      type="checkbox"
                      disabled={!entry.importable}
                      checked={ghListing.selected.has(entry.path)}
                      onChange={(event) => {
                        const next = new Set(ghListing.selected);
                        if (event.target.checked) next.add(entry.path);
                        else next.delete(entry.path);
                        setGhListing({ ...ghListing, selected: next });
                      }}
                    />
                    <span style={{ flex: 1, minWidth: 0 }}>
                      <span className="mono small" style={{ overflowWrap: 'anywhere' }}>{entry.path}</span>
                      <span className="tiny faint">
                        {' '}
                        · {(entry.size / 1024).toFixed(0)} KB{entry.reason ? ` · ${entry.reason}` : ''}
                      </span>
                    </span>
                  </label>
                ))}
              </div>
              <div className="btn-row" style={{ marginTop: '0.6rem' }}>
                <button type="button" className="btn btn--primary" onClick={() => void importGithubSelection()} disabled={Boolean(busy) || ghListing.selected.size === 0}>
                  Import {ghListing.selected.size} file(s)
                </button>
                <span className="tiny faint">Max 40 files / 6 MB per import.</span>
              </div>
            </>
          ) : null}
        </section>
      ) : null}

      {tab === 'media' ? (
        <section id="panel-media" role="tabpanel" aria-labelledby="tab-media">
          <MediaPanel notebookId={props.notebookId} onAdded={props.onClose} />
        </section>
      ) : null}
    </Modal>
  );
}
