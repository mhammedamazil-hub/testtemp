import { expect, test } from '@playwright/test';

/**
 * PWA behaviour in a real browser: manifest served and valid, service worker
 * installed and controlling the page, and the app shell opening with the
 * network switched off. These assertions cannot be reproduced in jsdom, so
 * they only run in Chromium (CI).
 */

test('serves a valid web app manifest with installable icons', async ({ page }) => {
  await page.goto('/');
  const href = await page.getAttribute('link[rel="manifest"]', 'href');
  expect(href).toBeTruthy();
  const manifest = await page.evaluate(async (manifestHref) => {
    const response = await fetch(manifestHref as string);
    return { ok: response.ok, body: await response.json() };
  }, href);
  expect(manifest.ok).toBe(true);
  expect(manifest.body.name).toBe('SourceForge');
  expect(manifest.body.display).toBe('standalone');
  expect(manifest.body.start_url).toBeTruthy();
  const sizes = (manifest.body.icons as { sizes: string }[]).map((icon) => icon.sizes);
  expect(sizes).toContain('192x192');
  expect(sizes).toContain('512x512');
});

test('registers a service worker that controls the page', async ({ page }) => {
  await page.goto('/');
  const state = await page.evaluate(async () => {
    if (!('serviceWorker' in navigator)) return 'unsupported';
    const registration = await navigator.serviceWorker.ready;
    return registration.active?.state ?? 'inactive';
  });
  expect(state).toBe('activated');
});

test('opens the app shell while offline', async ({ page, context }) => {
  await page.goto('/');
  await page.evaluate(() => navigator.serviceWorker.ready);
  // Give the worker one navigation to cache the shell.
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Welcome to SourceForge' })).toBeVisible();

  await context.setOffline(true);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Welcome to SourceForge' })).toBeVisible();
  await expect(page.locator('.sidebar')).toBeVisible();
  await context.setOffline(false);
});

test('keeps offline data after an offline reload', async ({ page, context }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Create a notebook' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('My first notebook');
  await page.evaluate(() => navigator.serviceWorker.ready);
  await page.reload();
  await context.setOffline(true);
  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('My first notebook');
  await context.setOffline(false);
});

test('never caches provider or third-party responses', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => navigator.serviceWorker.ready);
  const keys = await page.evaluate(async () => {
    const names = await caches.keys();
    const entries: string[] = [];
    for (const name of names) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) entries.push(request.url);
    }
    return entries;
  });
  const thirdParty = keys.filter((url) => !url.startsWith('http://127.0.0.1:4173') && !url.startsWith('http://localhost:4173'));
  expect(thirdParty).toEqual([]);
});
