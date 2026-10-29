import { expect, test } from '@playwright/test';

/**
 * End-to-end checks against the production build. Covers the paths that unit
 * tests cannot: real IndexedDB persistence across reloads, downloads produced by
 * the browser, focus/keyboard handling and the actual rendered dialog flow.
 *
 * Not executed in the environment where this was built (no Chromium available);
 * run with `npm run test:e2e` or in CI.
 */

async function createNotebook(page: import('@playwright/test').Page, title = 'My first notebook') {
  await page.getByRole('button', { name: 'Create a notebook' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(title);
}

async function pasteSource(page: import('@playwright/test').Page, title: string, content: string) {
  await page.getByRole('button', { name: 'Add source' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add sources' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('tab', { name: 'Paste' }).click();
  await dialog.getByLabel('Title').fill(title);
  await dialog.getByLabel('Content').fill(content);
  await dialog.getByRole('button', { name: 'Add pasted source' }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('tab', { name: /Sources \(/ })).toBeVisible();
}

test.describe('research workflow', () => {
  test('answers from a pasted source and opens the cited passage', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await pasteSource(
      page,
      'Annual report 2023',
      'Enterprise renewal rates reached 92 percent in 2023. Onboarding quality was the main driver of retention.',
    );

    await page.getByRole('tab', { name: /^Chat/ }).click();
    await page.locator('#chat-input').fill('What were the enterprise renewal rates?');
    await page.getByRole('button', { name: 'Ask' }).click();

    await expect(page.locator('.cite-chip').first()).toBeVisible();
    const chip = page.locator('.cite-chip').first();
    await expect(chip).toContainText('Annual report 2023');
    await chip.click();

    const viewer = page.getByRole('dialog');
    await expect(viewer).toBeVisible();
    await expect(viewer.getByText('Cited passage')).toBeVisible();
    await expect(viewer).toContainText('92 percent');
    await viewer.getByRole('button', { name: 'Close dialog' }).click();
  });

  test('refuses to answer without evidence', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await pasteSource(page, 'Caching notes', 'Write-through caches update the backing store synchronously.');

    await page.getByRole('tab', { name: /^Chat/ }).click();
    await page.locator('#chat-input').fill('What is the boiling point of mercury?');
    await page.getByRole('button', { name: 'Ask' }).click();
    await expect(page.getByText('Insufficient evidence')).toBeVisible();
    await expect(page.locator('.cite-chip')).toHaveCount(0);
  });

  test('persists notebooks and sources across a reload', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page, 'Persistence check');
    await pasteSource(page, 'Stored source', 'A sentence that must survive a page reload.');
    await page.reload();
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Persistence check');
    await expect(page.getByRole('tab', { name: 'Sources (1)' })).toBeVisible();
  });

  test('generates an offline study artifact and exports a CSV', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await pasteSource(
      page,
      'Retention study',
      'Net revenue retention increased to 118 percent in 2023. Enterprise renewal rates reached 92 percent. Onboarding quality drove the improvement.',
    );
    await page.getByRole('tab', { name: /^Studio/ }).click();
    await page.getByRole('button', { name: /^Flashcards/ }).click();
    await expect(page.getByText('offline deterministic')).toBeVisible();
    await page.getByRole('button', { name: 'View' }).click();
    await expect(page.getByRole('button', { name: 'Reveal' }).first()).toBeVisible();
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'CSV' }).first().click();
    expect((await download).suggestedFilename()).toMatch(/\.csv$/);
  });

  test('exports a valid backup and a PDF report', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await pasteSource(page, 'Article', 'Backups must round-trip notebook data without loss.');

    const backup = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Export backup' }).click();
    const backupFile = await backup;
    expect(backupFile.suggestedFilename()).toMatch(/^sourceforge-backup-\d{4}-\d{2}-\d{2}\.json$/);

    await page.getByRole('tab', { name: /^Studio/ }).click();
    const pdf = page.waitForEvent('download');
    await page.getByRole('button', { name: 'PDF report' }).click();
    expect((await pdf).suggestedFilename()).toMatch(/\.pdf$/);
  });

  test('searches across notebooks and sources with the keyboard shortcut', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await pasteSource(page, 'Quokka notes', 'Quokkas are marsupials native to Rottnest Island.');
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.locator('#global-search')).toBeFocused();
    await page.locator('#global-search').fill('quokka');
    await expect(page.getByText('Quokka notes').first()).toBeVisible();
  });

  test('creates, renames and deletes a notebook', async ({ page }) => {
    await page.goto('/');
    await createNotebook(page);
    await page.getByRole('button', { name: 'Rename My first notebook' }).click();
    await page.getByLabel('Notebook title').fill('Renamed notebook');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Renamed notebook');

    await page.getByRole('button', { name: 'Delete Renamed notebook' }).click();
    await page.getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Welcome to SourceForge' })).toBeVisible();
  });
});

test.describe('accessibility and layout', () => {
  test('has a skip link, landmarks and a focusable first control', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('a.skip-link')).toBeAttached();
    await expect(page.locator('nav[aria-label="Notebooks"]')).toBeVisible();
    await expect(page.locator('main#main-content')).toBeVisible();
    await page.keyboard.press('Tab');
    const tag = await page.evaluate(() => document.activeElement?.tagName ?? '');
    expect(['A', 'BUTTON', 'INPUT']).toContain(tag);
  });

  test('is usable at a narrow viewport', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/');
    await createNotebook(page);
    await page.getByRole('button', { name: 'Open navigation' }).click();
    await expect(page.locator('nav[aria-label="Notebooks"]')).toBeVisible();
  });
});
