import { useMemo, useState, type ReactNode } from 'react';
import { useAppState } from '../lib/store';
import { Modal } from './ui';
import { locatorFor } from '../lib/text';

export interface SourceViewerTarget {
  sourceId: string;
  chunkId?: string;
  citation?: { quote: string; locator: string; marker: number };
}

export function SourceViewer(props: { target: SourceViewerTarget; onClose: () => void }): ReactNode {
  const state = useAppState();
  const [highlightOnly, setHighlightOnly] = useState(false);
  const source = state.sources.find((s) => s.id === props.target.sourceId);
  const chunk = props.target.chunkId ? state.chunks.find((c) => c.id === props.target.chunkId) : undefined;
  const markers = useMemo(() => {
    if (!source) return [];
    return state.messages
      .filter((m) => m.notebookId === source.notebookId && m.citations?.some((c) => c.sourceId === source.id))
      .flatMap((m) => m.citations ?? [])
      .filter((c) => c.sourceId === source.id)
      .slice(-6)
      .reverse();
  }, [state.messages, source]);

  if (!source) return null;
  const lines = source.text.split('\n');
  const start = chunk?.startLine ?? 1;
  const end = chunk?.endLine ?? lines.length;

  return (
    <Modal
      title={source.title}
      onClose={props.onClose}
      footer={
        <>
          {source.origin.url ? (
            <a className="btn" href={source.origin.url} target="_blank" rel="noopener noreferrer">
              Open original
            </a>
          ) : null}
          <button type="button" className="btn" onClick={() => setHighlightOnly((v) => !v)}>
            {highlightOnly ? 'Show full source' : 'Show only cited passage'}
          </button>
          <button type="button" className="btn btn--primary" onClick={props.onClose}>
            Close
          </button>
        </>
      }
    >
      <div className="row small" style={{ marginBottom: '0.6rem' }}>
        <span className={`badge badge--${source.status}`}>{source.status}</span>
        <span className="badge">{source.kind}</span>
        <span className="faint">{source.statusDetail}</span>
      </div>
      <dl className="kv" style={{ marginBottom: '0.9rem' }}>
        {source.origin.url ? (
          <>
            <dt>Original URL</dt>
            <dd>
              <a href={source.origin.url} target="_blank" rel="noopener noreferrer">
                {source.origin.url}
              </a>
            </dd>
          </>
        ) : null}
        {source.origin.fileName ? (
          <>
            <dt>File</dt>
            <dd className="mono">{source.origin.fileName}</dd>
          </>
        ) : null}
        {source.origin.repo ? (
          <>
            <dt>Repository</dt>
            <dd className="mono">
              {source.origin.repo}/{source.origin.path ?? ''} @ {source.origin.ref ?? 'default'}
              {source.origin.commit ? ` (${source.origin.commit.slice(0, 10)})` : ''}
            </dd>
          </>
        ) : null}
        <dt>Size</dt>
        <dd>
          {source.text.length.toLocaleString()} characters · {source.segments.length} segment(s) · checksum {source.checksum}
        </dd>
        {chunk ? (
          <>
            <dt>Cited passage</dt>
            <dd>{locatorFor(chunk)}</dd>
          </>
        ) : null}
      </dl>

      {props.target.citation ? (
        <div className="passage">
          <div className="tiny faint" style={{ marginBottom: '0.25rem' }}>
            Quoted by citation [{props.target.citation.marker}] · {props.target.citation.locator}
          </div>
          <div>{props.target.citation.quote}</div>
        </div>
      ) : null}

      {source.warnings.length ? (
        <div className="notice notice--warning">
          <strong>Extraction notes</strong>
          <ul style={{ marginBottom: 0 }}>
            {source.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <h3 style={{ marginTop: '0.5rem' }}>{highlightOnly ? 'Cited passage' : 'Full source text'}</h3>
      {highlightOnly ? (
        <pre style={{ whiteSpace: 'pre-wrap', maxHeight: '24rem' }}>{chunk ? chunk.text : source.text.slice(0, 4000)}</pre>
      ) : (
        <div className="source-text">
          {lines.map((line, index) => {
            const lineNo = index + 1;
            const highlighted = chunk ? lineNo >= start && lineNo <= end : false;
            return (
              <div key={lineNo} style={{ display: 'flex', gap: '0.6rem' }}>
                <span className="faint mono tiny" style={{ flex: '0 0 3.2rem', textAlign: 'right', userSelect: 'none' }} aria-hidden="true">
                  {lineNo}
                </span>
                <span style={{ flex: 1, minWidth: 0, overflowWrap: 'anywhere' }}>
                  {highlighted ? <mark>{line || ' '}</mark> : line || '\u00a0'}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {markers.length ? (
        <>
          <h3 style={{ marginTop: '1.2rem' }}>Recent citations of this source</h3>
          <ul className="small muted">
            {markers.map((c) => (
              <li key={`${c.marker}-${c.chunkId}`}>
                [{c.marker}] {c.locator} — “{c.quote.slice(0, 160)}”
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </Modal>
  );
}
