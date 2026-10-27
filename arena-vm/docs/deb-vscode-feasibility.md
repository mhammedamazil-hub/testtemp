# `.deb` installs and VS Code — feasibility findings (2026-10-27)

All claims here were tested on the live sandbox (Debian 12 bookworm, x86_64,
3.8 GiB RAM). Raw command evidence: `evidence/deb-vscode-probes.txt`.

## 1. Can applications be installed from `.deb` files? — YES, locally

The mechanism is fully available:

- `dpkg`, `dpkg-deb`, `apt`, `apt-get` are installed; the user has
  passwordless `(ALL : ALL) ALL` sudo.
- A package was built locally with `dpkg-deb`, installed via
  `sudo dpkg -i`, executed successfully, queried with `dpkg -L/-s`, and
  cleanly removed with `sudo dpkg -r`. Round-trip verified.

The binding constraint is **obtaining** third-party `.deb` files, not
installing them:

- `apt-get update` fails — `deb.debian.org` is outside the egress allowlist.
- `packages.microsoft.com`, `update.code.visualstudio.com`,
  `code.visualstudio.com`, `electronjs.org`, and
  `objects.githubusercontent.com` / `release-assets.githubusercontent.com`
  all fail with `SSL_ERROR_SYSCALL` (HTTP 000).
- GitHub release links return 302 redirects to
  `release-assets.githubusercontent.com`, which is blocked. Therefore even
  projects that publish `.deb`/AppImage binaries as GitHub **release
  assets** (VS Code, VSCodium, code-server, Electron, Node itself) cannot be
  downloaded — only git clones, `api.github.com`, `codeload.github.com`
  (source archives), `registry.npmjs.org` and PyPI work.

So: locally-built or allowlist-sourced `.deb` packages install fine;
arbitrary vendor `.deb` downloads are impossible without a rules change.

## 2. Can VS Code be installed and used? — YES, built from source (web)

The official Microsoft `.deb` is unreachable, but VS Code is MIT-licensed
open source and its complete source is on GitHub. A working **Code-OSS
1.143.1 web workbench** was built and run using only github.com +
registry.npmjs.org, at zero cost.

### What was built

- `git clone --depth 1 --branch 1.143.1 https://github.com/microsoft/vscode`
  (resolved commit `e646868227c3e3a9e3f24277c8c7220be42c0be7`), outside the
  project Git repo so the multi-GB tree is never committed.
- Dependencies installed with **`npm install --ignore-scripts`** in the root
  and all 55 sub-project directories listed in `build/npm/dirs.ts` (1,592
  root packages, ~1.6 GB, zero failures). `--ignore-scripts` is essential:
  the normal postinstall drives `node-gyp` at `electronjs.org` for Electron
  headers (blocked) and compiles native modules; the web workbench does not
  need either.
- The repo's `.nvmrc` requires Node 24 but the sandbox has Node 22.22.3.
  The repo's own `preinstall.ts` provides the supported escape hatch
  `VSCODE_SKIP_NODE_VERSION_CHECK=1`; Node 22's `--experimental-strip-types`
  runs the TypeScript build tooling correctly.
- First bundle attempt was OOM-killed (exit 137, confirmed in kernel log).
  Fix: a 6 GiB swap file (`swapon`) and lowering the gulp heap cap in
  `package.json` from 8192 to 4096 MB.
- Build: `npm run gulp compile-web` (built-in web extensions) then
  `npm run gulp transpile-client-esbuild` (10,440 source files → `out/`,
  ~18 s).
- Run: `node scripts/code-web.js <workspace-folder> --host 0.0.0.0 --port
  9000 --browserType=none`, served by `@vscode/test-web` (npm package).
- Verified over HTTP: workbench shell 200 (639 KB),
  `workbench.web.main.internal.js` 200, `workbench.html` 200, mounted
  workspace files 200. The editor is reachable in a browser at the sandbox
  preview URL on port 9000.

### Honest limitations of this build

- It is the **browser** workbench (Code-OSS web), not the Electron desktop
  app: no native window, no OS integration, no terminal process host beyond
  what the web extension host provides, files opened via a mounted folder
  served over HTTP.
- No Microsoft Marketplace and no binary extensions that ship native
  Electron modules; the built-in web extensions (JSON/CSS/HTML/Markdown
  language features, Emmet, Git, themes, etc.) are compiled in.
- It depends on the sandbox's preinstalled Node 22 and a swap file; the
  source tree (~4 GB with node_modules) is outside Git and is wiped on
  sandbox recycle — rebuild steps are recorded here and in
  `evidence/deb-vscode-probes.txt`.
- EEVEE/GPU topics are unrelated here; the workbench is pure CPU/JS.

### Reproduce from scratch (allowlisted hosts only)

```bash
git clone --depth 1 --branch 1.143.1 https://github.com/microsoft/vscode.git vscode-src
cd vscode-src
# 6 GiB swap if RAM is ~4 GiB (reversible: swapoff + rm)
sudo fallocate -l 6G $HOME/.vscode-swapfile && sudo chmod 600 $HOME/.vscode-swapfile
sudo /usr/sbin/mkswap $HOME/.vscode-swapfile && sudo /usr/sbin/swapon $HOME/.vscode-swapfile
sed -i 's/--max-old-space-size=8192/--max-old-space-size=4096/' package.json
export VSCODE_SKIP_NODE_VERSION_CHECK=1
npm install --ignore-scripts --no-audit --no-fund
grep -oP "^\t'[^']*'" build/npm/dirs.ts | sed "s/\t'//;s/'//" | while read d; do
  [ -n "$d" ] && (cd "$d" && npm install --ignore-scripts --no-audit --no-fund)
done
npm run gulp compile-web
npm run gulp transpile-client-esbuild
mkdir -p $HOME/vscode-workspace   # put project files here
node scripts/code-web.js $HOME/vscode-workspace --host 0.0.0.0 --port 9000 --browserType=none
```

## 3. The arena-vm desktop itself

A sandbox recycle wiped `src/build/local/run` and reset the Git checkout to
its base commit during this session (a known failure mode). The branch was
re-fetched and `scripts/recover.sh` rebuilt the Xvfb/Openbox stack from
markers; acceptance results after that rebuild are recorded in
`evidence/acceptance.txt`. This re-confirms the documented recovery path
in `SKILL.md` / `FROM-ZERO-GUIDE.md`.
