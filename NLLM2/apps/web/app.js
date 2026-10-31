// Forge web client — no framework, no build step. Talks only to /api on the same origin.

const api = {
  async call(path, { method = "GET", body, raw } = {}) {
    const init = { method, headers: {} };
    if (body !== undefined) {
      if (raw) {
        init.body = body;
        init.headers["Content-Type"] = "application/octet-stream";
      } else {
        init.body = JSON.stringify(body);
        init.headers["Content-Type"] = "application/json";
      }
    }
    const res = await fetch("/api" + path, init);
    const text = await res.text();
    let data = null;
    try { data = text ? JSON.parse(text) : null; } catch { data = { error: text }; }
    if (!res.ok) {
      const msg = (data && (data.error || data.message)) || `HTTP ${res.status}`;
      const err = new Error(msg);
      err.code = data && data.code;
      err.status = res.status;
      throw err;
    }
    return data;
  },
  get: (p) => api.call(p),
  post: (p, body) => api.call(p, { method: "POST", body }),
  patch: (p, body) => api.call(p, { method: "PATCH", body }),
  del: (p) => api.call(p, { method: "DELETE" }),
};

// ---------------------------------------------------------------- state
const state = {
  caps: null,
  notebooks: [],
  nb: null,            // full notebook object
  selected: new Set(), // source ids chosen for chat/artifacts
  tab: "notes",
  messages: [],
  conversationId: null,
  artifact: null,
  busy: false,
};

// ---------------------------------------------------------------- dom helpers
function h(tag, props = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "html") el.innerHTML = v; // only for trusted, self-generated SVG
    else if (k === "text") el.textContent = v;
    else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === "dataset") Object.assign(el.dataset, v);
    else if (k === "style") el.setAttribute("style", v);
    else if (v === true) el.setAttribute(k, "");
    else el.setAttribute(k, v);
  }
  for (const kid of kids.flat(9)) {
    if (kid === null || kid === undefined || kid === false) continue;
    el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  }
  return el;
}

let toastTimer;
function toast(message, kind = "") {
  const t = document.getElementById("toast");
  t.textContent = message;
  t.className = "toast " + kind;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, kind === "err" ? 7000 : 3200);
}

function spinner(label = "Working") {
  return h("span", { class: "row-between", style: "gap:8px;display:inline-flex" },
    h("span", { class: "spinner" }), h("span", { class: "hint" }, label + "…"));
}

const fmtDate = (iso) => {
  if (!iso) return "";
  const d = new Date(iso);
  return isNaN(d) ? iso : d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
};

// ---------------------------------------------------------------- router
function parseHash() {
  const raw = location.hash.replace(/^#\/?/, "");
  const parts = raw.split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] === "capabilities") return { view: "capabilities" };
  if (parts[0] === "nb" && parts[1]) return { view: "notebook", id: parts[1], artifact: parts[3] || null };
  return { view: "library" };
}

async function route() {
  const r = parseHash();
  const app = document.getElementById("app");
  app.replaceChildren(h("div", { class: "library" }, h("p", { class: "empty" }, "Loading…")));
  try {
    if (r.view === "capabilities") return renderCapabilities(app);
    if (r.view === "notebook") return await openNotebook(app, r.id, r.artifact);
    await renderLibrary(app);
  } catch (e) {
    app.replaceChildren(h("div", { class: "library" },
      h("h1", {}, "Something went wrong"),
      h("p", { class: "empty" }, e.message),
      h("button", { class: "primary", onclick: () => route() }, "Retry")));
  }
}
window.addEventListener("hashchange", route);

// ---------------------------------------------------------------- library
async function renderLibrary(app) {
  const data = await api.get("/notebooks");
  state.notebooks = data.notebooks || [];
  const list = h("div", { class: "grid" });

  for (const nb of state.notebooks) {
    list.append(h("div", {
      class: "card", tabindex: "0", role: "link",
      onclick: () => (location.hash = `#/nb/${nb.id}`),
      onkeydown: (e) => { if (e.key === "Enter") location.hash = `#/nb/${nb.id}`; },
    },
      h("h3", {}, nb.title),
      nb.description ? h("p", {}, nb.description) : null,
      h("div", { class: "meta" },
        h("span", {}, `${nb.source_count} source${nb.source_count === 1 ? "" : "s"}`),
        h("span", {}, `${nb.note_count} note${nb.note_count === 1 ? "" : "s"}`),
        h("span", {}, `${nb.artifact_count} artifact${nb.artifact_count === 1 ? "" : "s"}`),
        h("span", {}, fmtDate(nb.updated_at))),
      h("div", { class: "row-between", style: "margin-top:6px" },
        h("span", { class: "badge" + (nb.archived ? " warn" : "") }, nb.archived ? "archived" : "active"),
        h("button", {
          class: "ghost", onclick: async (e) => {
            e.stopPropagation();
            if (!confirm(`Delete “${nb.title}” and all of its sources, notes and artifacts?`)) return;
            await api.del(`/notebooks/${nb.id}`);
            toast("Notebook deleted");
            route();
          }
        }, "Delete"))));
  }

  const createForm = h("form", {
    class: "stack",
    onsubmit: async (e) => {
      e.preventDefault();
      const title = createForm.querySelector('[name="title"]').value.trim();
      if (!title) return toast("Give the notebook a name", "err");
      try {
        const nb = await api.post("/notebooks", {
          title,
          description: createForm.querySelector('[name="description"]').value.trim(),
        });
        toast("Notebook created");
        location.hash = `#/nb/${nb.id}`;
      } catch (err) { toast(err.message, "err"); }
    },
  },
    h("input", { type: "text", name: "title", placeholder: "e.g. Retrieval research", "aria-label": "Notebook title" }),
    h("input", { type: "text", name: "description", placeholder: "Optional description", "aria-label": "Description" }),
    h("button", { class: "primary", type: "submit" }, "Create notebook"));

  list.append(h("div", { class: "card new-card", onclick: () => createForm.querySelector("input").focus() }, createForm));

  app.replaceChildren(h("div", { class: "library" },
    h("h1", {}, "Notebooks"),
    h("p", { class: "sub" }, "Each notebook is a private corpus. Import sources, ask questions, and every answer points back to the exact evidence."),
    list,
    h("p", { class: "hint", style: "margin-top:20px" }, "All data stays on this machine, in the server's data directory.")));
}

// ---------------------------------------------------------------- capabilities
async function renderCapabilities(app) {
  const caps = state.caps || (state.caps = await api.get("/capabilities"));
  const badgeFor = (v) => h("span", { class: "badge " + (v === "supported" ? "ok" : v === "blocked" ? "warn" : "err") }, v);
  const groups = [];
  for (const [group, value] of Object.entries(caps)) {
    if (typeof value !== "object" || value === null || Array.isArray(value)) continue;
    const KNOWN = ["supported", "blocked", "unsupported", "experimental", "partial", "not_configured"];
    const rows = Object.entries(value).map(([k, v]) => {
      // A row may carry a status object, a plain status word, or an informational
      // value (e.g. web_fetch.transport = "curl"). Only real statuses get a badge,
      // so an informational row can never be mistaken for a capability claim.
      const raw = typeof v === "string" ? v : v.status || "supported";
      const isStatus = KNOWN.includes(String(raw));
      const detail = v && typeof v === "object"
        ? [v.reason, v.note, v.locator ? `locator: ${v.locator}` : "", v.engine ? `engine: ${v.engine}` : ""]
            .filter(Boolean).join(" · ")
        : (isStatus ? "" : String(raw));
      return h("tr", {},
        h("td", {}, k.replace(/_/g, " ")),
        h("td", {}, isStatus ? badgeFor(String(raw)) : h("span", { class: "badge" }, "info")),
        h("td", { class: "hint" }, detail));
    });
    groups.push(h("section", {},
      h("h2", { style: "font-size:15px;margin:22px 0 8px;text-transform:capitalize" }, group),
      h("table", { style: "width:100%;border-collapse:collapse" },
        h("tbody", {}, rows))));
  }
  app.replaceChildren(h("div", { class: "library" },
    h("h1", {}, "Capabilities"),
    h("p", { class: "sub" }, `Forge ${caps.version}. This table is generated by the server, so it always describes what is actually implemented — unsupported features say why instead of pretending.`),
    groups,
    await providerPanel(),
    await notebookLmPanel(),
    await binaryPanel(),
    h("button", { class: "ghost", style: "margin-top:24px", onclick: () => (location.hash = "#/") }, "← Back to notebooks")));
}

async function providerPanel() {
  const wrap = h("section", { style: "margin-top:26px" });
  const list = h("div", { class: "stack" });
  wrap.append(h("h2", { style: "font-size:15px;margin:0 0 8px" }, "Model providers (BYOK)"));
  wrap.append(h("p", { class: "hint" },
    "Keys are stored on this machine with owner-only permissions and are never returned by the API. This build cannot make HTTPS requests, so a configured provider is reported as unavailable and the local extractive engine keeps answering."));

  const render = async () => {
    try {
      const data = await api.get("/providers");
      list.replaceChildren();
      if (!data.providers.length) list.append(h("p", { class: "empty" }, "No provider configured."));
      for (const p of data.providers) {
        list.append(h("div", { class: "note-item" },
          h("div", { class: "row-between" },
            h("strong", {}, `${p.kind} · ${p.model}`),
            h("span", { class: "badge " + (p.available ? "ok" : "warn") }, p.available ? "available" : "unavailable")),
          h("div", { class: "hint" }, p.base_url),
          h("div", { class: "hint" }, `key ${p.key_set ? "stored" : "not set"} · ${p.reason}`),
          h("button", { class: "ghost danger", style: "margin-top:6px", onclick: async () => {
            await api.del(`/providers/${p.id}`); toast("Provider removed"); render();
          } }, "remove")));
      }
    } catch (e) { list.replaceChildren(h("p", { class: "empty" }, e.message)); }
  };

  const base = h("input", { type: "text", placeholder: "https://api.example.com/v1", "aria-label": "Provider base URL" });
  const model = h("input", { type: "text", placeholder: "model name", "aria-label": "Model name" });
  const key = h("input", { type: "password", placeholder: "API key (write-only)", "aria-label": "API key" });
  wrap.append(
    h("div", { class: "stack", style: "margin:10px 0" }, base, model, key,
      h("button", { class: "primary", onclick: async () => {
        try {
          await api.post("/providers", { kind: "openai_compatible", base_url: base.value, model: model.value, api_key: key.value });
          key.value = "";
          toast("Provider saved");
          render();
        } catch (err) { toast(err.message, "err"); }
      } }, "Save provider")),
    list);
  await render();
  return wrap;
}

async function notebookLmPanel() {
  const wrap = h("section", { style: "margin-top:26px" });
  wrap.append(h("h2", { style: "font-size:15px;margin:0 0 8px" }, "Gemini Notebook / NotebookLM bridge"));
  wrap.append(h("p", { class: "hint" },
    "The service's web protocol was reconstructed from public reverse-engineering evidence (pinned commit, sha256 recorded) and is implemented here in Rust. This build cannot reach Google, so nothing is ever sent — but you can inspect the exact request it would send."));
  let status;
  try { status = await api.get("/notebooklm/status"); } catch (e) { status = null; }

  if (status) {
    wrap.append(h("div", { class: "note-item" },
      h("div", { class: "row-between" },
        h("strong", {}, status.service),
        h("span", { class: "badge warn" }, "protocol implemented · target unreachable")),
      h("div", { class: "hint" }, status.reason),
      h("div", { class: "hint", style: "margin-top:6px" },
        `${status.rpc_count} RPC methods recovered · ${status.artifact_types.length} artifact types`),
      h("div", { class: "hint" }, status.provenance)));
  }

  const out = h("div", { class: "stack" });
  const actions = [
    ["add_url", "add_url", "Add URL source"],
    ["add_text", "add_text", "Add pasted text"],
    ["add_youtube", "add_youtube", "Add YouTube"],
    ["artifact", "artifact", "Generate audio overview"],
    ["ask", "ask", "Ask a question"],
  ];
  const row = h("div", { class: "row-between", style: "flex-wrap:wrap;gap:6px;margin:10px 0" });
  for (const [action, , label] of actions) {
    row.append(h("button", { onclick: async () => {
      try {
        const r = await api.post("/notebooklm/preview", {
          action,
          notebook_id: "demo-notebook-id",
          url: "https://example.com/article",
          title: "Pasted note",
          text: "Evidence text",
          kind: "audio",
          question: "What do the sources say?",
          source_ids: ["demo-source-id"],
        });
        out.replaceChildren(
          h("div", { class: "hint" }, `${r.rpc}  ·  ${r.method}  ·  sent: ${r.sent}`),
          h("pre", { style: "font-size:11.5px;overflow:auto;white-space:pre-wrap" }, r.url + "\n\n" + r.body),
          h("p", { class: "hint" }, r.why_not_sent || ""));
      } catch (e) { toast(e.message, "err"); }
    } }, label));
  }
  wrap.append(row, out);
  return wrap;
}

// ---------------------------------------------------------------- notebook
async function openNotebook(app, id, artifactId) {
  const nb = await api.get(`/notebooks/${id}`);
  state.nb = nb;
  state.selected = new Set((nb.sources || []).filter((s) => s.status === "ready").map((s) => s.id));

  const workspace = h("div", { class: "workspace" });

  // --- left: sources
  const sourcesPane = h("div", { class: "pane" });
  const chatPane = h("div", { class: "pane", style: "padding:0;display:flex;flex-direction:column" });
  const sidePane = h("div", { class: "pane" });
  workspace.append(sourcesPane, chatPane, sidePane);
  app.replaceChildren(workspace);

  renderSources(sourcesPane);
  renderChat(chatPane);
  renderSide(sidePane);
  if (artifactId) showArtifact(artifactId);
}

function renderSources(pane) {
  const nb = state.nb;
  pane.replaceChildren();

  const titleEl = h("input", {
    type: "text", value: nb.title, "aria-label": "Notebook title",
    onchange: async (e) => {
      const t = e.target.value.trim();
      if (!t) return;
      await api.patch(`/notebooks/${nb.id}`, { title: t });
      nb.title = t;
      toast("Renamed");
    },
  });
  const instr = h("textarea", {
    placeholder: "Optional instructions, e.g. “Answer for a beginner; prefer definitions.”",
    "aria-label": "Notebook instructions",
  }, nb.instructions || "");
  const instrBox = h("div", { class: "stack", hidden: !nb.instructions },
    instr,
    h("button", {
      class: "ghost", onclick: async () => {
        await api.patch(`/notebooks/${nb.id}`, { instructions: instr.value });
        nb.instructions = instr.value;
        toast("Instructions saved");
        instrBox.hidden = true;
      }
    }, "Save instructions"));

  pane.append(
    h("div", { class: "row-between" },
      titleEl,
      h("button", { class: "ghost", title: "Delete notebook", onclick: async () => {
        if (!confirm("Delete this notebook?")) return;
        await api.del(`/notebooks/${nb.id}`); location.hash = "#/";
      } }, "🗑")),
    h("div", { class: "row-between", style: "margin-top:10px" },
      h("span", { class: "hint" }, "Instructions"),
      h("button", { class: "ghost", onclick: () => (instrBox.hidden = !instrBox.hidden) }, "edit")),
    instrBox,
    h("h2", {}, `Sources (${(nb.sources || []).length})`));

  const importBox = h("div", { class: "stack" });
  importBox.append(
    h("div", { class: "row-between" },
      h("span", { class: "hint" }, "Add a source"),
      h("button", { class: "ghost", onclick: () => (importBox.hidden = true) }, "hide")),
    h("input", { type: "text", id: "pasteTitle", placeholder: "Title (optional)", "aria-label": "Source title" }),
    h("textarea", { id: "pasteText", placeholder: "Paste text, markdown, CSV…", "aria-label": "Paste source text" }),
    h("button", { class: "primary", onclick: async () => {
      const text = document.getElementById("pasteText").value;
      const title = document.getElementById("pasteTitle").value.trim();
      if (!text.trim()) return toast("Nothing to import", "err");
      try {
        const r = await api.post(`/notebooks/${nb.id}/sources/text`, { text, title });
        toast(`Imported ${r.chunk_count} passages`);
        await openNotebook(document.getElementById("app"), nb.id);
      } catch (e) { toast(e.message, "err"); }
    } }, "Import text"));

  const fileInput = h("input", {
    type: "file", accept: ".pdf,.docx,.pptx,.xlsx,.md,.markdown,.txt,.html,.htm,.csv,.tsv", class: "sr-only", id: "fileInput",
    onchange: async (e) => {
      const f = e.target.files[0];
      if (!f) return;
      const btn = pane.querySelector("#uploadBtn");
      btn.disabled = true; btn.textContent = "Uploading…";
      try {
        const buf = await f.arrayBuffer();
        const r = await api.call(`/notebooks/${nb.id}/sources/upload?filename=${encodeURIComponent(f.name)}`, { method: "POST", body: buf, raw: true });
        toast(`Imported ${r.chunk_count} passages from ${f.name}`);
        await openNotebook(document.getElementById("app"), nb.id);
      } catch (err) {
        toast(err.message, "err");
        btn.disabled = false; btn.textContent = "Upload file";
      }
    },
  });

  const uploadRow = h("div", { class: "stack", style: "margin-top:9px" },
    fileInput,
    h("label", { for: "fileInput", class: "sr-only" }, "Upload a file"),
    h("button", { id: "uploadBtn", onclick: () => fileInput.click() }, "Upload file (PDF, DOCX, PPTX, XLSX, MD, TXT, HTML, CSV)"),
    h("button", { class: "ghost", id: "addUrlBtn", onclick: () => promptUrl() }, "Add from URL…"));

  const urlRow = h("div", { class: "stack", style: "margin-top:9px", hidden: true });
  const urlInput = h("input", { type: "url", id: "urlInput", placeholder: "https://…", "aria-label": "Web page URL" });
  const urlGo = h("button", { class: "primary", onclick: async () => {
    const url = urlInput.value.trim();
    if (!url) return;
    urlGo.disabled = true; urlGo.textContent = "Fetching…";
    try {
      const r = await api.post(`/notebooks/${nb.id}/sources/url`, { url });
      toast(`Imported “${r.title}” (${r.chunk_count} passages)`);
      await openNotebook(document.getElementById("app"), nb.id);
    } catch (e) {
      toast(e.message, "err");
      urlGo.disabled = false; urlGo.textContent = "Fetch page";
    }
  } }, "Fetch page");
  urlRow.append(
    h("p", { class: "hint" }, "Forge fetches the page itself, keeps its title and canonical URL, and indexes the readable text. Duplicate URLs are refused."),
    urlInput, urlGo);
  importBox.append(urlRow);

  function promptUrl() {
    urlRow.hidden = false;
    urlInput.focus();
  }

  importBox.append(uploadRow);
  pane.append(importBox);

  for (const s of nb.sources || []) {
    const item = h("div", {
      class: "source-item" + (state.selected.has(s.id) ? " sel" : "") + (s.status === "failed" ? " failed" : ""),
    },
      h("div", { class: "row" },
        h("input", {
          type: "checkbox", checked: state.selected.has(s.id), "aria-label": `Use ${s.title}`,
          disabled: s.status !== "ready",
          onchange: (e) => {
            if (e.target.checked) state.selected.add(s.id); else state.selected.delete(s.id);
            item.classList.toggle("sel", e.target.checked);
            renderChat(document.querySelectorAll(".pane")[1], true);
          },
        }),
        h("span", { class: "name", title: s.title, onclick: () => openViewer(s.id) }, s.title),
        h("button", { class: "ghost", title: "Source guide (quoted, not generated)", onclick: () => openViewer(s.id) }, "guide"),
        h("button", { class: "ghost", title: "Remove source", onclick: async () => {
          if (!confirm(`Remove “${s.title}”?`)) return;
          await api.del(`/sources/${s.id}`);
          toast("Source removed");
          await openNotebook(document.getElementById("app"), nb.id);
        } }, "×")),
      h("div", { class: "sub" },
        `${s.kind} · ${s.chunk_count} passages${s.page_count ? ` · ${s.page_count} pages` : ""}${s.status !== "ready" ? ` · ${s.status}` : ""}`,
        s.table_count ? h("span", { title: (s.table_sheets || []).map((t) => `${t.sheet}: ${t.row_count} rows`).join("\n") },
          ` · ${s.table_count} sheet${s.table_count === 1 ? "" : "s"} · ${s.table_row_count} rows`) : null,
        s.origin === "url" ? h("span", { title: s.canonical_url }, " · from URL") : null,
        (s.fetched_at || "").startsWith("20") ? h("span", { title: s.fetched_at }, " · synced") : null),
      (s.labels || []).length
        ? h("div", { class: "sub" }, s.labels.map((l) => h("span", { class: "badge" }, l)))
        : null,
      s.origin === "url"
        ? h("div", { class: "row", style: "gap:6px;margin-top:4px" },
            h("button", { class: "ghost", onclick: async (e) => {
              const btn = e.target;
              btn.disabled = true; btn.textContent = "Syncing…";
              try {
                const r = await api.post(`/sources/${s.id}/refresh`, {});
                toast(r.changed ? `Re-indexed: ${r.chunk_count} passages` : "No change since last sync");
                await openNotebook(document.getElementById("app"), nb.id);
              } catch (err) {
                toast(err.message, "err"); btn.disabled = false; btn.textContent = "Sync now";
              }
            } }, "Sync now"))
        : null,
      s.error ? h("div", { class: "sub", style: "color:var(--err)" }, s.error) : null,
      (s.warnings || []).length ? h("div", { class: "sub", style: "color:var(--warn)" }, s.warnings.join(" · ")) : null);
    pane.append(item);
  }

  if (!(nb.sources || []).length) {
    pane.append(h("p", { class: "empty" }, "No sources yet. Paste text or upload a file to give this notebook something to read."));
  }
}

// ---------------------------------------------------------------- chat
function renderChat(pane, onlyComposer = false) {
  const nb = state.nb;
  if (!pane) return;
  const scroll = h("div", { class: "messages", id: "messages" });

  for (const m of state.messages) scroll.append(messageEl(m));

  if (!state.messages.length) {
    scroll.append(h("div", { class: "msg assistant" },
      h("div", { class: "who" }, "Forge"),
      h("div", { class: "body" }, "Ask a question about the sources in this notebook. Answers are extracted from your sources and every claim carries a citation you can open."),
      h("p", { class: "hint", style: "margin-top:10px" }, `Using ${state.selected.size} of ${(nb.sources || []).filter(s => s.status === "ready").length} ready sources.`)));
  }

  const ta = h("textarea", {
    placeholder: "Ask about your sources…  (Enter to send, Shift+Enter for a new line)",
    "aria-label": "Ask a question",
    onkeydown: (e) => {
      if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
    },
  });

  async function send() {
    const q = ta.value.trim();
    if (!q || state.busy) return;
    state.busy = true;
    ta.value = "";
    scroll.append(messageEl({ role: "user", text: q }));
    const pending = h("div", { class: "msg assistant" }, h("div", { class: "who" }, "Forge"), spinner("Searching sources"));
    scroll.append(pending);
    scroll.scrollTop = scroll.scrollHeight;
    try {
      const r = await api.post(`/notebooks/${nb.id}/chat`, {
        question: q,
        conversation_id: state.conversationId,
        source_ids: [...state.selected],
      });
      state.conversationId = r.conversation_id;
      const msg = {
        role: "assistant", text: r.answer, citations: r.citations,
        abstained: r.abstained, coverage: r.coverage, engine: r.engine,
        computed: r.computed === true, problems: r.citation_problems,
      };
      state.messages.push({ role: "user", text: q }, msg);
      pending.replaceWith(messageEl(msg));
    } catch (e) {
      pending.replaceWith(h("div", { class: "msg assistant" },
        h("div", { class: "who" }, "Forge"),
        h("div", { class: "body", style: "color:var(--err)" }, e.message)));
    } finally {
      state.busy = false;
      scroll.scrollTop = scroll.scrollHeight;
    }
  }

  const composer = h("div", { class: "composer" },
    h("div", { class: "row" },
      ta,
      h("button", { class: "primary", onclick: send }, "Ask")),
    h("div", { class: "row-between", style: "margin-top:7px" },
      h("span", { class: "hint" }, "Answers are extractive: sentences from your sources, never invented."),
      h("span", { class: "hint" }, `${state.selected.size} source(s) selected`)));

  if (onlyComposer) {
    const existing = pane.querySelector(".composer");
    if (existing) existing.replaceWith(composer);
    return;
  }
  pane.replaceChildren(scroll, composer);
  scroll.scrollTop = scroll.scrollHeight;
}

function messageEl(m) {
  if (m.role === "user") {
    return h("div", { class: "msg user" }, h("div", { class: "body" }, m.text));
  }
  const body = h("div", { class: "body" });
  if (m.abstained) {
    body.append(h("div", { class: "abstain" }, m.text));
  } else {
    body.append(document.createTextNode(m.text));
  }
  const cites = h("div", { class: "cites" });
  (m.citations || []).forEach((c) => {
    cites.append(h("button", {
      class: "cite", title: `${c.source_title} — ${c.locator_label || "evidence"}\n“${(c.quote || "").slice(0, 160)}…”`,
      onclick: () => openViewer(c.source_id, c.chunk_id),
    },
      h("span", { class: "n" }, `[${c.n}]`),
      h("span", { class: "src" }, c.source_title || c.source_id),
      h("span", { class: "hint" }, c.locator_label || "")));
  });
  const meta = h("div", { class: "row-between", style: "margin-top:8px" },
    h("span", { class: "hint" },
      m.computed ? "computed from table cells · " : "",
      m.engine ? `engine: ${m.engine} · coverage ${Math.round((m.coverage || 0) * 100)}%` : ""),
    (m.problems || []).length ? h("span", { class: "badge warn" }, "citation check: " + m.problems.join("; ")) : null);
  return h("div", { class: "msg assistant" }, h("div", { class: "who" }, "Forge"), body, cites, meta);
}

// ---------------------------------------------------------------- side panel
function renderSide(pane) {
  const nb = state.nb;
  pane.replaceChildren();

  const tabs = h("div", { class: "tabs", role: "tablist" });
  const content = h("div", { id: "sideContent" });
  for (const [key, label] of [["notes", "Notes"], ["study", "Study"], ["report", "Report"], ["overview", "Overview"]]) {
    tabs.append(h("button", {
      role: "tab", "aria-selected": String(state.tab === key),
      onclick: () => { state.tab = key; renderSide(pane); },
    }, label));
  }
  pane.append(tabs, content);

  if (state.tab === "notes") return renderNotes(content);
  if (state.tab === "overview") return renderOverview(content);
  if (state.tab === "report") return renderReportTab(content);
  return renderStudy(content);
}

function renderNotes(el) {
  const nb = state.nb;
  const body = h("textarea", { placeholder: "Write a note… paste a quote, record a thought.", "aria-label": "Note body" });
  const title = h("input", { type: "text", placeholder: "Note title (optional)", "aria-label": "Note title" });

  el.append(h("div", { class: "stack" }, title, body,
    h("button", { class: "primary", onclick: async () => {
      if (!body.value.trim()) return toast("Note is empty", "err");
      await api.post(`/notebooks/${nb.id}/notes`, { title: title.value, body: body.value, source_ids: [...state.selected] });
      toast("Note saved");
      await openNotebook(document.getElementById("app"), nb.id);
      state.tab = "notes";
    } }, "Save note")));

  for (const n of nb.notes || []) {
    el.append(h("div", { class: "note-item" },
      h("h4", {}, n.title),
      h("div", { style: "white-space:pre-wrap" }, n.body),
      h("div", { class: "row-between", style: "margin-top:6px" },
        h("span", { class: "hint" }, fmtDate(n.updated_at)),
        h("div", { class: "row", style: "gap:6px" },
          h("button", { class: "ghost", title: "Make this note selectable as evidence", onclick: async (e) => {
            e.target.disabled = true;
            try {
              const r = await api.post(`/notes/${n.id}/to-source`, {});
              toast(`“${r.title}” is now a source — select it to use it in chat`);
              await openNotebook(document.getElementById("app"), nb.id);
              state.tab = "notes";
            } catch (err) {
              toast(err.message, "err");
              e.target.disabled = false;
            }
          } }, "use as source"),
          h("button", { class: "ghost danger", onclick: async () => {
            await api.del(`/notes/${n.id}`); toast("Deleted");
            await openNotebook(document.getElementById("app"), nb.id);
            state.tab = "notes";
          } }, "delete")))));
  }
  if (!(nb.notes || []).length) el.append(h("p", { class: "empty" }, "No notes yet."));
}

async function generate(kind, topic = "") {
  const nb = state.nb;
  if (!state.selected.size) throw new Error("Select at least one source first");
  const r = await api.post(`/notebooks/${nb.id}/artifacts`, { kind, topic, source_ids: [...state.selected] });
  return r;
}

function renderOverview(el) {
  el.append(h("p", { class: "hint" }, "A grounded orientation built from the top of each source. Generated by the server; every item is quoted from your sources."));
  const box = h("div", { class: "stack" });
  el.append(box);
  box.append(h("button", { class: "primary", onclick: async (e) => {
    e.target.disabled = true;
    try {
      const r = await generate("summary");
      box.replaceChildren(renderSummary(r.payload));
    } catch (err) { toast(err.message, "err"); e.target.disabled = false; }
  } }, "Generate overview"));
  el.append(h("p", { class: "hint", style: "margin-top:16px" }, "Audio and video overviews are not available in this build — no speech engine is bundled."));

  const scriptOut = h("div", { class: "stack" });
  el.append(h("div", { class: "stack", style: "margin-top:14px" },
    h("div", { class: "row-between" },
      h("span", { class: "hint" }, "Spoken briefing"),
      h("button", { onclick: async (e) => {
        e.target.disabled = true;
        try {
          const r = await generate("audio_script");
          scriptOut.replaceChildren(renderScript(r.payload));
        } catch (err) { toast(err.message, "err"); }
        e.target.disabled = false;
      } }, "Narration script")),
    h("p", { class: "hint" }, "A script you can read aloud or feed to any TTS tool. This build produces text, not audio — it says so in the artifact itself."),
    scriptOut));
}

function renderSummary(p) {
  const list = h("div", { class: "stack" });
  for (const s of p.per_source || []) {
    list.append(h("div", { class: "note-item" },
      h("h4", {}, s.title),
      h("div", { class: "hint" }, `${s.status} · ${s.chunks} passages`),
      h("div", { style: "margin-top:5px" }, s.lead)));
  }
  return list;
}

function renderReportTab(el) {
  const topic = h("input", { type: "text", placeholder: "Focus (optional), e.g. evaluation metrics", "aria-label": "Report focus" });
  const out = h("div", { class: "stack" });
  el.append(h("div", { class: "stack" }, topic,
    h("button", { class: "primary", onclick: async (e) => {
      e.target.disabled = true; e.target.replaceWith(spinner("Building report"));
      try {
        const r = await generate("report", topic.value);
        const box = h("div", {});
        box.append(h("h3", { style: "margin:4px 0 10px;font-size:15px" }, r.title));
        for (const s of r.payload.sections || []) {
          box.append(h("div", { class: "note-item" },
            h("h4", {}, s.source_title),
            h("ul", { style: "margin:4px 0 0 16px;padding:0" },
              (s.points || []).map((pt) => h("li", {},
                pt.text + " ",
                h("button", {
                  class: "cite", title: pt.citation.locator_label,
                  onclick: () => openViewer(pt.citation.source_id, pt.citation.chunk_id),
                }, `[${pt.citation.n}]`))))));
        }
        out.replaceChildren(box);
      } catch (err) { toast(err.message, "err"); }
    } }, "Build report")));
  el.append(out);
  el.append(h("p", { class: "hint", style: "margin-top:14px" }, "Reports are assembled from source sentences with page/paragraph locators — no prose is generated by a model in this build."));
}

function renderStudy(el) {
  const wrap = h("div", { class: "stack" });
  const out = h("div", { class: "art-grid" });
  el.append(wrap, out);

  const actions = [
    ["flashcards", "Flashcards"],
    ["quiz", "Quiz"],
    ["mindmap", "Concept map"],
    ["faq", "FAQ"],
    ["study_guide", "Study guide"],
  ];
  const row = h("div", { class: "row-between", style: "flex-wrap:wrap;gap:6px" });
  for (const [kind, label] of actions) {
    row.append(h("button", { onclick: async (e) => {
      e.target.disabled = true;
      out.replaceChildren(spinner(`Building ${label.toLowerCase()}`));
      try {
        const r = await generate(kind);
        out.replaceChildren(renderArtifact(r.payload));
      } catch (err) { toast(err.message, "err"); out.replaceChildren(); }
      e.target.disabled = false;
    } }, label));
  }
  wrap.append(row, h("p", { class: "hint" }, "Generated from the selected sources only. Click any citation marker to open its evidence."));
}

function renderArtifact(payload) {
  const box = h("div", { class: "stack" });
  if (payload.kind === "flashcards") {
    for (const c of payload.cards || []) {
      const inner = h("div", { class: "inner" }, h("div", {}, c.front));
      let shown = false;
      inner.addEventListener("click", () => {
        shown = !shown;
        inner.replaceChildren(h("div", { class: shown ? "back" : "" }, shown ? c.back : c.front));
      });
      box.append(h("div", { class: "flashcard" }, inner,
        h("div", { class: "row-between", style: "margin-top:4px" },
          h("span", { class: "hint" }, "click to flip"),
          h("button", { class: "cite", onclick: () => openViewer(c.citation.source_id, c.citation.chunk_id) },
            `[${c.citation.n}] ${c.citation.locator_label}`))));
    }
  } else if (payload.kind === "quiz") {
    for (const q of payload.questions || []) {
      const opts = h("div", { class: "opts" });
      q.options.forEach((o, i) => {
        opts.append(h("button", { class: "opt", onclick: (e) => {
          const buttons = opts.querySelectorAll("button");
          buttons.forEach((b) => (b.disabled = true));
          e.target.classList.add(i === q.answer_index ? "correct" : "wrong");
          buttons[q.answer_index].classList.add("correct");
          expl.hidden = false;
        } }, o));
      });
      const expl = h("div", { class: "hint", hidden: true, style: "margin-top:6px" }, "Source: " + q.explanation);
      box.append(h("div", { class: "quiz-q" },
        h("div", {}, q.question), opts, expl,
        h("button", { class: "cite", onclick: () => openViewer(q.citation.source_id, q.citation.chunk_id) },
          `[${q.citation.n}] ${q.citation.locator_label}`)));
    }
  } else if (payload.kind === "mindmap") {
    box.append(renderMindmap(payload));
  } else if (payload.kind === "faq") {
    for (const it of payload.items || []) {
      box.append(h("div", { class: "note-item" },
        h("h4", {}, it.question), h("div", {}, it.answer),
        h("button", { class: "cite", style: "margin-top:6px", onclick: () => openViewer(it.citation.source_id, it.citation.chunk_id) },
          `[${it.citation.n}] ${it.citation.locator_label}`)));
    }
  } else if (payload.kind === "study_guide") {
    box.append(h("h3", { style: "font-size:15px;margin:0" }, payload.title));
    const terms = h("div", { class: "cites" });
    for (const t of payload.key_terms || []) terms.append(h("span", { class: "badge" }, `${t.term} · ${t.weight}`));
    box.append(h("div", { class: "hint" }, "Key terms"), terms);
    for (const s of payload.key_points || []) {
      box.append(h("div", { class: "note-item" }, h("h4", {}, s.source_title),
        h("ul", { style: "margin:4px 0 0 16px;padding:0" },
          (s.points || []).map((pt) => h("li", {}, pt.text + " ",
            h("button", { class: "cite", onclick: () => openViewer(pt.citation.source_id, pt.citation.chunk_id) }, `[${pt.citation.n}]`))))));
    }
  } else if (payload.kind === "report") {
    box.append(h("h3", { style: "font-size:15px;margin:0" }, payload.title));
    for (const s of payload.sections || []) {
      box.append(h("div", { class: "note-item" }, h("h4", {}, s.source_title),
        h("ul", { style: "margin:4px 0 0 16px;padding:0" },
          (s.points || []).map((pt) => h("li", {}, pt.text + " ",
            h("button", { class: "cite", onclick: () => openViewer(pt.citation.source_id, pt.citation.chunk_id) }, `[${pt.citation.n}]`))))));
    }
  } else if (payload.kind === "summary") {
    box.append(renderSummary(payload));
  } else {
    box.append(h("pre", { style: "font-size:12px;overflow:auto" }, JSON.stringify(payload, null, 2)));
  }
  return box;
}

function renderScript(p) {
  const box = h("div", { class: "stack" });
  box.append(h("div", { class: "hint" },
    `${p.word_count} words · about ${Math.max(1, Math.round((p.estimated_seconds || 0) / 60))} min · ${p.voice}`));
  p.segments.forEach((seg) => {
    const line = h("div", { class: "note-item", style: "white-space:pre-wrap" }, seg.text);
    if (seg.citation) {
      line.append(h("button", {
        class: "cite", style: "margin-top:6px",
        onclick: () => openViewer(seg.citation.source_id, seg.citation.chunk_id),
      }, `[${seg.citation.n}] ${seg.citation.locator_label}`));
    }
    box.append(line);
  });
  box.append(h("p", { class: "hint" }, p.note || ""));
  const copy = h("button", { class: "ghost", onclick: async () => {
    const text = p.segments.map((s) => s.text).join("\n\n");
    try { await navigator.clipboard.writeText(text); toast("Script copied"); }
    catch { toast("Copy blocked by the browser", "err"); }
  } }, "Copy script text");
  box.append(copy);
  return box;
}

function renderMindmap(mm) {
  const nodes = (mm.nodes || []).slice(0, 18);
  const W = 360, H = 320, cx = W / 2, cy = H / 2, R = Math.min(W, H) / 2 - 46;
  const pos = new Map();
  nodes.forEach((n, i) => {
    const a = (i / Math.max(1, nodes.length)) * Math.PI * 2 - Math.PI / 2;
    const r = R * (0.65 + 0.35 * (i % 2));
    pos.set(n.id, { x: cx + Math.cos(a) * r, y: cy + Math.sin(a) * r });
  });
  const svgNS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(svgNS, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  const maxW = Math.max(1, ...(mm.edges || []).map((e) => e.weight));
  for (const e of mm.edges || []) {
    const a = pos.get(e.from), b = pos.get(e.to);
    if (!a || !b) continue;
    const line = document.createElementNS(svgNS, "line");
    line.setAttribute("x1", a.x); line.setAttribute("y1", a.y);
    line.setAttribute("x2", b.x); line.setAttribute("y2", b.y);
    line.setAttribute("stroke", "currentColor");
    line.setAttribute("stroke-opacity", String(0.12 + 0.35 * (e.weight / maxW)));
    line.setAttribute("stroke-width", String(1 + 2 * (e.weight / maxW)));
    svg.append(line);
  }
  for (const n of nodes) {
    const p = pos.get(n.id);
    const dot = document.createElementNS(svgNS, "circle");
    dot.setAttribute("cx", p.x); dot.setAttribute("cy", p.y);
    dot.setAttribute("r", String(4 + Math.min(6, n.weight)));
    dot.setAttribute("fill", "#d98b3a");
    svg.append(dot);
    const label = document.createElementNS(svgNS, "text");
    label.setAttribute("x", p.x); label.setAttribute("y", p.y - 9);
    label.setAttribute("font-size", "10");
    label.setAttribute("text-anchor", "middle");
    label.setAttribute("fill", "currentColor");
    label.textContent = n.label;
    svg.append(label);
  }
  return h("div", { class: "mindmap" },
    h("p", { class: "hint" }, `Concept co-occurrence map — ${nodes.length} terms, ${(mm.edges || []).length} links`),
    svg);
}

// ---------------------------------------------------------------- source viewer
async function openViewer(sourceId, highlightChunkId) {
  const [src, data, tableData, guide] = await Promise.all([
    api.get(`/sources/${sourceId}`),
    api.get(`/sources/${sourceId}/chunks`),
    api.get(`/sources/${sourceId}/tables`).catch(() => ({ tables: [] })),
    api.get(`/sources/${sourceId}/guide`).catch(() => null),
  ]);
  let chunks = data.chunks || [];
  let page = null;
  const tables = tableData.tables || [];

  const content = h("div", { class: "content" });
  const pager = h("div", { class: "pager" });

  // The guide quotes the source; it never paraphrases it. Every line links to its passage.
  function guideBlock(g) {
    if (!g) return null;
    const jump = (chunkId) => () => {
      content.querySelectorAll(".chunk.hit").forEach((el) => el.classList.remove("hit"));
      const el = content.querySelector("#chunk-" + chunkId);
      if (el) { el.classList.add("hit"); el.scrollIntoView({ block: "center" }); }
    };
    const quote = (s, cls) => h("div", { class: "guide-quote" + (cls ? " " + cls : ""), title: `${s.locator} — click to open the passage`, onclick: jump(s.chunk_id) },
      "“" + s.text + "”", h("span", { class: "loc" }, " — " + s.locator));
    const asks = (q) => h("button", { class: "cite", title: "Put this question in the composer", onclick: () => {
      const ta = document.querySelector(".composer textarea");
      if (ta) { ta.value = q; ta.focus(); }
    } }, q);
    return h("div", { class: "guide" },
      h("div", { class: "guide-head" },
        h("strong", {}, "Source guide"),
        h("span", { class: "badge", title: "No language model was used: these are verbatim quotations" },
          "extractive · quoted, not generated")),
      g.headline ? quote(g.headline, "lead") : null,
      (g.summary || []).length ? h("div", {}, g.summary.map((s) => quote(s))) : null,
      (g.topics || []).length
        ? h("div", { class: "chips", style: "margin-top:8px" },
            h("span", { class: "hint" }, "Recurring terms: "),
            g.topics.map((t) => h("span", { class: "badge", title: `${t.count} mentions in ${t.chunks} passages · first at ${t.locator}` },
              `${t.term} · ${t.count}`)))
        : null,
      (g.outline || []).length
        ? h("p", { class: "hint", style: "margin-top:6px" }, "Structure: " + g.outline.join(" · "))
        : null,
      (g.questions || []).length
        ? h("div", { style: "margin-top:8px" },
            h("div", { class: "hint" }, "Try asking:"),
            h("div", { class: "cites" }, g.questions.map(asks)))
        : null,
      (g.warnings || []).length
        ? h("p", { class: "hint", style: "color:var(--warn)" }, g.warnings.join(" · "))
        : null,
      h("p", { class: "hint" }, `${g.stats.words} words · ${g.stats.chunks} passages` +
        (g.stats.tables ? ` · ${g.stats.tables} sheet(s), ${g.stats.table_rows} rows` : "")));
  }

  // A cited cell opens on the real grid, so "Sales!C2:C6" is verifiable, not decorative.
  function gridBlock(t) {
    const head = h("tr", {}, h("th", { class: "rownum" }, ""),
      ...t.columns.map((c) => h("th", {}, c)));
    const rows = (t.rows || []).map((r) => {
      const byCol = {};
      for (const c of r.cells || []) byCol[c.column] = c;
      return h("tr", {},
        h("th", { class: "rownum", title: `row ${r.row}` }, String(r.row)),
        ...t.columns.map((col) => {
          const cell = byCol[col];
          const shown = cell ? (cell.text === "" ? "" : cell.text) : "";
          return h("td", {
            class: cell && cell.number !== null && cell.number !== undefined ? "num" : "",
            title: cell ? `${cell.ref}${cell.text === "" ? " (empty)" : ""}` : "cell not present",
          }, shown);
        }));
    });
    return h("div", { class: "sheet" },
      h("div", { class: "loc" },
        `${t.sheet} · ${t.row_count} row${t.row_count === 1 ? "" : "s"}${t.hidden ? " · hidden sheet" : ""}`),
      h("div", { class: "gridwrap" }, h("table", { class: "grid" },
        h("thead", {}, head), h("tbody", {}, ...rows))),
      t.truncated ? h("p", { class: "hint" }, `Showing the first ${t.rows.length} of ${t.row_count} rows.`) : null,
      (t.warnings || []).length ? h("p", { class: "hint" }, t.warnings.join(" · ")) : null);
  }

  function paint() {
    const shown = page === null ? chunks : chunks.filter((c) => c.locator && c.locator.page === page);
    const chunksEl = shown.length ? shown.map((c) =>
      h("div", { class: "chunk" + (c.id === highlightChunkId ? " hit" : ""), id: "chunk-" + c.id },
        h("div", { class: "loc" }, c.locator_label),
        h("div", { style: "white-space:pre-wrap" }, c.text))) : [h("p", { class: "empty" }, "No text on this page.")];
    // guide first (what this source is), then the grid (the raw data), then passages
    const guideEl = guideBlock(guide);
    content.replaceChildren(...(guideEl ? [guideEl] : []), ...tables.map(gridBlock), ...chunksEl);
    const hit = content.querySelector(".hit");
    if (hit) hit.scrollIntoView({ block: "center" });
  }

  const overlay = h("div", { class: "overlay", onclick: (e) => { if (e.target === overlay) close(); } },
    h("div", { class: "viewer", role: "dialog", "aria-modal": "true", "aria-label": "Source viewer" },
      h("header", {},
        h("div", { style: "flex:1;min-width:0" },
          h("div", { style: "font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" }, src.title),
          h("div", { class: "hint" },
            `${src.kind} · ${src.chunk_count} passages · ${src.status}` +
            (tables.length ? ` · ${tables.length} sheet${tables.length === 1 ? "" : "s"}` : ""))),
        pager,
        h("button", { class: "ghost", onclick: close, "aria-label": "Close" }, "✕")),
      content));

  function close() {
    overlay.remove();
    document.removeEventListener("keydown", onKey);
  }
  function onKey(e) { if (e.key === "Escape") close(); }
  document.addEventListener("keydown", onKey);

  if (src.page_count > 0) {
    pager.append(
      h("button", { class: "ghost", onclick: () => { page = null; paint(); } }, "All"),
      h("button", { class: "ghost", onclick: () => { page = Math.max(1, (page || 1) - 1); paint(); } }, "‹"),
      h("span", { id: "pageLabel" }, page ? `page ${page} / ${src.page_count}` : `${src.page_count} pages`),
      h("button", { class: "ghost", onclick: () => { page = Math.min(src.page_count, (page || 0) + 1); paint(); } }, "›"));
    const origPaint = paint;
    paint = () => {
      origPaint();
      const lbl = overlay.querySelector("#pageLabel");
      if (lbl) lbl.textContent = page ? `page ${page} / ${src.page_count}` : `${src.page_count} pages`;
    };
  } else {
    pager.append(h("span", { class: "hint" }, "single document"));
  }

  document.body.append(overlay);
  if (highlightChunkId) {
    const c = chunks.find((x) => x.id === highlightChunkId);
    if (c && c.locator && c.locator.page) page = c.locator.page;
  }
  paint();
}

async function showArtifact(id) {
  try {
    const a = await api.get(`/artifacts/${id}`);
    const overlay = h("div", { class: "overlay", onclick: (e) => { if (e.target === overlay) overlay.remove(); } },
      h("div", { class: "viewer" },
        h("header", {},
          h("div", { style: "flex:1" }, h("div", { style: "font-weight:600" }, a.title),
            h("div", { class: "hint" }, `${a.kind} · ${fmtDate(a.created_at)}`),
            a.stale ? h("div", { class: "hint", style: "color:var(--warn)" }, "A source changed after this artifact was generated — regenerate to refresh it.") : null),
          h("button", { class: "ghost", onclick: () => overlay.remove() }, "✕")),
        h("div", { class: "content" }, renderArtifact(a.payload))));
    document.body.append(overlay);
  } catch (e) { toast(e.message, "err"); }
}

// ---------------------------------------------------------------- theme + boot
document.getElementById("themeBtn").addEventListener("click", () => {
  const root = document.documentElement;
  const next = root.dataset.theme === "light" ? "dark" : "light";
  root.dataset.theme = next;
  localStorage.setItem("forge.theme", next);
});
{
  const saved = localStorage.getItem("forge.theme");
  if (saved) document.documentElement.dataset.theme = saved;
}

async function boot() {
  try {
    const caps = await api.get("/capabilities");
    state.caps = caps;
    document.getElementById("engineBadge").textContent = `engine: ${caps.answering.engine}`;
    document.getElementById("engineBadge").classList.add("warn");
    document.getElementById("engineBadge").title = caps.answering.llm.reason || "";
  } catch (e) {
    document.getElementById("engineBadge").textContent = "engine: unknown";
    document.getElementById("engineBadge").classList.add("err");
  }
  await route();
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("/sw.js").catch(() => {});
  }
}

boot();

// ------------------------------------------------------- binary inspection
// Renders a structural report for an uploaded APK / EXE / DLL / ELF.
// Structural only: it never claims decompiled source (see docs/BINARY_ANALYSIS.md).
async function binaryPanel() {
  const wrap = h("section", { style: "margin-top:26px" });
  wrap.append(h("h2", { style: "font-size:15px;margin:0 0 8px" }, "Binary inspection (APK / EXE)"));
  wrap.append(h("p", { class: "hint" },
    "Drop an APK, EXE, DLL or ELF. Forge decodes the container, the binary manifest, the DEX reference graph and the PE import/export tables — with no JVM, no Ghidra and no network. " +
    "It reports structure and risk indicators; it does not produce decompiled source or instruction-level disassembly."));

  const file = h("input", { type: "file", accept: ".apk,.exe,.dll,.so,.bin,.zip", "aria-label": "Binary file" });
  const out = h("div", { class: "stack", style: "margin-top:10px" });
  const btn = h("button", { class: "primary", onclick: async () => {
    const f = file.files && file.files[0];
    if (!f) { toast("Choose a file first", "err"); return; }
    btn.disabled = true;
    out.replaceChildren(h("p", { class: "empty" }, `analysing ${f.name} (${f.size} bytes)…`));
    try {
      const res = await api.call(`/binary/inspect?name=${encodeURIComponent(f.name)}`, { method: "POST", body: f, raw: true });
      out.replaceChildren(renderBinaryReport(res.report));
    } catch (e) {
      out.replaceChildren(h("p", { class: "empty" }, e.message));
    } finally { btn.disabled = false; }
  } }, "Analyse");

  wrap.append(h("div", { class: "stack", style: "margin:10px 0" }, file, btn), out);
  return wrap;
}

function kvTable(rows) {
  return h("table", { style: "width:100%;border-collapse:collapse" },
    h("tbody", {}, rows.filter(Boolean).map(([k, v]) =>
      h("tr", {}, h("td", { class: "hint", style: "width:38%;padding:2px 0" }, k), h("td", {}, String(v))))));
}

function renderBinaryReport(r) {
  if (!r) return h("p", { class: "empty" }, "empty report");
  const frag = document.createDocumentFragment();
  frag.append(h("div", { class: "note-item" },
    h("div", { class: "row-between" },
      h("strong", {}, r.file || "(artifact)"),
      h("span", { class: "badge " + (r.format === "unknown" ? "warn" : "ok") }, r.format || "unknown")),
    kvTable([
      ["size", `${r.bytes} bytes`],
      ["sha256", r.sha256],
      ["sha1", r.sha1],
      ["entropy", typeof r.entropy === "number" ? `${r.entropy.toFixed(2)} bits/byte` : ""],
    ])));

  if (r.error) frag.append(h("div", { class: "note-item" }, h("strong", {}, "not analysed"), h("div", { class: "hint" }, r.error)));

  if (r.apk) {
    const a = r.apk;
    const comps = ["activities", "services", "receivers", "providers"]
      .flatMap((g) => (a[g] || []).map((c) => ({ ...c, kind: g.slice(0, -1) })))
      .filter((c) => c.exported);
    frag.append(h("div", { class: "note-item" },
      h("div", { class: "row-between" },
        h("strong", {}, a.package || "(no package)"),
        h("span", { class: "badge " + (a.debuggable ? "warn" : "ok") }, a.debuggable ? "debuggable" : "release")),
      kvTable([
        ["version", `${a.version_name || "?"} (code ${a.version_code || "?"})`],
        ["sdk", `min ${a.min_sdk || "?"} / target ${a.target_sdk || "?"}`],
        ["backup / cleartext", `${a.allow_backup ? "backup allowed" : "backup blocked"} · ${a.cleartext_traffic ? "cleartext allowed" : "cleartext blocked"}`],
        ["code", `${(a.dex_files || []).join(", ")} — ${a.classes} classes, ${a.methods} methods, ${a.strings} strings`],
        ["container", `${a.entries} entries, ${a.uncompressed_bytes} bytes uncompressed`],
        ["native", (a.native_libs || []).length ? `${(a.native_libs || []).length} libs (${(a.abis || []).join(", ")})` : "pure Java/Kotlin"],
        ["signature", (a.signatures || []).length ? (a.signatures || []).map((s) => s.file).join(", ") : "no v1 signature block"],
      ]),
      (a.permissions || []).length
        ? h("div", { class: "hint", style: "margin-top:6px" }, `permissions: ${a.permissions.join(", ")}`)
        : null,
      comps.length
        ? h("div", { class: "hint", style: "margin-top:6px" }, `exported: ${comps.map((c) => `${c.name} (${c.kind})`).join(", ")}`)
        : null));

    if ((a.code_findings || []).length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, "Code findings (DEX reference graph)"),
        ...a.code_findings.slice(0, 20).map((f) =>
          h("div", { class: "hint" }, `[${f.category}] ${f.detail}`))));
    }
    if ((a.embedded_urls || []).length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, "Hardcoded cleartext URLs"),
        ...a.embedded_urls.slice(0, 20).map((u) => h("div", { class: "hint" }, u))));
    }
    if ((a.class_names || []).length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, `Application classes (${a.class_names.length})`),
        h("div", { class: "hint" }, a.class_names.slice(0, 40).join(", "))));
    }
  }

  if (r.pe) {
    const p = r.pe;
    frag.append(h("div", { class: "note-item" },
      h("div", { class: "row-between" },
        h("strong", {}, `${p.bits}-bit ${p.machine}`),
        h("span", { class: "badge " + (p.dotnet ? "warn" : "ok") }, p.dotnet ? `managed ${p.dotnet_version}` : "native")),
      kvTable([
        ["subsystem", p.subsystem],
        ["entry point", `${p.entry_point_rva} (image base ${p.image_base})`],
        ["sections", (p.sections || []).map((s) => `${s.name} ${s.entropy.toFixed(2)}`).join(" · ")],
        ["imports", (p.imports || []).map((i) => `${i.dll} (${i.functions.length})`).join(", ") || "none"],
        ["exports", (p.exports || []).join(", ") || "none"],
        ["overlay", `${p.overlay_bytes} bytes`],
      ])));
    if ((p.version_info || []).length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, "Version info"),
        kvTable(p.version_info.map((v) => [v.key, v.value]))));
    }
    if ((p.indicators || []).length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, "Anomalies / packer signals"),
        ...p.indicators.map((i) => h("div", { class: "hint" }, `! ${i}`))));
    }
    const strings = [...(p.wide_strings || []), ...(p.ascii_strings || [])].slice(0, 20);
    if (strings.length) {
      frag.append(h("div", { class: "note-item" },
        h("strong", {}, "Strings"),
        ...strings.map((s) => h("div", { class: "hint" }, s.length > 160 ? s.slice(0, 160) + "…" : s))));
    }
  }

  if (r.elf) {
    const e = r.elf;
    frag.append(h("div", { class: "note-item" },
      h("strong", {}, `ELF ${e.type}`),
      kvTable([
        ["machine", `${e.bits}-bit ${e.machine}`],
        ["entry", e.entry],
        ["symbols", e.stripped ? "stripped" : "symbol table present"],
        ["sections", (e.sections || []).join(", ")],
      ])));
  }

  return frag;
}
