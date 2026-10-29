/*
 * SourceForge service worker.
 *
 * Strategy:
 *  - App shell: network-first for navigations, falling back to the cached
 *    index.html so the shell opens offline.
 *  - Same-origin static assets (JS/CSS/icons/fonts/wasm): stale-while-revalidate.
 *  - Everything else (AI providers, GitHub API, reader services, any
 *    cross-origin request) is never cached and always goes to the network.
 *    API keys live only in memory, so they cannot be captured by the cache,
 *    and provider responses never land in Cache Storage.
 */

const VERSION = 'sourceforge-v1';
const SHELL_CACHE = `${VERSION}-shell`;
const ASSET_CACHE = `${VERSION}-assets`;
const SHELL = ['./', './index.html', './manifest.webmanifest', './icons/icon.svg', './icons/icon-192.png'];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(SHELL_CACHE)
      .then((cache) => cache.addAll(SHELL))
      .catch(() => undefined)
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => !key.startsWith(VERSION)).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('message', (event) => {
  if (event.data && event.data.type === 'SKIP_WAITING') self.skipWaiting();
  if (event.data && event.data.type === 'CLEAR_CACHES') {
    event.waitUntil(caches.keys().then((keys) => Promise.all(keys.map((key) => caches.delete(key)))));
  }
});

function isCacheableAsset(url) {
  return /\.(js|css|woff2?|ttf|png|svg|webp|wasm|json|webmanifest)$/.test(url.pathname);
}

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return; // never cache third-party responses
  if (url.pathname.endsWith('/sw.js')) return;

  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone();
          caches.open(SHELL_CACHE).then((cache) => cache.put('./index.html', copy)).catch(() => undefined);
          return response;
        })
        .catch(async () => {
          const cached = await caches.match('./index.html');
          return (
            cached ||
            new Response('SourceForge is offline and the app shell has not been cached yet. Reconnect and reload once.', {
              status: 503,
              headers: { 'content-type': 'text/plain; charset=utf-8' },
            })
          );
        }),
    );
    return;
  }

  if (!isCacheableAsset(url)) return;

  event.respondWith(
    caches.open(ASSET_CACHE).then(async (cache) => {
      const cached = await cache.match(request);
      const network = fetch(request)
        .then((response) => {
          if (response.ok) cache.put(request, response.clone()).catch(() => undefined);
          return response;
        })
        .catch(() => undefined);
      if (cached) {
        event.waitUntil(network);
        return cached;
      }
      const response = await network;
      return (
        response ||
        new Response('', { status: 504, statusText: 'Offline and not cached' })
      );
    }),
  );
});
