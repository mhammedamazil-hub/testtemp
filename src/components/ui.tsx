import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { renderMarkdown } from '../lib/markdown';
import { actions, useAppState, type Toast } from '../lib/store';

export function Toasts(): ReactNode {
  const { toasts } = useAppState();
  if (!toasts.length) return null;
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast: Toast) => (
        <div key={toast.id} className={`toast toast--${toast.kind}`}>
          <div className="row" style={{ alignItems: 'flex-start' }}>
            <div style={{ flex: 1 }}>
              <div>{toast.message}</div>
              {toast.detail ? <div className="toast__detail">{toast.detail}</div> : null}
            </div>
            <button
              type="button"
              className="btn btn--ghost btn--icon btn--sm"
              aria-label="Dismiss notification"
              onClick={() => actions.dismissToast(toast.id)}
            >
              ✕
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

const FOCUSABLE = 'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Modal(props: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  footer?: ReactNode;
  size?: 'sm' | 'md';
  labelledBy?: string;
}): ReactNode {
  const ref = useRef<HTMLDivElement>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    previouslyFocused.current = document.activeElement as HTMLElement | null;
    const node = ref.current;
    const first = node?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? node)?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        props.onClose();
        return;
      }
      if (event.key !== 'Tab' || !node) return;
      const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (!items.length) return;
      const firstEl = items[0];
      const lastEl = items[items.length - 1];
      if (event.shiftKey && document.activeElement === firstEl) {
        event.preventDefault();
        lastEl.focus();
      } else if (!event.shiftKey && document.activeElement === lastEl) {
        event.preventDefault();
        firstEl.focus();
      }
    };
    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      previouslyFocused.current?.focus?.();
    };
  }, [props]);

  return createPortal(
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        className={`modal${props.size === 'sm' ? ' modal--sm' : ''}`}
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        ref={ref}
        tabIndex={-1}
      >
        <div className="modal__head">
          <h2>{props.title}</h2>
          <button type="button" className="btn btn--ghost btn--icon" aria-label="Close dialog" onClick={props.onClose}>
            ✕
          </button>
        </div>
        <div className="modal__body">{props.children}</div>
        {props.footer ? <div className="modal__foot">{props.footer}</div> : null}
      </div>
    </div>,
    document.body,
  );
}

export function ConfirmDialog(props: {
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}): ReactNode {
  return (
    <Modal
      title={props.title}
      size="sm"
      onClose={props.onCancel}
      footer={
        <>
          <button type="button" className="btn" onClick={props.onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={props.danger ? 'btn btn--danger' : 'btn btn--primary'}
            onClick={props.onConfirm}
          >
            {props.confirmLabel ?? 'Confirm'}
          </button>
        </>
      }
    >
      <p className="muted">{props.message}</p>
    </Modal>
  );
}

export function EmptyState(props: { icon?: string; title: string; children?: ReactNode; action?: ReactNode }): ReactNode {
  return (
    <div className="empty">
      <div className="empty__icon" aria-hidden="true">{props.icon ?? '🗂️'}</div>
      <h3>{props.title}</h3>
      {props.children ? <div>{props.children}</div> : null}
      {props.action ? <div className="btn-row" style={{ justifyContent: 'center', marginTop: '0.5rem' }}>{props.action}</div> : null}
    </div>
  );
}

export function MarkdownView(props: { markdown: string; onCitation?: (marker: number) => void; className?: string }): ReactNode {
  const html = renderMarkdown(props.markdown);
  return (
    <div
      className={props.className ?? 'msg__content'}
      onClick={(event) => {
        const target = (event.target as HTMLElement).closest<HTMLElement>('[data-citation]');
        if (target && props.onCitation) {
          const marker = Number(target.dataset.citation);
          if (Number.isFinite(marker)) props.onCitation(marker);
        }
      }}
      // Content is escaped by renderMarkdown before markup is added.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

export function StatusBadge(props: { status: 'ready' | 'partial' | 'error'; label?: string }): ReactNode {
  const text = props.label ?? (props.status === 'ready' ? 'ready' : props.status === 'partial' ? 'partial' : 'failed');
  return <span className={`badge badge--${props.status}`}>{text}</span>;
}

export function Spinner(props: { label?: string }): ReactNode {
  return (
    <span className="row" style={{ gap: '0.45rem' }}>
      <span className="spinner" aria-hidden="true" />
      {props.label ? <span className="small muted">{props.label}</span> : null}
    </span>
  );
}

export function Field(props: { label: string; hint?: string; children: ReactNode; htmlFor?: string }): ReactNode {
  return (
    <div className="field">
      <label htmlFor={props.htmlFor}>{props.label}</label>
      {props.children}
      {props.hint ? <div className="hint">{props.hint}</div> : null}
    </div>
  );
}
