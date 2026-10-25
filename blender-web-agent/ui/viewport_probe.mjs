// viewport_probe.mjs — does the interactive Blender viewport actually render
// and respond to input? A startup log is not evidence; this samples the canvas
// and the runtime's own present counters, then injects pointer input.
//
// usage: node viewport_probe.mjs [--flags "a b c"] [--seconds 25] [--out dir]
import { createServer } from 'node:http';
import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join, extname } from 'node:path';
import { createHash } from 'node:crypto';
import puppeteer from 'puppeteer-core';

const arg = (k, d) => {
  const i = process.argv.indexOf('--' + k);
  return i > -1 ? process.argv[i + 1] : d;
};
const extraFlags = (arg('flags', '')).split(' ').filter(Boolean);
const seconds = Number(arg('seconds', 25));
const outDir = arg('out', '/home/user/bw-ui');
const SHELL = '/home/user/bw/blender-web-local/shell';
const BIN = '/home/user/bw/blender-web-local/bin';
const label = arg('label', 'default');

mkdirSync(outDir, { recursive: true });
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
  '.wasm': 'application/wasm', '.data': 'application/octet-stream',
  '.json': 'application/json', '.png': 'image/png', '.ttf': 'font/ttf' };

const server = createServer((req, res) => {
  const p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  let file;
  if (p.startsWith('/bin/')) file = join(BIN, p.slice(5));
  else if (p === '/') file = join(SHELL, 'windowed.html');
  else file = join(SHELL, p.replace(/^\//, ''));
  if (!existsSync(file)) { res.writeHead(404); res.end(); return; }
  const body = readFileSync(file);
  res.writeHead(200, {
    'Content-Type': MIME[extname(file).toLowerCase()] || 'application/octet-stream',
    'Content-Length': body.length,
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp',
    'Cross-Origin-Resource-Policy': 'same-origin',
  });
  res.end(body);
});
const port = await new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server.address().port)));

const report = { label, extraFlags, supported: false, reason: null, samples: [], input: null };
let browser;
try {
  browser = await puppeteer.launch({
    executablePath: '/tmp/chromium',
    protocolTimeout: 180000,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage',
      '--headless=shell', '--ignore-gpu-blocklist', '--disable-gpu-sandbox',
      '--enable-unsafe-swiftshader', `--user-data-dir=/tmp/uiprobe-${Date.now()}`,
      '--enable-features=SharedArrayBuffer', ...extraFlags],
    env: { ...process.env, LD_LIBRARY_PATH: '/tmp/lib' },
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 900, height: 600, deviceScaleFactor: 1 });
  const errors = [];
  page.on('console', (m) => {
    const t = m.text();
    if (/GPU-LOST|Instance dropped|rejected/.test(t)) errors.push(t.slice(0, 160));
  });
  page.on('pageerror', (e) => errors.push('[pageerror] ' + String(e).slice(0, 160)));

  await page.goto(`http://127.0.0.1:${port}/?gate=900x600`, { waitUntil: 'domcontentloaded', timeout: 60000 });

  const t0 = Date.now();
  let booted = false;
  for (let i = 0; i < 40; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    const st = await page.evaluate(() => {
      const el = document.getElementById('state');
      return el ? el.textContent.trim() : '';
    }).catch(() => '');
    if (/main loop/i.test(st)) { booted = true; break; }
    if (Date.now() - t0 > seconds * 1000) break;
  }
  report.booted_main_loop = booted;

  const sample = async () => page.evaluate(() => {
    const mod = window.__bwModule;
    const c = document.querySelector('#canvas');
    const num = (f) => { try { return typeof mod[f] === 'function' ? Number(mod[f]()) : null; } catch (e) { return null; } };
    let hash = null, nonBlack = null;
    try {
      const url = c.toDataURL('image/png');
      const bin = atob(url.split(',')[1] || '');
      let h = 0, nz = 0;
      for (let i = 0; i < bin.length; i += 97) { h = (h * 31 + bin.charCodeAt(i)) | 0; }
      hash = (h >>> 0).toString(16) + ':' + bin.length;
      // sample the decoded PNG bytes: a pure-black canvas compresses to a tiny PNG
      nonBlack = bin.length > 2000;
    } catch (e) { hash = 'err:' + String(e).slice(0, 40); }
    return {
      presentCount: num('_bw_present_count'),
      redrawEpisodes: num('_bw_redraw_episode_count'),
      wmTicks: num('_bw_wm_tick_count'),
      canvasSize: c ? `${c.width}x${c.height}` : null,
      pngHash: hash,
      nonBlack,
    };
  }).catch((e) => ({ error: String(e).slice(0, 80) }));

  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 2500));
    report.samples.push(await sample());
  }

  // inject real pointer input: click into the viewport and drag
  const before = await sample();
  await page.mouse.move(450, 300);
  await page.mouse.down();
  await page.mouse.move(560, 340, { steps: 8 });
  await page.mouse.up();
  await new Promise((r) => setTimeout(r, 2500));
  const after = await sample();
  report.input = {
    before, after,
    redraw_changed: before.redrawEpisodes !== after.redrawEpisodes,
    present_changed: before.presentCount !== after.presentCount,
    png_changed: before.pngHash !== after.pngHash,
  };

  report.supported = Boolean(
    booted && after.presentCount !== null && after.presentCount > 0
    && (report.input.png_changed || report.input.redraw_changed));
  if (!report.supported) {
    report.reason = errors.length
      ? 'renderer errors: ' + [...new Set(errors)].slice(0, 3).join(' | ')
      : (booted ? 'main loop reached but the viewport never presented a frame'
                : 'never reached the Blender main loop');
  }
  await page.screenshot({ path: join(outDir, `page-${label}.png`) }).catch(() => {});
} catch (e) {
  report.reason = 'driver error: ' + String(e).slice(0, 200);
} finally {
  if (browser) await Promise.race([browser.close(), new Promise((r) => setTimeout(r, 8000))]).catch(() => {});
  server.close();
}

writeFileSync(join(outDir, `probe-${label}.json`), JSON.stringify(report, null, 2));
console.log(JSON.stringify({ label, supported: report.supported, reason: report.reason,
  samples: report.samples.map((s) => ({ present: s.presentCount, redraw: s.redrawEpisodes, nonBlack: s.nonBlack })),
  input: report.input && { redraw_changed: report.input.redraw_changed, png_changed: report.input.png_changed } }));
process.exit(report.supported ? 0 : 1);
