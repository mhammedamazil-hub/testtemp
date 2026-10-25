// run_plan.mjs — execute ONE plan against the real Blender wasm runtime.
//
// Self-contained: it serves the payload itself (COOP/COEP required for the
// pthreads build), boots a fresh Chromium profile, runs the plan, and writes
// machine-readable output. One process == one isolated runtime session.
//
// usage: node run_plan.mjs --plan <plan.json> --out <dir> [--restore <snap.json>]
//                          [--timeout-ms 300000] [--bin <bin dir>] [--harness <dir>]
//
// exit codes: 0 = plan executed (inspect result.json for per-op status)
//             3 = runtime produced no result (crash / hang / timeout)
//             4 = bad invocation
import { createServer } from 'node:http';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, copyFileSync, existsSync } from 'node:fs';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, extname, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';
import puppeteer from 'puppeteer-core';

const HERE = dirname(fileURLToPath(import.meta.url));

function arg(name, fallback) {
  const i = process.argv.indexOf('--' + name);
  return i > -1 ? process.argv[i + 1] : fallback;
}

const planPath = arg('plan');
const outDir = arg('out');
const restorePath = arg('restore', null);
const timeoutMs = Number(arg('timeout-ms', 300000));
const binDir = resolve(arg('bin', '/home/user/bw/blender-web-local/bin'));
const harnessDir = resolve(arg('harness', HERE));
const chromiumPath = resolve(arg('chromium', '/tmp/chromium'));
const libDir = arg('lib-dir', '/tmp/lib');

if (!planPath || !outDir) {
  console.error('usage: node run_plan.mjs --plan <plan.json> --out <dir> [--restore <snap.json>]');
  process.exit(4);
}

mkdirSync(outDir, { recursive: true });

// --- per-run work dir: static files handed to the page -------------------
const work = mkdtempSync(join(tmpdir(), 'bw-run-'));
for (const f of ['agent.html', 'bw_ops.py']) {
  copyFileSync(join(harnessDir, f), join(work, f));
}
copyFileSync(planPath, join(work, 'plan.json'));
if (restorePath && existsSync(restorePath)) copyFileSync(restorePath, join(work, 'restore.json'));

const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.py': 'text/plain',
  '.json': 'application/json', '.wasm': 'application/wasm',
  '.data': 'application/octet-stream', '.css': 'text/css',
};

const uploads = [];
const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  const p = decodeURIComponent(url.pathname);
  if (req.method === 'POST' && p === '/_artifact') {
    const name = (url.searchParams.get('name') || 'artifact.bin').replace(/[^\w.-]/g, '_');
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const buf = Buffer.concat(chunks);
      writeFileSync(join(outDir, name), buf);
      uploads.push({ name, bytes: buf.length });
      res.writeHead(200, { 'Content-Length': '0' });
      res.end();
    });
    return;
  }
  let file;
  if (p.startsWith('/bin/')) file = join(binDir, p.slice(5));
  else if (p === '/' || p === '/agent.html') file = join(work, 'agent.html');
  else if (p === '/plan.json' || p === '/restore.json' || p === '/bw_ops.py') file = join(work, p.slice(1));
  else file = null;
  if (!file || !existsSync(file)) {
    res.writeHead(404, { 'Content-Length': '0' }); res.end(); return;
  }
  const body = readFileSync(file);
  res.writeHead(200, {
    'Content-Type': MIME[extname(file).toLowerCase()] || 'application/octet-stream',
    'Content-Length': body.length,
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp',
    'Cross-Origin-Resource-Policy': 'same-origin',
    'Cache-Control': 'no-store',
  });
  res.end(body);
});

const port = await new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server.address().port)));

// --- runtime -------------------------------------------------------------
const t0 = Date.now();
const logs = [];
const files = {};
let result = null, done = false, current = null;
let browser = null;
let exited = false;

const timer = setTimeout(() => {
  console.error('TIMEOUT after ' + timeoutMs + 'ms');
  exited = true;
}, timeoutMs);

try {
  browser = await puppeteer.launch({
    executablePath: chromiumPath,
    protocolTimeout: timeoutMs + 30000,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage',
      '--headless=shell', '--enable-unsafe-swiftshader',
      `--user-data-dir=${join(work, 'profile')}`,
      '--js-flags=--max-old-space-size=2048', '--enable-features=SharedArrayBuffer'],
    env: { ...process.env, LD_LIBRARY_PATH: libDir },
  });
  const page = await browser.newPage();
  page.on('console', (m) => {
    const t = m.text();
    logs.push(t.slice(0, 2000));
    if (t.startsWith('BW_RESULT ')) {
      try { result = JSON.parse(t.slice(10)); } catch (e) { logs.push('[driver] bad BW_RESULT: ' + e); }
    } else if (t.startsWith('BW_FILE_BEGIN ')) {
      const p = t.split(' ');
      current = { name: p[1], chunks: [] };
    } else if (t.startsWith('BW_CHUNK ')) {
      if (current) current.chunks.push(t.slice(9));
    } else if (t.startsWith('BW_FILE_END ')) {
      if (current) {
        let buf = Buffer.from(current.chunks.join(''), 'base64');
        try { buf = gunzipSync(buf); } catch (e) { /* not gzipped */ }
        files[current.name] = buf;
        current = null;
      }
    } else if (t.startsWith('BW_DONE')) {
      done = true;
    }
  });
  page.on('pageerror', (e) => logs.push('[pageerror] ' + String(e).slice(0, 500)));
  page.on('error', (e) => { logs.push('[crash] ' + String(e).slice(0, 500)); exited = true; });

  await page.goto(`http://127.0.0.1:${port}/agent.html`, { waitUntil: 'domcontentloaded', timeout: 60000 });

  while (!done && !exited) {
    await new Promise((r) => setTimeout(r, 500));
    if (Date.now() - t0 > timeoutMs) { exited = true; break; }
  }
  if (done) {
    // wait for in-flight artifact POSTs before tearing the browser down
    await page.waitForFunction('window.__bwArtifactsPending === 0', { timeout: 30000 })
      .catch(() => logs.push('[driver] artifact uploads still pending at shutdown'));
    await new Promise((r) => setTimeout(r, 300));
  }
} catch (e) {
  logs.push('[driver-error] ' + String(e).slice(0, 500));
} finally {
  clearTimeout(timer);
  if (browser) {
    await Promise.race([browser.close(), new Promise((r) => setTimeout(r, 8000))]).catch(() => {});
  }
  server.close();
}

// --- outputs -------------------------------------------------------------
const elapsed = Number(((Date.now() - t0) / 1000).toFixed(2));
for (const [name, buf] of Object.entries(files)) writeFileSync(join(outDir, name), buf);
writeFileSync(join(outDir, 'console.log'), logs.join('\n'));

const payload = result
  ? { ...result, wall_clock_s: elapsed, runtime_exit: 'ok' }
  : { schema: null, plan_ok: false, wall_clock_s: elapsed, runtime_exit: exited ? 'timeout-or-crash' : 'no-result',
      error: { code: exited ? 'RUNTIME_TIMEOUT' : 'RUNTIME_NO_RESULT',
               message: 'runtime produced no result within ' + elapsed + 's' },
      tasks: [] };
writeFileSync(join(outDir, 'result.json'), JSON.stringify(payload, null, 2));
writeFileSync(join(outDir, 'artifacts.json'), JSON.stringify(uploads, null, 2));

console.log(`RESULT ${payload.plan_ok ? 'plan_ok' : 'FAILED'} elapsed=${elapsed}s artifacts=${uploads.length}`);
process.exit(result ? 0 : 3);
