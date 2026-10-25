/* app.js — Blender Web workspace.
 *
 * The UI never invents scene state: everything shown comes from the last
 * execution record returned by the supervisor, which in turn comes from
 * Blender. The AI panel shows the exact request the orchestrating model
 * receives; plans are authored outside this page.
 */
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const state = {
    sessionId: null,
    state: null,       // last /api/state payload
    model: null,        // parsed GLB for the preview
    previewMode: 'wire',
    selected: null,
    busy: false,
    events: 0,
  };

  /* ----------------------------------------------------------- utilities */
  const fmtTime = () => new Date().toLocaleTimeString([], { hour12: false });

  function log(kind, title, message) {
    const box = $('log');
    const row = document.createElement('div');
    row.className = 'ev ' + kind;
    row.innerHTML = '<time>' + fmtTime() + '</time><span class="t"></span><span class="m"></span>';
    row.querySelector('.t').textContent = title;
    row.querySelector('.m').textContent = message || '';
    box.appendChild(row);
    box.scrollTop = box.scrollHeight;
    state.events++;
    $('logCount').textContent = state.events + ' events';
  }

  let toastTimer = null;
  function toast(msg, kind) {
    const el = $('toast');
    el.textContent = msg;
    el.className = 'toast show ' + (kind || '');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.className = 'toast ' + (kind || ''); }, 4200);
  }

  async function api(path, body, method) {
    const res = await fetch(path, {
      method: method || (body ? 'POST' : 'GET'),
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });
    let data = null;
    try { data = await res.json(); } catch (e) { data = { ok: false, code: 'BAD_RESPONSE', message: String(e) }; }
    return { status: res.status, data };
  }

  /* ------------------------------------------------------------ rendering */
  function sceneObjects() {
    const scene = state.state && state.state.current_scene;
    return scene ? scene.objects : [];
  }

  function renderHierarchy() {
    const box = $('hierarchy');
    const objs = sceneObjects();
    $('objCount').textContent = objs.length;
    if (!objs.length) {
      box.innerHTML = '<p class="empty">No objects.<br><span>Add a primitive or ask the assistant.</span></p>';
      return;
    }
    box.innerHTML = '';
    for (const o of objs) {
      const row = document.createElement('div');
      row.className = 'obj' + (state.selected === o.name ? ' sel' : '');
      row.dataset.type = o.type;
      row.tabIndex = 0;
      row.setAttribute('role', 'button');
      const meta = o.type === 'MESH' ? (o.verts + ' v') : '';
      row.innerHTML = '<span class="ic"></span><span class="nm"></span><span class="meta"></span>';
      row.querySelector('.ic').textContent = o.type === 'MESH' ? 'M' : (o.type === 'CAMERA' ? 'C' : 'L');
      row.querySelector('.nm').textContent = o.name;
      row.querySelector('.meta').textContent = meta;
      const select = () => { state.selected = o.name; renderHierarchy(); renderInspector(); };
      row.addEventListener('click', select);
      row.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); select(); } });
      box.appendChild(row);
    }
  }

  function renderMaterials() {
    const scene = state.state && state.state.current_scene;
    const mats = scene ? scene.materials : [];
    $('matCount').textContent = mats.length;
    const box = $('materials');
    if (!mats.length) { box.innerHTML = '<p class="empty">—</p>'; return; }
    box.innerHTML = '';
    for (const m of mats) {
      const row = document.createElement('div');
      row.className = 'matrow';
      const sw = document.createElement('span');
      sw.className = 'swatch';
      sw.style.background = colorFor(m);
      row.appendChild(sw);
      const nm = document.createElement('span');
      nm.textContent = m;
      row.appendChild(nm);
      box.appendChild(row);
    }
  }

  // deterministic swatch colour derived from the material name (display only)
  function colorFor(name) {
    let h = 0;
    for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) % 360;
    return 'hsl(' + h + ' 45% 52%)';
  }

  function renderInspector() {
    const box = $('inspector');
    const o = sceneObjects().find((x) => x.name === state.selected);
    if (!o) {
      box.innerHTML = '<p class="empty">Select an object.<br><span>Fields are read from Blender.</span></p>';
      return;
    }
    const rows = [
      ['Name', o.name],
      ['Type', o.type],
      ['ID', o.bw_id || '—'],
      ['Location', (o.location || []).map((v) => round(v)).join(', ')],
      ['Vertices', o.verts != null ? o.verts : '—'],
      ['Faces', o.faces != null ? o.faces : '—'],
      ['Materials', (o.materials || []).join(', ') || '—'],
    ];
    box.innerHTML = '<dl class="fields"></dl>';
    const dl = box.querySelector('dl');
    for (const [k, v] of rows) {
      const wrap = document.createElement('div');
      wrap.className = 'field';
      wrap.innerHTML = '<dt></dt><dd></dd>';
      wrap.querySelector('dt').textContent = k;
      wrap.querySelector('dd').textContent = v;
      dl.appendChild(wrap);
    }
    // quick material assignment for meshes
    const scene = state.state && state.state.current_scene;
    if (o.type === 'MESH' && scene && scene.materials.length) {
      const sel = document.createElement('select');
      sel.className = 'btn';
      sel.style.width = 'calc(100% - 16px)';
      sel.style.margin = '8px';
      sel.innerHTML = '<option value="">Assign material…</option>' +
        scene.materials.map((m) => '<option>' + escapeHtml(m) + '</option>').join('');
      sel.addEventListener('change', () => {
        if (!sel.value) return;
        submitOps([{ op: 'material.assign', material: sel.value, target: o.name }], 'assign ' + sel.value);
      });
      box.appendChild(sel);
    }
  }

  const round = (v) => (typeof v === 'number' ? Math.round(v * 1000) / 1000 : v);
  const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  function renderModelState() {
    if (!state.state) { $('modelState').textContent = '—'; return; }
    const s = state.state;
    const brief = {
      session_id: s.session_id,
      task: s.task,
      state: s.state,
      decided_by: s.decided_by,
      budget: s.budget,
      current_scene: s.current_scene
        ? {
          objects: (s.current_scene.objects || []).map((o) => ({
            name: o.name, type: o.type, verts: o.verts, materials: o.materials,
          })),
          materials: s.current_scene.materials,
        }
        : null,
      last_execution: s.last_execution
        ? {
          step: s.last_execution.step,
          ok: s.last_execution.ok,
          errors: s.last_execution.errors,
          ops: (s.last_execution.ops || []).map((o) => ({ op: o.op, ok: o.ok })),
          artifacts: Object.keys(s.last_execution.artifacts || {}),
        }
        : null,
    };
    $('modelState').textContent = JSON.stringify(brief, null, 2);
  }

  function renderAll() {
    renderHierarchy();
    renderMaterials();
    renderInspector();
    renderModelState();
    updateChips();
  }

  function updateChips() {
    const s = state.state;
    $('chipEngine').textContent = s && s.runtime ? 'Blender wasm' : 'Blender —';
    $('chipSchema').textContent = 'schema ' + (window.__capabilities ? window.__capabilities.schema : '—');
    const rt = $('chipRuntime');
    if (!s || !s.runtime) { rt.className = 'chip'; rt.textContent = 'runtime'; return; }
    const ready = Object.values(s.runtime).every(Boolean);
    rt.className = 'chip ' + (ready ? 'chip-ok' : 'chip-bad');
    rt.textContent = ready ? 'runtime ready' : 'runtime incomplete';
  }

  /* ------------------------------------------------------------- preview */
  function drawPreview() {
    const canvas = $('preview');
    const empty = $('vpEmpty');
    if (!state.model || !state.model.meshes.length) {
      empty.style.display = '';
      canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height);
      $('vpLabel').textContent = 'no geometry';
      return;
    }
    empty.style.display = 'none';
    const r = BWGLB.render(canvas, state.model, { mode: state.previewMode, fit: state.fit });
    const n = state.model.meshes.length;
    $('vpLabel').textContent = n + ' mesh' + (n === 1 ? '' : 'es') + ' · ' + r.drawn + ' edges';
  }

  // Load the newest verified GLB from the session and preview it.
  async function loadPreview() {
    if (!state.sessionId) { drawPreview(); return; }
    const steps = (state.state && state.state.history) || [];
    let name = null;
    for (let i = steps.length - 1; i >= 0 && !name; i--) {
      const arts = steps[i].artifacts || [];
      name = arts.find((a) => a.endsWith('.glb')) || null;
    }
    if (!name) { state.model = null; drawPreview(); return; }
    try {
      const url = '/api/artifact?session=' + encodeURIComponent(state.sessionId) + '&name=' + encodeURIComponent(name);
      const buf = await (await fetch(url)).arrayBuffer();
      state.model = BWGLB.parseGLB(buf);
      state.fit = null;
      log('ok', 'preview', name + ' · ' + state.model.meshes.length + ' meshes · ' + state.model.generator);
      drawPreview();
    } catch (e) {
      state.model = null;
      log('err', 'preview', String(e));
      drawPreview();
    }
  }

  /* --------------------------------------------------------------- plans */
  function opPlan(ops, goal) {
    return {
      schema: 'bw.agent.ops/v2',
      tasks: [{ id: 'ui-' + Date.now().toString(36), goal: goal || 'ui action', ops: ops }],
    };
  }

  async function submitPlan(plan, label) {
    if (!state.sessionId) { toast('Start a session first', 'err'); return null; }
    if (state.busy) { toast('A plan is already executing', 'err'); return null; }
    state.busy = true;
    setBusy(true);
    log('warn', 'plan', (label || 'plan') + ' → ' + plan.tasks[0].ops.length + ' op(s); booting Blender…');
    const t0 = performance.now();
    const { status, data } = await api('/api/plan', { session: state.sessionId, plan });
    const ms = Math.round(performance.now() - t0);
    state.busy = false;
    setBusy(false);

    if (!data || data.ok === undefined) {
      log('err', 'error', (data && data.message) || 'no response');
      toast('Execution failed', 'err');
      return null;
    }
    for (const o of data.ops || []) {
      if (o.ok) log('ok', o.op, 'ok' + (o.result ? '' : ''));
      else log('err', o.op, (o.error && o.error.code) + ': ' + (o.error && o.error.message));
    }
    for (const [name, meta] of Object.entries(data.artifacts || {})) {
      const v = meta.independent_verification;
      log('ok', 'artifact', name + ' · ' + meta.bytes + ' B · independent ' + (v && v.ok ? 'verified' : 'unverified'));
    }
    window.__bwLastStep = { ok: data.ok, ops: data.ops, errors: data.errors, artifacts: data.artifacts };
    log(data.ok ? 'ok' : 'err', data.ok ? 'done' : 'failed', ms + ' ms · ' + (data.errors || []).length + ' error(s)');
    toast(data.ok ? 'Executed in ' + ms + ' ms' : 'Finished with errors', data.ok ? 'ok' : 'err');
    await refreshState();
    return data;
  }

  async function submitOps(ops, label) {
    return submitPlan(opPlan(ops, label), label);
  }

  function setBusy(on) {
    document.querySelectorAll('.btn-primary, .btn-tile').forEach((b) => { b.disabled = on; });
    document.body.style.cursor = on ? 'progress' : '';
  }

  /* ---------------------------------------------------------------- state */
  async function refreshState() {
    if (!state.sessionId) return;
    const { data } = await api('/api/state?session=' + encodeURIComponent(state.sessionId));
    if (!data || !data.ok) { toast('Could not read state', 'err'); return; }
    state.state = data.state;
    publishState();
    renderAll();
    await loadPreview();
  }

  async function startSession(task) {
    const t = (task || $('taskInput').value || '').trim();
    if (!t) { toast('Describe the task first', 'err'); return; }
    log('warn', 'session', 'starting…');
    const { data } = await api('/api/session', { task: t });
    if (!data || !data.ok) { toast('Could not start session', 'err'); return; }
    state.sessionId = data.session_id;
    state.state = data.state;
    state.model = null;
    state.fit = null;
    state.selected = null;
    log('ok', 'session', data.session_id + ' · ' + t.slice(0, 60));
    renderAll();
    drawPreview();
    // a fresh session starts from Blender's factory scene
    await submitOps([{ op: 'scene.inspect' }], 'read initial scene');
  }

  /* --------------------------------------------------------------- events */
  function wire() {
    $('btnNewSession').addEventListener('click', () => startSession());
    $('btnTask').addEventListener('click', () => startSession());
    $('btnRefresh').addEventListener('click', refreshState);
    $('btnExport').addEventListener('click', () => exportScene());
    $('btnFrame').addEventListener('click', () => { state.fit = null; drawPreview(); });
    $('btnCopyState').addEventListener('click', async () => {
      try { await navigator.clipboard.writeText($('modelState').textContent); toast('Copied model state', 'ok'); }
      catch (e) { toast('Clipboard blocked', 'err'); }
    });
    $('btnClearLog').addEventListener('click', () => { $('log').innerHTML = ''; state.events = 0; $('logCount').textContent = '0 events'; });
    $('btnToggleLog').addEventListener('click', (e) => {
      const c = $('console');
      c.classList.toggle('collapsed');
      const collapsed = c.classList.contains('collapsed');
      e.target.textContent = collapsed ? 'Expand' : 'Collapse';
      e.target.setAttribute('aria-expanded', String(!collapsed));
    });
    document.querySelectorAll('.seg-btn').forEach((b) => {
      b.addEventListener('click', () => {
        document.querySelectorAll('.seg-btn').forEach((x) => x.classList.remove('active'));
        b.classList.add('active');
        state.previewMode = b.dataset.mode;
        drawPreview();
      });
    });
    document.querySelectorAll('[data-add]').forEach((b) => {
      b.addEventListener('click', () => {
        const kind = b.dataset.add;
        const n = sceneObjects().filter((o) => o.name.startsWith(capitalize(kind))).length + 1;
        submitOps([{ op: 'object.create', type: kind, name: capitalize(kind) + '_' + n, size: 2.0 }],
          'add ' + kind);
      });
    });
    document.querySelectorAll('a[href="#"]').forEach((a) => {
      a.addEventListener('click', (e) => {
        e.preventDefault();
        toast('See docs/VIEWPORT.md and docs/ARCHITECTURE.md in the repository');
      });
    });
    window.addEventListener('resize', drawPreview);
    $('taskInput').addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) startSession();
    });
  }

  const capitalize = (s) => s.charAt(0).toUpperCase() + s.slice(1);

  async function exportScene() {
    if (!state.sessionId) { toast('Start a session first', 'err'); return; }
    const formats = ['glb', 'obj', 'stl'];
    await submitPlan({
      schema: 'bw.agent.ops/v2',
      tasks: [{ id: 'export-' + Date.now().toString(36), goal: 'export', ops: [
        { op: 'export', prefix: 'scene', formats },
        { op: 'validate.roundtrip', prefix: 'scene', formats },
      ] }],
    }, 'export');
  }

  /* ------------------------------------------------------------- the API */
  // A small, documented surface so an external driver (e.g. Obscura --eval)
  // can operate the app without scraping the DOM.
  // Obscura's --eval serialises promises as {} (it does not awaitPromise), so
  // we publish synchronous mirrors. A driver reads window.__bwState after the
  // page has settled instead of awaiting an async call.
  window.__bwState = null;
  window.__bwLastStep = null;
  window.__bwEvents = [];

  function publishState() {
    window.__bwState = state.state;
    window.__bwSession = state.sessionId;
    window.__bwObjects = sceneObjects();
  }

  window.BW = {
    version: '1.0',
    ready: false,
    // Fire a plan without awaiting it. `keepalive` keeps the request alive
    // through page teardown, so a non-awaiting driver can submit and exit.
    submitNoWait(plan) {
      if (!state.sessionId) return 'ERR_NO_SESSION';
      const res = fetch('/api/plan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session: state.sessionId, plan }),
        keepalive: true,
      });
      if (res && res.catch) res.catch(() => {});
      return 'SUBMITTED ' + state.sessionId;
    },
    submitOpsNoWait(ops) {
      return this.submitNoWait(opPlan(ops, 'external'));
    },
    state: () => state.state,
    session: () => state.sessionId,
    scene: () => (state.state && state.state.current_scene) || null,
    objects: () => sceneObjects(),
    async startSession(task) { await startSession(task); return state.sessionId; },
    async submit(plan) { return submitPlan(plan, 'external'); },
    async submitOps(ops) { return submitOps(ops, 'external'); },
    async refresh() { await refreshState(); return state.state; },
    async exportScene() { await exportScene(); return (state.state && state.state.history) || []; },
    log,
  };

  /* ---------------------------------------------------------------- boot */
  (async function boot() {
    wire();
    drawPreview();
    // A driver can revisit /?session=<id> to read state without creating one.
    const attach = new URLSearchParams(location.search).get('session');
    if (attach) {
      state.sessionId = attach;
      log('warn', 'session', 'attached to ' + attach);
      await refreshState();
    }
    log('ok', 'app', 'workspace ready');
    try {
      const { data } = await api('/api/health');
      window.__capabilities = data;
      const ok = data && data.ok;
      $('runtimeLine').textContent = ok
        ? 'Blender ' + (data.blender || 'wasm') + ' · Chromium-backed'
        : 'runtime prerequisites missing — see log';
      const rt = $('chipRuntime');
      rt.className = 'chip ' + (ok ? 'chip-ok' : 'chip-bad');
      rt.textContent = ok ? 'runtime ready' : 'runtime incomplete';
      $('chipSchema').textContent = 'schema ' + (data.schema || '—');
      if (!ok) {
        log('err', 'runtime', 'missing: ' + Object.entries(data.preflight || {})
          .filter(([, v]) => !v.ok).map(([k]) => k).join(', '));
      }
      window.BW.ready = true;
    } catch (e) {
      log('err', 'health', String(e));
    }
  })();
})();
