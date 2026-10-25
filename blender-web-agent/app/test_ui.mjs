// test_ui.mjs — drive the running app in a real browser through window.BW,
// the documented JS surface (the same one Obscura --eval uses).
//
// usage: node app/test_ui.mjs [--url http://127.0.0.1:8077] [--out dir]
import puppeteer from 'puppeteer-core';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i > -1 ? process.argv[i + 1] : d; };
const URL_ = arg('url', 'http://127.0.0.1:8077');
const OUT = arg('out', '/home/user/bw-ui-test');
mkdirSync(OUT, { recursive: true });

const results = [];
const check = (name, ok, detail) => {
  results.push({ name, ok: Boolean(ok), detail });
  console.log((ok ? 'PASS ' : 'FAIL ') + name + (detail !== undefined ? '  ' + JSON.stringify(detail).slice(0, 220) : ''));
};

const browser = await puppeteer.launch({
  executablePath: '/tmp/chromium', protocolTimeout: 600000,
  args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage', '--headless=shell',
    '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
    `--user-data-dir=/tmp/uitest-${Date.now()}`, '--enable-features=SharedArrayBuffer'],
  env: { ...process.env, LD_LIBRARY_PATH: '/tmp/lib' },
});
const page = await browser.newPage();
await page.setViewport({ width: 1440, height: 900, deviceScaleFactor: 1 });
const consoleErrors = [];
page.on('pageerror', (e) => consoleErrors.push(String(e).slice(0, 200)));
page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text().slice(0, 200)); });

try {
  await page.goto(URL_, { waitUntil: 'networkidle2', timeout: 60000 });
  await page.waitForFunction('window.BW && window.BW.ready', { timeout: 30000 });
  check('app boots and exposes window.BW', true, await page.evaluate(() => window.BW.version));

  const layout = await page.evaluate(() => ({
    left: !!document.querySelector('.panel-left'),
    center: !!document.querySelector('#preview'),
    right: !!document.querySelector('.panel-right'),
    console: !!document.querySelector('#log'),
    ai: !!document.querySelector('#taskInput'),
  }));
  check('workspace layout present (hierarchy, preview, inspector, log, AI)',
    Object.values(layout).every(Boolean), layout);

  // --- operate the app exactly as an agent would -------------------------
  const sid = await page.evaluate(() => window.BW.startSession(
    'UI test: cube with material, exported'));
  check('session created from the UI', Boolean(sid), sid);

  const opsRes = await page.evaluate(async () => {
    const r = await window.BW.submitOps([
      { op: 'object.create', type: 'cube', name: 'UiCube', size: 2.0, location: [0, 0, 1] },
      { op: 'object.set_dimensions', target: 'UiCube', dimensions: [0.5, 0.4, 0.3] },
      { op: 'mesh.bevel', target: 'UiCube', width: 0.02, segments: 2 },
      { op: 'material.create', name: 'UiMat', base_color: [0.2, 0.6, 0.9], roughness: 0.3 },
      { op: 'material.assign', material: 'UiMat', target: 'UiCube' },
    ]);
    return { ok: r.ok, ops: (r.ops || []).length, errors: (r.errors || []).length };
  });
  check('ops executed against real Blender via window.BW', opsRes.ok && opsRes.errors === 0, opsRes);

  const scene = await page.evaluate(() => window.BW.scene());
  const cube = (scene.objects || []).find((o) => o.name === 'UiCube');
  check('scene reports the created object with the new material',
    Boolean(cube) && (cube.materials || []).includes('UiMat'),
    cube && { verts: cube.verts, materials: cube.materials });

  const dom = await page.evaluate(() => ({
    rows: [...document.querySelectorAll('#hierarchy .obj .nm')].map((n) => n.textContent),
    mats: [...document.querySelectorAll('#materials .matrow')].map((n) => n.textContent),
  }));
  check('hierarchy renders objects from Blender', dom.rows.includes('UiCube'), dom.rows);
  check('material list renders from Blender', dom.mats.includes('UiMat'), dom.mats);

  // export -> the preview should decode the real GLB
  const exp = await page.evaluate(async () => {
    const r = await window.BW.submit({ schema: 'bw.agent.ops/v2', tasks: [{ id: 'ui-exp', ops: [
      { op: 'export', prefix: 'uicube', formats: ['glb'] },
    ] }] });
    return { ok: r.ok, artifacts: Object.keys(r.artifacts || {}) };
  });
  check('export through the UI produced a GLB', exp.ok && exp.artifacts.length > 0, exp);

  await page.evaluate(() => window.BW.refresh());
  await new Promise((r) => setTimeout(r, 2500));
  const preview = await page.evaluate(() => ({
    label: document.getElementById('vpLabel').textContent,
    hidden: document.getElementById('vpEmpty').style.display === 'none',
    painted: (() => {
      const c = document.getElementById('preview');
      const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 3; i < d.length; i += 4 * 97) if (d[i] > 0) n++;
      return n;
    })(),
  }));
  check('preview renders real GLB geometry (pixels drawn)', preview.painted > 0, preview);

  // an invalid op must be rejected with a clear, machine-readable error
  const bad = await page.evaluate(async () => {
    const r = await window.BW.submit({ schema: 'bw.agent.ops/v2', tasks: [{ id: 'bad', ops: [
      { op: 'scene.teleport' },
    ] }] });
    return { ok: r.ok, code: r.code, rejected: r.rejected_before_execution };
  });
  check('unknown operation rejected before execution', bad.ok === false && bad.code === 'UNKNOWN_OP', bad);

  const logLines = await page.evaluate(() => document.querySelectorAll('#log .ev').length);
  check('execution log records the run', logLines > 4, logLines);

  await page.screenshot({ path: join(OUT, 'workspace.png'), fullPage: false });
  await page.screenshot({ path: join(OUT, 'workspace-full.png'), fullPage: true });
} catch (e) {
  check('ui test completed without exception', false, String(e).slice(0, 300));
} finally {
  // The deliberate invalid-op test makes the server answer 422; the browser
  // logs that as a resource error. That is expected, not an app fault.
  const unexpected = consoleErrors.filter((e) => !/status of 4\d\d/.test(e));
  check('no unexpected page errors', unexpected.length === 0,
    { unexpected: unexpected.slice(0, 3), expected_http_4xx: consoleErrors.length - unexpected.length });
  writeFileSync(join(OUT, 'ui-results.json'), JSON.stringify({ results, consoleErrors }, null, 2));
  await Promise.race([browser.close(), new Promise((r) => setTimeout(r, 8000))]).catch(() => {});
}

const passed = results.filter((r) => r.ok).length;
console.log(`UI ${passed}/${results.length} checks passed`);
process.exit(passed === results.length ? 0 : 1);
