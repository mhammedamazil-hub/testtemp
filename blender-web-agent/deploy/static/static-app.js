/* static-app.js — fully client-side Blender runner.
 *
 * There is no server. The plan is embedded into `--python-expr` as base64,
 * Blender executes it in this tab, and artifact bytes are reassembled from the
 * runtime's own print channel and decoded here. Nothing is uploaded.
 *
 * One plan per page boot: the Blender wasm runtime cannot be re-entered after
 * it exits, so pressing Run again reloads the document with the chosen plan
 * stashed in sessionStorage. That is a property of the engine, not a shortcut.
 */
(function () {
  'use strict';

  const PRESETS = {
    cube: {
      label: 'Dimensioned cube + material',
      goal: 'Create a cube, set exact dimensions, bevel it, assign a metal material, export GLB.',
      ops: [
        { op: 'scene.reset' },
        { op: 'object.create', type: 'cube', name: 'Widget', size: 2.0, location: [0, 0, 1] },
        { op: 'object.set_dimensions', target: 'Widget', dimensions: [0.4, 0.3, 0.2] },
        { op: 'mesh.bevel', target: 'Widget', width: 0.012, segments: 2 },
        { op: 'material.create', name: 'BrushedSteel', base_color: [0.68, 0.7, 0.73], roughness: 0.28, metallic: 0.9 },
        { op: 'material.assign', material: 'BrushedSteel', target: 'Widget' },
        { op: 'export', prefix: 'widget', formats: ['glb'] },
        { op: 'validate.roundtrip', prefix: 'widget', formats: ['glb'] },
      ],
    },
    spacecraft: {
      label: 'Spacecraft: hull, wings, cockpit',
      goal: 'Low-poly spacecraft assembled from primitives with dimensions, then exported.',
      ops: [
        { op: 'scene.reset' },
        { op: 'object.create', type: 'uv_sphere', name: 'Hull', location: [0, 0, 0], scale: [1.1, 2.4, 0.85] },
        { op: 'object.create', type: 'cube', name: 'WingL', size: 2.0, location: [-1.446, 0.2, -0.1], scale: [0.75, 1.0, 0.06] },
        { op: 'object.create', type: 'cube', name: 'WingR', size: 2.0, location: [1.446, 0.2, -0.1], scale: [0.75, 1.0, 0.06] },
        { op: 'object.create', type: 'uv_sphere', name: 'Cockpit', location: [0, 1.35, 0.42], scale: [0.42, 0.6, 0.32] },
        { op: 'material.create', name: 'HullMat', base_color: [0.62, 0.64, 0.68], roughness: 0.45, metallic: 0.6 },
        { op: 'material.create', name: 'GlassMat', base_color: [0.1, 0.35, 0.55], roughness: 0.1, metallic: 0.2 },
        { op: 'material.assign', material: 'HullMat', target: 'Hull' },
        { op: 'material.assign', material: 'HullMat', target: 'WingL' },
        { op: 'material.assign', material: 'HullMat', target: 'WingR' },
        { op: 'material.assign', material: 'GlassMat', target: 'Cockpit' },
        { op: 'export', prefix: 'spacecraft', formats: ['glb'] },
      ],
    },
    trio: {
      label: 'Primitive trio + OBJ',
      goal: 'Three primitives with distinct materials, exported as GLB and OBJ.',
      ops: [
        { op: 'scene.reset' },
        { op: 'object.create', type: 'uv_sphere', name: 'Ball', location: [-1.6, 0, 1], radius: 0.7 },
        { op: 'object.create', type: 'cylinder', name: 'Post', location: [0, 0, 1], radius: 0.35, depth: 1.6 },
        { op: 'object.create', type: 'cone', name: 'Tip', location: [1.6, 0, 1], radius1: 0.6, depth: 1.2 },
        { op: 'material.create', name: 'Red', base_color: [0.8, 0.2, 0.2], roughness: 0.4 },
        { op: 'material.create', name: 'Green', base_color: [0.2, 0.7, 0.3], roughness: 0.4 },
        { op: 'material.create', name: 'Blue', base_color: [0.2, 0.3, 0.8], roughness: 0.4 },
        { op: 'material.assign', material: 'Red', target: 'Ball' },
        { op: 'material.assign', material: 'Green', target: 'Post' },
        { op: 'material.assign', material: 'Blue', target: 'Tip' },
        { op: 'export', prefix: 'trio', formats: ['glb', 'obj'] },
      ],
    },
  };

  const $ = (id) => document.getElementById(id);
  const state = { planKey: sessionStorage.getItem('bw.static.plan') || 'cube', mode: 'wire',
    fit: true, model: null, files: {}, logCount: 0, running: false };

  function log(kind, tag, msg) {
    const body = $('log');
    const row = document.createElement('div');
    row.className = 'ev';
    row.innerHTML = '<span class="t"></span><span class="k ' + kind + '"></span><span class="m"></span>';
    row.querySelector('.t').textContent = new Date().toLocaleTimeString();
    row.querySelector('.k').textContent = tag;
    row.querySelector('.m').textContent = msg;
    body.appendChild(row);
    body.scrollTop = body.scrollHeight;
    state.logCount += 1;
    $('logCount').textContent = state.logCount + ' event' + (state.logCount === 1 ? '' : 's');
  }

  function chip(id, text, status) {
    const el = $(id);
    el.textContent = text;
    el.dataset.s = status;
  }

  /* ------------------------------------------------------------ rendering */
  function drawPreview() {
    const canvas = $('preview');
    if (!state.model) {
      $('vpEmpty').style.display = '';
      canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height);
      $('vpLabel').textContent = 'no geometry';
      return;
    }
    $('vpEmpty').style.display = 'none';
    // fit must stay null so render() computes bounds; a truthy `fit` would be
    // used as the bounds object itself.
    const r = BWGLB.render(canvas, state.model, { mode: state.mode, fit: null });
    $('vpLabel').textContent = state.model.meshes.length + ' meshes · ' + r.drawn + ' edges';
  }

  function sizeCanvas() {
    const c = $('preview');
    const box = c.parentElement.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    c.width = Math.max(320, Math.floor(box.width * dpr));
    c.height = Math.max(240, Math.floor(box.height * dpr));
    c.style.width = '100%';
    c.style.height = '100%';
    drawPreview();
  }

  /* --------------------------------------------------------------- artefacts */
  function offerArtifacts() {
    const host = $('artifacts');
    host.innerHTML = '';
    const names = Object.keys(state.files);
    if (!names.length) { host.innerHTML = '<p class="hint">No artifact was produced.</p>'; return; }
    for (const name of names.sort()) {
      const bytes = state.files[name];
      const row = document.createElement('div');
      row.className = 'matrow';
      const dot = document.createElement('span');
      dot.className = 'dot';
      const label = document.createElement('span');
      label.textContent = name + ' · ' + bytes.length.toLocaleString() + ' B';
      row.append(dot, label);
      const blob = new Blob([bytes], { type: 'application/octet-stream' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = name;
      a.textContent = 'download';
      a.className = 'dl';
      row.appendChild(a);
      host.appendChild(row);
    }
  }

  /* -------------------------------------------------------------------- run */
  function buildScript(opsSource, planJson) {
    const b64 = (s) => btoa(unescape(encodeURIComponent(s)));
    return opsSource + '\n\nimport base64 as _b64\n' +
      'BW_PLAN_JSON = _b64.b64decode("' + b64(planJson) + '").decode()\n' +
      'BW_RESTORE_JSON = None\nmain()\n';
  }

  async function run() {
    if (state.running) return;
    state.running = true;
    $('runBtn').disabled = true;
    const key = state.planKey;
    const preset = PRESETS[key];
    const plan = { schema: 'bw.agent.ops/v2', tasks: [{ id: key, goal: preset.goal, ops: preset.ops }] };
    chip('chipEngine', 'booting engine…', 'warn');
    log('info', 'plan', key + ' · ' + preset.ops.length + ' ops');

    // Cross-origin isolation is a hard requirement for Blender's pthreads.
    if (!self.crossOriginIsolated) {
      chip('chipCoi', 'not isolated', 'err');
      log('err', 'coi', 'crossOriginIsolated is false; Blender cannot start. Reloading once so the service worker can take control.');
      sessionStorage.setItem('bw.static.autorun', '1');
      setTimeout(() => location.reload(), 900);
      return;
    }
    chip('chipCoi', 'isolated ✓ SAB', 'ok');

    let opsSource;
    try {
      const res = await fetch('bw_ops.py', { cache: 'no-store' });
      if (!res.ok) throw new Error('HTTP ' + res.status);
      opsSource = await res.text();
    } catch (e) {
      log('err', 'fetch', 'cannot load bw_ops.py: ' + e);
      state.running = false; $('runBtn').disabled = false;
      return;
    }

    const script = buildScript(opsSource, JSON.stringify(plan));
    const pending = {};   // name -> {chunks: []}
    let current = null;
    let result = null;

    const config = {
      arguments: ['--background', '--factory-startup', '--python-expr', script],
      canvas: (() => { const c = document.createElement('canvas'); c.width = 64; c.height = 64; return c; })(),
      locateFile: (p) => '/bin/' + p,
      print: (line) => {
        // Artifact bytes travel through the runtime's own print hook; the
        // DevTools console is deliberately not used for bulk data.
        if (line.startsWith('BW_FILE_BEGIN ')) {
          const parts = line.split(' ');
          current = parts[1];
          pending[current] = [];
          return;
        }
        if (line.startsWith('BW_CHUNK ')) { if (current) pending[current].push(line.slice(9)); return; }
        if (line.startsWith('BW_FILE_END ')) {
          if (current) {
            const bin = atob(pending[current].join(''));
            const bytes = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
            state.files[current] = bytes;
            log('ok', 'file', current + ' · ' + bytes.length.toLocaleString() + ' B');
            delete pending[current];
            current = null;
          }
          return;
        }
        if (line.startsWith('BW_RESULT ')) {
          try { result = JSON.parse(line.slice(10)); } catch (e) { log('err', 'result', 'unparsable: ' + e); }
          return;
        }
        if (line.startsWith('BW_DONE')) { onDone(); return; }
        if (line.startsWith('BW_STREAM_ERROR')) { log('err', 'stream', line.slice(16)); return; }
        log('info', 'py', line.slice(0, 400));
      },
      printErr: (line) => log('err', 'stderr', line.slice(0, 400)),
      preRun: [(mod) => {
        Object.assign(mod.ENV || (mod.ENV = {}), {
          BLENDER_SYSTEM_RESOURCES: '/bw', BLENDER_SYSTEM_PYTHON: '/bw/python',
          BLENDER_SYSTEM_SCRIPTS: '/bw/scripts', BLENDER_SYSTEM_DATAFILES: '/bw/datafiles',
        });
      }],
      onRuntimeInitialized: () => { chip('chipEngine', 'engine running', 'ok'); log('info', 'engine', 'runtime initialised'); },
      onAbort: (why) => { chip('chipEngine', 'aborted', 'err'); log('err', 'engine', 'ABORT ' + String(why).slice(0, 200)); if (!done) onDone(); },
      // onExit is a safety net: BW_DONE can be lost when the runtime tears down
      // its stdout at quit. Finalising here keeps the UI truthful either way.
      onExit: (code) => { chip('chipEngine', 'exited ' + code, code === 0 ? 'ok' : 'err'); log('info', 'engine', 'exit ' + code); if (!done) onDone(); },
    };

    let done = false;
    function onDone() {
      if (done) return;
      done = true;
      try { finalize(); } catch (e) {
        log('err', 'finalize', String(e && e.stack ? e.stack.split('\n')[0] : e).slice(0, 300));
        chip('chipEngine', 'result error', 'err');
      } finally {
        state.running = false;
        $('runBtn').disabled = false;
      }
    }

    function finalize() {
      log('info', 'engine', 'run finished · result ' + (result ? 'present' : 'MISSING'));
      $('runBtn').disabled = false;
      chip('chipEngine', 'finished', 'ok');
      // showResult() clears and fills the panel; do not clear it again below.
      if (result) showResult(result);
      else { $('result').innerHTML = ''; log('err', 'result', 'no structured result was returned'); }
      offerArtifacts();
      const glb = Object.keys(state.files).find((n) => n.endsWith('.glb'));
      if (glb) {
        try {
          state.model = BWGLB.parseGLB(state.files[glb].buffer.slice(0));
          log('ok', 'preview', glb + ' · ' + state.model.meshes.length + ' meshes · ' + state.model.generator);
          drawPreview();
        } catch (e) { log('err', 'preview', 'preview failed: ' + e); }
      }
      const dl = document.createElement('div');
      dl.className = 'laststep';
      dl.textContent = 'plan_ok=' + (result ? result.plan_ok : '?') + ' · ' + (result ? result.elapsed_s : '?') + ' s';
      $('result').appendChild(dl);
    }

    try {
      await createBlenderModule(config);
      // IMPORTANT: this promise resolves when the emscripten module is created,
      // NOT when Blender finishes. Blender keeps running (and printing) after
      // it. Completion is signalled by BW_DONE, with onExit as the safety net.
      log('info', 'engine', 'module handle resolved; awaiting BW_DONE / exit');
    } catch (e) {
      if (e && typeof e.status === 'number') {
        chip('chipEngine', 'exited ' + e.status, e.status === 0 ? 'ok' : 'err');
        if (!done) onDone();
      } else {
        chip('chipEngine', 'failed to boot', 'err');
        log('err', 'engine', String(e && e.message ? e.message : e).slice(0, 300));
        state.running = false; $('runBtn').disabled = false;
      }
    }
  }

  function showResult(r) {
    const host = $('result');
    host.innerHTML = '';
    const rows = [
      ['plan_ok', r.plan_ok ? 'true' : 'false'],
      ['elapsed', r.elapsed_s + ' s'],
      ['tasks', String((r.tasks || []).length)],
    ];
    for (const t of r.tasks || []) {
      const failed = (t.ops || []).filter((o) => o.ok === false);
      rows.push(['task ' + t.id, (failed.length ? failed.length + ' failed · ' : 'ok · ') + (t.ops || []).length + ' ops']);
      for (const e of t.errors || []) rows.push(['  error', e.code + ': ' + (e.message || '').slice(0, 90)]);
      for (const o of (t.ops || []).slice(-6)) {
        const res = o.result || {};
        let detail = '';
        if (o.op === 'object.create') detail = (res.name || '') + ' · ' + (res.verts || '?') + ' verts';
        else if (o.op === 'export') detail = Object.keys(res).join(', ');
        else if (o.op === 'validate.roundtrip') detail = Object.keys(res).map((k) => k + '=' + (res[k].ok ? 'ok' : 'fail')).join(' ');
        else if (res.verts !== undefined) detail = res.verts + ' verts';
        rows.push([o.op, detail || 'ok']);
      }
    }
    for (const [k, v] of rows) {
      const row = document.createElement('div');
      row.className = 'kv';
      const a = document.createElement('span'); a.textContent = k;
      const b = document.createElement('b'); b.textContent = v;
      row.append(a, b);
      host.appendChild(row);
    }
  }

  /* ------------------------------------------------------------------- wire */
  function wire() {
    const list = $('planList');
    for (const [key, p] of Object.entries(PRESETS)) {
      const b = document.createElement('button');
      b.className = 'obj' + (key === state.planKey ? ' sel' : '');
      b.dataset.plan = key;
      b.innerHTML = '<span class="dot"></span><span class="nm"></span><span class="v"></span>';
      b.querySelector('.nm').textContent = p.label;
      b.querySelector('.v').textContent = p.ops.length + ' ops';
      b.addEventListener('click', () => {
        state.planKey = key;
        sessionStorage.setItem('bw.static.plan', key);
        [...list.querySelectorAll('.obj')].forEach((n) => n.classList.toggle('sel', n.dataset.plan === key));
      });
      list.appendChild(b);
    }
    $('runBtn').addEventListener('click', run);
    document.querySelectorAll('.segbtn[data-mode]').forEach((b) => {
      b.addEventListener('click', () => {
        state.mode = b.dataset.mode;
        document.querySelectorAll('.segbtn[data-mode]').forEach((x) => x.classList.toggle('active', x === b));
        drawPreview();
      });
    });
    $('fitBtn').addEventListener('click', () => { state.fit = true; drawPreview(); });
    window.addEventListener('resize', sizeCanvas);
    window.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); run(); }
    });
    sizeCanvas();
    chip('chipCoi', self.crossOriginIsolated ? 'isolated ✓ SAB' : 'not isolated',
      self.crossOriginIsolated ? 'ok' : 'err');
  }

  wire();
  window.__bwStatic = { state, run, PRESETS };

  // Auto-run once after the isolation reload, so a first visit needs no clicks.
  if (sessionStorage.getItem('bw.static.autorun') === '1') {
    sessionStorage.removeItem('bw.static.autorun');
    setTimeout(run, 300);
  }
})();
