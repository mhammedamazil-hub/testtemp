import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import App from '../src/App';
import { actions, store } from '../src/lib/store';

/**
 * Runtime smoke test of the real UI tree in jsdom: mount, create a notebook,
 * import a pasted source through the dialog, ask a question in offline mode and
 * generate a study artifact. This is the closest thing to a browser check that
 * runs in this environment; e2e/ covers the real Chromium paths in CI.
 */

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function waitFor(predicate: () => boolean, label: string, timeout = 4000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (predicate()) return;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
  }
  throw new Error(`Timed out waiting for ${label}. Body: ${document.body.textContent?.slice(0, 600)}`);
}

function text(): string {
  return document.body.textContent ?? '';
}

function buttonByText(pattern: RegExp): HTMLButtonElement {
  const buttons = Array.from(document.querySelectorAll('button'));
  const found = buttons.find((button) => pattern.test((button.textContent ?? '').trim()));
  if (!found) throw new Error(`No button matching ${pattern}. Buttons: ${buttons.map((b) => b.textContent?.trim()).join(' | ')}`);
  return found as HTMLButtonElement;
}

async function click(element: Element | null | undefined): Promise<void> {
  if (!element) throw new Error('click target missing');
  await act(async () => {
    (element as HTMLElement).click();
  });
}

async function typeInto(element: HTMLInputElement | HTMLTextAreaElement | null, value: string): Promise<void> {
  if (!element) throw new Error('input missing');
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
  await act(async () => {
    setter?.call(element, value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

beforeAll(() => {
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

beforeEach(async () => {
  await actions.wipeAll();
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root.render(<App />);
  });
  await waitFor(() => text().includes('Welcome to SourceForge') || text().includes('Opening your local workspace'), 'initial render');
  await waitFor(() => text().includes('Welcome to SourceForge'), 'welcome screen');
});

afterEach(async () => {
  await act(async () => {
    root.unmount();
  });
  container.remove();
});

afterAll(() => {
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = false;
});

describe('application shell', () => {
  it('renders the workspace with navigation, skip link and storage status', () => {
    expect(document.querySelector('a.skip-link')?.textContent).toContain('Skip to main content');
    expect(document.querySelector('nav[aria-label="Notebooks"]')).not.toBeNull();
    expect(document.querySelector('main#main-content')).not.toBeNull();
    expect(text()).toContain('SourceForge');
    expect(text()).toMatch(/Storage/i);
    expect(['indexeddb', 'memory']).toContain(store.get().storage.backend);
    expect(store.get().storage.message).not.toMatch(/unavailable|could not/i);
  });

  it('creates a notebook and shows it in the sidebar', async () => {
    await click(buttonByText(/^Create a notebook$/));
    await waitFor(() => document.querySelector('h1')?.textContent === 'My first notebook', 'notebook heading');
    const state = store.get();
    expect(state.notebooks).toHaveLength(1);
    expect(state.notebooks[0].title).toBe('My first notebook');
    expect(text()).toContain('0 sources');
  });

  it('imports a pasted source and answers a question with a resolvable citation', async () => {
    await click(buttonByText(/^Create a notebook$/));
    await waitFor(() => document.querySelector('h1')?.textContent === 'My first notebook', 'notebook');
    await click(buttonByText(/Add source/));
    await waitFor(() => document.querySelector('[role="dialog"][aria-label="Add sources"]') !== null, 'add-source dialog');

    await click(buttonByText(/^Paste$/));
    await typeInto(document.querySelector('#paste-title'), 'Annual report');
    await typeInto(
      document.querySelector('#paste-text'),
      'Enterprise renewal rates reached 92 percent in 2023. Onboarding quality was the main driver of retention.',
    );
    await click(buttonByText(/Add pasted source/));
    await waitFor(() => store.get().sources.length === 1, 'source stored');
    expect(store.get().chunks.length).toBeGreaterThan(0);
    await waitFor(() => text().includes('Sources (1)'), 'source tab count');

    await click(buttonByText(/^Chat \(/));
    await typeInto(document.querySelector('#chat-input'), 'What were the enterprise renewal rates?');
    await click(buttonByText(/^Ask$/));
    await waitFor(() => document.querySelectorAll('.cite-chip').length > 0, 'citation chip', 8000);

    const answer = store.get().messages.find((message) => message.role === 'assistant');
    expect(answer).toBeTruthy();
    expect(answer?.content).toMatch(/92 percent/);
    expect(answer?.citations?.length).toBeGreaterThan(0);
    expect(answer?.citations?.[0].chunkId).toMatch(/^[^#]+#\d+$/);

    await click(document.querySelector('.cite-chip'));
    await waitFor(() => text().includes('Cited passage'), 'passage viewer');
    expect(text()).toContain('92 percent');
  });

  it('reports insufficient evidence instead of inventing an answer', async () => {
    await click(buttonByText(/^Create a notebook$/));
    await waitFor(() => document.querySelector('h1')?.textContent === 'My first notebook', 'notebook');
    await click(buttonByText(/Add source/));
    await waitFor(() => document.querySelector('[role="dialog"]') !== null, 'dialog');
    await click(buttonByText(/^Paste$/));
    await typeInto(document.querySelector('#paste-title'), 'Caching notes');
    await typeInto(document.querySelector('#paste-text'), 'Write-through caches update the backing store synchronously.');
    await click(buttonByText(/Add pasted source/));
    await waitFor(() => store.get().sources.length === 1, 'source stored');

    await click(buttonByText(/^Chat \(/));
    await typeInto(document.querySelector('#chat-input'), 'What is the boiling point of mercury?');
    await click(buttonByText(/^Ask$/));
    await waitFor(() => text().includes('Insufficient evidence'), 'insufficient evidence notice', 8000);
    expect(document.querySelectorAll('.cite-chip')).toHaveLength(0);
  });

  it('generates a deterministic study artifact and keeps it in the library', async () => {
    await click(buttonByText(/^Create a notebook$/));
    await waitFor(() => document.querySelector('h1')?.textContent === 'My first notebook', 'notebook');
    await click(buttonByText(/Add source/));
    await waitFor(() => document.querySelector('[role="dialog"]') !== null, 'dialog');
    await click(buttonByText(/^Paste$/));
    await typeInto(document.querySelector('#paste-title'), 'Retention study');
    await typeInto(
      document.querySelector('#paste-text'),
      'Net revenue retention increased to 118 percent in 2023. Enterprise renewal rates reached 92 percent. Onboarding quality drove the improvement.',
    );
    await click(buttonByText(/Add pasted source/));
    await waitFor(() => store.get().sources.length === 1, 'source stored');

    await click(buttonByText(/^Studio \(/));
    await click(buttonByText(/^Flashcards/));
    await waitFor(() => store.get().artifacts.length === 1, 'artifact stored', 6000);
    const artifact = store.get().artifacts[0];
    expect(artifact.kind).toBe('flashcards');
    expect(artifact.mode).toBe('local');
    expect(artifact.body).toMatch(/card/i);
  });

  it('opens settings and exposes provider configuration without a key', async () => {
    await click(buttonByText(/Settings/));
    await waitFor(() => text().includes('Provider') || text().includes('providers'), 'settings view');
    expect(text()).toMatch(/OpenAI|Anthropic|Gemini|Ollama/);
    expect(store.get().settings.providers).toHaveLength(0);
    expect(text()).toMatch(/key/i);
  });

  it('deletes a notebook after confirmation and clears its data', async () => {
    await click(buttonByText(/^Create a notebook$/));
    await waitFor(() => store.get().notebooks.length === 1, 'notebook created');
    const deleteButton = document.querySelector('button[aria-label^="Delete "]');
    await click(deleteButton);
    await waitFor(() => text().includes('Delete “'), 'delete confirmation');
    await click(buttonByText(/^Delete$/));
    await waitFor(() => store.get().notebooks.length === 0, 'notebook deleted');
    await waitFor(() => text().includes('Welcome to SourceForge'), 'welcome screen again');
  });
});
