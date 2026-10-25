# Deployment

Two supported shapes, one shared engine. Neither is claimed to be live unless a
URL was actually visited — see "What is verified" at the end.

---

## Shape 1 — supervised app (requires a process)

```bash
cd blender-web-agent
python3 app/server.py --host 0.0.0.0 --port 8077
```

Requirements: Node 22+, a Chromium binary, the Blender wasm kit on disk, and a
browser. This is the full workspace: sessions, artifacts, agent API, real
crashes and all.

Verify before believing:

```bash
curl -s localhost:8077/api/health
node app/test_ui.mjs            # 12/12 checks
```

The server sets `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` on every response. Blender's
pthreads need `SharedArrayBuffer`, and those headers are what unlock it.

Hosts that can set headers: Cloudflare Pages/Workers, Netlify, Render, Fly,
any container or VM. **GitHub Pages cannot.**

---

## Shape 2 — static bundle (no backend)

```bash
./deploy/stage-static.sh /path/to/blender-web-local /tmp/bw-static
python3 -m http.server 8089 --directory /tmp/bw-static     # or any static host
```

`deploy/stage-static.sh` assembles: `index.html`, `static/static-app.js`,
`static/glb.js`, `static/static.css`, `coi-serviceworker.min.js`, `bw_ops.py`,
and the engine under `bin/` (wasm + data are symlinked from the kit so the
repository never carries 285 MB).

In this shape the browser does **all** of the work:

- the plan is embedded into `--python-expr` as base64,
- Blender runs in the tab and prints `BW_RESULT` / `BW_FILE_*` / `BW_DONE`,
- artifact bytes are reassembled in page memory and offered as downloads,
- the GLB is decoded and drawn as a wireframe preview.

No request ever leaves the origin. There is no API to call, which is also why
`/api/*` returns 404 in the test suite.

**One plan per page boot.** The Blender runtime cannot be re-entered after it
exits, so a second run reloads the document with the chosen plan in
`sessionStorage`. This is an engine property, stated in the UI.

### The header problem — solved with a service worker

GitHub Pages cannot send COOP/COEP, and without them `SharedArrayBuffer` is
unavailable and Blender cannot start. `coi-serviceworker` installs a service
worker that injects those headers into same-origin responses at runtime.

Verified on a deliberately dumb static server that sends **no** COOP/COEP
(`bw-pages/serve_static.mjs`, `bw-pages/probe_pages.mjs`):

```
A  first visit          coi=true  sab=function  controller=true  6.0 s
   BW_STATIC_PROBE blender=5.2.0 LTS objs=3
B  after reload         coi=true  sab=function  controller=true  7.6 s
   BW_STATIC_PROBE blender=5.2.0 LTS objs=3
```

So real Blender executes Python on a header-less host. Two caveats that keep
this honest:

1. The first paint is not isolated; the shim reloads the page once. Budget for
   that, and show a "preparing engine" state.
2. A service worker only works over HTTPS or `localhost`, and only within its
   scope. It is a workaround for headers, not a substitute for them.

---

## GitHub Pages

Static shape only. Publish the staged directory to a `gh-pages` branch or the
`/docs` folder, and keep the engine out of Git:

```bash
./deploy/stage-static.sh ~/blender-web-local /tmp/bw-static
git worktree add /tmp/gh-pages gh-pages
rsync -a --delete /tmp/bw-static/ /tmp/gh-pages/
cd /tmp/gh-pages && git add -A && git commit -m "static build" && git push
```

`.nojekyll` must exist so files beginning with `_` are served. `_headers` is
ignored by Pages — that is exactly why the service worker is included.

**Pages must not be treated as a backend.** There is no upload endpoint, no
session store, no artifact server. The static build is designed around that.

---

## Cloudflare Pages

Supports headers, so the service worker becomes optional (keep it as a fallback
for browsers that skip the first reload).

```
Build command:      ./deploy/stage-static.sh ../blender-web-local dist
Output directory:   dist
```

`deploy/static/_headers` already contains the required COOP/COEP block and an
immutable cache policy for `/bin/*`.

---

## What is verified, and what is not

**Verified here, in this sandbox**

- Supervised app boots and serves COOP/COEP; UI drives real Blender; 12/12.
- Static bundle runs real Blender on a no-header host; 10/10, ~11 s per plan.
- `coi-serviceworker` restores `crossOriginIsolated` and `SharedArrayBuffer`
  on that host.
- 100 unit tests, baseline 5/5.

**Not verified, and therefore not claimed**

- Any public deployment URL. No deployment host was reachable with credentials
  from this environment, so no URL is published and none is described as live.
- GitHub Pages with the actual Pages CDN in front of it (behaviour should match
  the emulated server, but "should" is not "did").
- Safari and Firefox. Only Chromium 153 was exercised.

**Do not** announce a live URL for this project until someone has loaded it in a
browser and run one plan end to end.
