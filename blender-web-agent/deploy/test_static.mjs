// test_static.mjs — E2E for the zero-backend static bundle, served by a host
// that cannot send COOP/COEP (the GitHub Pages constraint). Proves the build
// works with no server-side logic whatsoever.
import puppeteer from 'puppeteer-core';
import { writeFileSync, mkdirSync } from 'node:fs';

const URL_ = process.argv[2] || 'http://127.0.0.1:8089/';
const OUT = process.argv[3] || '/home/user/bw-static-evidence';
mkdirSync(OUT, { recursive: true });
const results = [];
const check = (n, ok, d) => {
  results.push({ name: n, ok: Boolean(ok), detail: d });
  console.log((ok ? 'PASS ' : 'FAIL ') + n + (d !== undefined ? '  ' + JSON.stringify(d).slice(0, 240) : ''));
};

const browser = await puppeteer.launch({
  executablePath: '/tmp/chromium', protocolTimeout: 600000,
  args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage', '--headless=shell',
    '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
    `--user-data-dir=/tmp/static-${Date.now()}`],
  env: { ...process.env, LD_LIBRARY_PATH: '/tmp/lib' },
});
const page = await browser.newPage();
await page.setViewport({ width: 1440, height: 900 });
const errs = [];
page.on('pageerror', (e) => errs.push(String(e).slice(0, 200)));

try {
  const responses = [];
  page.on('response', (r) => responses.push(r.status()));
  await page.goto(URL_, { waitUntil: 'domcontentloaded', timeout: 60000 });

  // First load installs the service worker and reloads itself for isolation.
  await page.waitForFunction('self.crossOriginIsolated === true', { timeout: 90000 });
  check('cross-origin isolated on a header-less host', true,
    await page.evaluate(() => ({ sab: typeof SharedArrayBuffer, sw: !!navigator.serviceWorker.controller })));

  // The bundle must contain no API surface at all — it is static files only.
  const noApi = await page.evaluate(async () => {
    const r = await fetch('/api/health').catch(() => null);
    return { status: r ? r.status : 'network-error', ok: r ? r.ok : false };
  });
  check('no backend endpoints exist in the static bundle', noApi.ok === false, noApi);

  // Press Run and wait for the engine to finish the real plan.
  await page.waitForFunction('window.__bwStatic && !window.__bwStatic.state.running', { timeout: 30000 });
  await page.evaluate(() => { sessionStorage.setItem('bw.static.autorun', '0'); window.__bwStatic.run(); });
  await page.waitForFunction('Object.keys(window.__bwStatic.state.files).length > 0', { timeout: 240000 });
  await page.waitForFunction('window.__bwStatic.state.model !== null', { timeout: 60000 });

  const run = await page.evaluate(() => ({
    files: Object.keys(window.__bwStatic.state.files).map((n) => [n, window.__bwStatic.state.files[n].length]),
    meshes: window.__bwStatic.state.model.meshes.length,
    generator: window.__bwStatic.state.model.generator,
    label: document.getElementById('vpLabel').textContent,
    logEvents: document.querySelectorAll('#log .ev').length,
    resultRows: document.querySelectorAll('#result .kv').length,
  }));
  check('real Blender produced a GLB in the browser, no server', run.files.some(([n]) => n.endsWith('.glb')), run.files);
  check('GLB decoded into renderable meshes', run.meshes > 0, { meshes: run.meshes, generator: run.generator });
  check('preview reports real edge counts', /edges/.test(run.label), run.label);
  check('execution log captured Blender output', run.logEvents > 5, run.logEvents);
  check('result panel rendered execution facts', run.resultRows > 2, run.resultRows);

  const painted = await page.evaluate(() => {
    const c = document.getElementById('preview');
    const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 3; i < d.length; i += 4 * 89) if (d[i] > 0) n++;
    return n;
  });
  check('geometry actually painted on the canvas', painted > 0, painted);

  // A download link must exist for every artifact (bytes are real, in-page).
  const links = await page.evaluate(() =>
    [...document.querySelectorAll('#artifacts a.dl')].map((a) => a.download));
  check('downloadable artifacts offered', links.length > 0, links);

  await page.screenshot({ path: OUT + '/static-build.png' });
  await page.screenshot({ path: OUT + '/static-build-full.png', fullPage: true });
} catch (e) {
  check('static E2E completed without exception', false, String(e).slice(0, 300));
} finally {
  check('no uncaught page errors', errs.length === 0, errs.slice(0, 3));
  writeFileSync(OUT + '/results.json', JSON.stringify({ url: URL_, results, errs }, null, 2));
  await Promise.race([browser.close(), new Promise((r) => setTimeout(r, 8000))]).catch(() => {});
}
const passed = results.filter((r) => r.ok).length;
console.log(`STATIC ${passed}/${results.length} checks passed`);
process.exit(passed === results.length ? 0 : 1);
