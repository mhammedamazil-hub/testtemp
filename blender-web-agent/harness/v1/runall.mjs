import puppeteer from 'puppeteer-core';
import { writeFileSync, mkdirSync, appendFileSync, rmSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

const plan = JSON.parse(await (await import('node:fs/promises')).readFile('/home/user/bw-probe/agent/plan.json', 'utf8'));
const outRoot = '/home/user/bw-probe/runs';
rmSync(outRoot, { recursive: true, force: true }); mkdirSync(outRoot, { recursive: true });

const browser = await puppeteer.launch({
  executablePath: '/tmp/chromium', protocolTimeout: 600000,
  args: ['--no-sandbox','--disable-setuid-sandbox','--disable-dev-shm-usage','--headless=shell',
    '--enable-unsafe-swiftshader', `--user-data-dir=/tmp/cr-${Date.now()}`,
    '--js-flags=--max-old-space-size=2048','--enable-features=SharedArrayBuffer'],
  env: { ...process.env, LD_LIBRARY_PATH: '/tmp/lib' },
});

const summary = [];
for (const task of plan.tasks) {
  const dir = outRoot + '/' + task.id; mkdirSync(dir, { recursive: true });
  const log = dir + '/console.log';
  const t0 = Date.now();
  const page = await browser.newPage();
  let result = null, done = false; const files = {}; let cur = null;
  const emit = (l) => appendFileSync(log, l + '\n');
  page.on('console', m => {
    const t = m.text(); emit(t.slice(0, 500));
    if (t.startsWith('BW_RESULT ')) { try { result = JSON.parse(t.slice(10)); } catch(e) {} }
    else if (t.startsWith('BW_FILE_BEGIN ')) { const p = t.split(' '); cur = { name: p[1], chunks: [] }; }
    else if (t.startsWith('BW_CHUNK ')) { if (cur) cur.chunks.push(t.slice(9)); }
    else if (t.startsWith('BW_FILE_END ')) { if (cur) { let b = Buffer.from(cur.chunks.join(''), 'base64'); if (cur.name.endsWith('.gz')) b = gunzipSync(b); files[cur.name.replace(/\.gz$/,'')] = b; cur = null; } }
    else if (t.startsWith('BW_DONE')) { done = true; }
  });
  page.on('pageerror', e => emit('[pageerror] ' + String(e)));
  try {
    await page.goto('http://127.0.0.1:8090/agent.html?task=' + task.id, { waitUntil: 'domcontentloaded', timeout: 60000 });
    for (let i = 0; i < 60 && !done; i++) await new Promise(r => setTimeout(r, 2000));
  } catch (e) { emit('[driver-error] ' + String(e).slice(0,300)); }
  for (const [n, b] of Object.entries(files)) writeFileSync(dir + '/' + n, b);
  if (result) writeFileSync(dir + '/result.json', JSON.stringify(result, null, 1));
  const ok = result && result.plan_ok && result.tasks.every(t => t.ops.every(o => o.ok));
  summary.push({ id: task.id, ok, done, seconds: Math.round((Date.now()-t0)/1000), artifacts: Object.keys(files),
                 exportBytes: result?.tasks?.[0]?.artifacts || null });
  console.log(JSON.stringify(summary.at(-1)));
  await page.close().catch(()=>{});
}
writeFileSync(outRoot + '/summary.json', JSON.stringify(summary, null, 1));
console.log('ALL_DONE ' + summary.filter(s => s.ok).length + '/' + summary.length);
await Promise.race([browser.close(), new Promise(r => setTimeout(r, 10000))]).catch(()=>{});
process.exit(0);
