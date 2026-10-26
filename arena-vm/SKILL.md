# SKILL.md — Complete guide for a brand-new AI (zero prior context)

You are an AI that has never seen this project, this chat, or this machine.
This file is the entire briefing. Read it top to bottom before typing
anything. By the end you will be able to: rebuild the whole system from an
empty disk, control the virtual desktop, verify it, and repair failures.

---

# PART A — What this is, and what the machine must have

## A.1 In one paragraph

The repository builds a **complete, AI-controllable virtual Linux graphical
desktop** entirely from free open-source software, with no root package
installs and no money spent. A virtual X11 server (**Xvfb**) runs a real
screen in memory; **Openbox** manages windows; **st/dmenu/dillo/vim/ranger**
provide terminal, launcher, browser, editor and file manager; the AI drives
them by injecting genuine keyboard/mouse events (XTEST) through
`control/vmc.py` and reads the screen back through screenshots. Blender
(bpy) and an authenticated noVNC tablet bridge are optional add-ons.
Everything is installed under `arena-vm/local/`, pinned to exact git commits
so any rebuild is reproducible.

## A.2 Where the code lives

- Repository: `https://github.com/mhammedamazil-hub/testtemp.git`
- Working branch: `arena/e86028ee-testtemp` (never create or switch branches;
  all work happens on this branch). `main` only contains an older base.
- The project itself is in the **`arena-vm/` subdirectory** of that repo.
  All commands below assume your working directory is `arena-vm/` unless
  stated otherwise.

## A.3 Host machine prerequisites (verify first)

Run this block. Every line must succeed before you start building:

```bash
set -x
git --version
gcc --version | head -1
g++ --version | head -1
make --version | head -1
python3 --version                 # 3.11 expected; venv module required
python3 -m venv --help >/dev/null && echo venv-ok
sudo -n true && echo sudo-ok      # passwordless sudo (used ONLY for ldconfig)
tar --version | head -1
# network: only these three outbound hosts are permitted
curl -sS -o /dev/null -w 'github %{http_code}\n' --max-time 15 https://github.com
curl -sS -o /dev/null -w 'pypi   %{http_code}\n' --max-time 15 https://pypi.org
df -h . | tail -1                 # need ~8 GB free for the full stack + bpy
```

If `python3-venv` is missing and you cannot apt-install, stop and report
BLOCKED — the build cannot proceed. If any of the three network probes fail,
report BLOCKED with the output; there is no workaround inside the allowlist.

## A.4 Non-negotiable environment rules

1. **Egress allowlist = github.com, pypi.org, registry.npmjs.org only.**
   No Google, no ftp.gnu.org, no apt mirrors, no other sites. A browser
   inside the desktop therefore cannot load external websites — this is
   expected, not a bug. Use `file://` pages to demonstrate the browser.
2. **The sandbox can be wiped at any time.** `src/ build/ local/ run/` are
   git-ignored and may vanish. Git-tracked files are the only durable state.
3. **Zero budget.** FOSS, PyPI, and source builds only. No paid APIs,
   tunnels, domains, cloud VMs, or external LLM services. The Arena agent
   (you) is the only controller/"brain".
4. **Never put credentials in Git.** The tablet password is generated at
   runtime into `run/tablet.cred` (git-ignored).
5. **Never fake evidence.** Outcomes are PASS / FAIL / BLOCKED / NOT_TESTED
   with real logs or screenshots behind each claim.
6. All builds are unprivileged except one `ldconfig` call inside recover.sh.

---

# PART B — Every tracked file, explained

The repo contains **41 tracked files**. Generated directories (`src/`,
`build/`, `local/`, `run/`, evidence media) are NOT in Git — Part E explains
them. Here is the complete inventory.

## B.1 Top-level files

| File | What it contains / does |
|---|---|
| `SKILL.md` | This guide. |
| `requirements.txt` | Python packages installed into `local/venv` by recover.sh: meson, ninja, cmake (build tools), python-xlib (X11 control), pillow + mss (screenshots), matplotlib (font source + plotting), websockify (noVNC bridge), ranger-fm (file manager). bpy is deliberately commented out (520 MB, installed separately). |
| `pins.env` | **The reproducibility ledger.** 80 lines `PIN_<name>=<full 40-char git SHA>` recording the exact commit of every source checkout. Rewritten by fetch.sh on each clone. |
| `.gitignore` | Keeps all generated artifacts and credentials out of Git: `src/ build/ local/ run/`, `*.pid`, `*.pyc`, evidence images/logs/blends, recovery logs. |

## B.2 `scripts/` — the build and runtime engine (16 files)

### Orchestration

| File | Contents |
|---|---|
| `recover.sh` | **Master entry point — the one command you normally run.** Six phases: (1) create venv + pip install, (2) fetch sources, (3) ldconfig registration, (4) X11/Xvfb stack, (5) apps + suckless + Openbox chain (+ Blender extras with `--blender`), (6) start the desktop. Flags: `--tablet` (also start noVNC bridge), `--blender` (also build bison/xkbcommon/glvnd + install bpy). Marker-gated, so re-running resumes. |
| `env.sh` | Sourced by every other script. Defines `$VM_ROOT $VM_SRC $VM_BUILD $VM_PREFIX(=local) $VM_VENV`; exports PATH, PKG_CONFIG paths, CFLAGS/LDFLAGS with rpath, `LD_LIBRARY_PATH` (so wheels like bpy find local .so files), `TERMINFO` (local ncurses database). Provides helper functions: `log/fail`, `apply_patch` (idempotent unified-diff patcher), `pin_timestamps` (sets source mtimes to 2020-01-01 to neutralize autotools remake rules), `conf_build`, `meson_build`, `cmake_build`, `make_build`. |
| `fetch.sh` | Reads `manifest.env`, shallow-clones every repo into `src/<name>` (or checks out the named tag), then writes the resolved commit SHA into `pins.env`. Accepts optional name arguments to fetch single packages. Exits 2 with a failure list if any clone fails. |
| `manifest.env` | 77-line package table, columns `name|github-owner/repo|ref(- means default branch, otherwise a tag)|purpose`. This is the bill of materials; edit it to add dependencies. |

### Build scripts (each defines `s_<stage>()` functions and a `stage name s_name`
### dispatch; call `bash <script> <stage>` to build one piece, or `all`)

| File | Stages, in order | What it produces |
|---|---|---|
| `build_stack.sh` | pkgconf → shims → **syslinks** → zlib → libpng → expat → freetype → harfbuzz → xorgproto → xtrans → libxau → libxdmcp → xcbproto → libxcb → libx11 → xmacros → extlibs → gperf → pixman → fontenc → xfont → xkbfile → fontconfig → xft → xkbdata → xkbcomp → xauth → xbitmaps → libmd → **xvfb** → xutils → xautomation → xxf86vm (33 stages) | The entire X11 client stack plus the Xvfb virtual server, fonts, keyboard data, and xautomation input-injection tooling. `syslinks` exposes the host's libncursesw/libtinfo runtimes through `local/lib` via absolute symlinks (the base image lacks unversioned `.so` dev links). |
| `build_apps.sh` | m4 → autoconf → automake → libtool → openssl → fltk13 → mbedtls → dillo → libjpeg → libvnc → x11vnc → vim (12 stages) | The autotools toolchain itself, FLTK + the dillo browser, libvncserver + x11vnc, vim 9.2 (in-source build; vim has no VPATH support). Must run before build_openbox because Openbox's bootstrap needs autoreconf/libtoolize. |
| `build_suckless.sh` | ncurses → fonts → dwm → st → dmenu | ncurses (`--without-tests`), DejaVu TTFs extracted from matplotlib + a fontconfig local.conf, and the three suckless tools. dwm is the fallback window manager. |
| `build_openbox.sh` | libffi → pcre2 → glib → fribidi → pango → libxml2 → gettext → openbox → openbox_config (9 stages) | The full glib/pango/gettext dependency chain and **Openbox 3.7**, the required WM. Notable internals: pcre2 built shared; glib/pango/libxml2 via meson with most optional features off; pango uses the Xft backend (no cairo); a real `autopoint` is assembled from the gettext source template plus its prebuilt archive; the final stage installs `config/openbox/*` into `local/etc/xdg/openbox/` and live-reconfigures a running Openbox. |
| `build_extras.sh` | bison → xkbcommon → glvnd | Blender-path-only libraries. bison 3.0.4 comes from a release-tarball GitHub mirror (pregenerated parsers, no flex needed) with the fseterr glibc patch; xkbcommon accepts bison 3.0 after dropping the ≥3.6 version gate and renaming `parse.error detailed`→`verbose`; glvnd provides libGL.so.1 with GLX enabled. |
| `build_xft.sh` | single-purpose | Generates Xft.h from its template and drops the hand-written `xft/meson.build` into the libXft source (upstream libXft ships autotools only). Invoked from build_stack's xft stage. |
| `install_blender.sh` | — | `pip install bpy==4.2.0` into the venv and prints its version. Takes an optional version argument. |

### Service scripts

| File | Contents |
|---|---|
| `start_desktop.sh` | Kills any old desktop, starts Xvfb on a display arg (default `:99`, geometry `1280x800x24`), applies the US keymap, then starts a window manager — third arg `openbox` (default) or `dwm`, auto-fallback to dwm if openbox is missing. Writes PIDs to `run/`. |
| `stop_desktop.sh` | Kills the WM and Xvfb for a display via PIDs and pkill (also takes down anything attached, including x11vnc). |
| `start_tablet.sh` | Long-running (blocks). Starts x11vnc bound to **127.0.0.1:5900 only** (no VNC password, loopback restriction is the control), generates `run/tablet.cred` (user `arena`, random 12-char password) on first run, then execs websockify on `0.0.0.0:6080` with HTTP Basic auth proxying to loopback VNC and serving the noVNC web client. |

### Small support files

| File | Contents |
|---|---|
| `patches/bison-fseterr-glibc.patch` | Unified diff making old gnulib's fseterr.c compile on modern glibc (≥2.32 hides `_IO_ftrylockfile`). Applied automatically via apply_patch. |
| `patches/libxrender.patch` | Fixes a missing semicolon in the libXrender source used by this stack. |
| `shims/makeinfo`, `shims/help2man` | Tiny shell stand-ins that touch the requested output file, so packages with documentation make rules build without texinfo/help2man installed. |
| `shims/xgettext`, `shims/msgfmt` | gettext stand-ins (empty catalog outputs) for packages whose NLS refresh rules would otherwise fail. The Openbox chain later installs a *real* gettext/autopoint. |
| `xft/meson.build` | Hand-written Meson build definition for libXft 2.3.2 (see build_xft.sh). |

## B.3 `control/` — the AI's hands and eyes (3 files)

| File | Contents |
|---|---|
| `vmc.py` | **The main control surface.** Python, python-xlib, XTEST injection. Subcommands: `info, shot, windows, clients, focus, close, closeall, move, mousedown, mouseup, click, doubleclick, scroll, key, type, launch, wait-title, exec, keysyms`. Full reference in Part D. Every GUI action you take goes through here. |
| `shoot.py` | Standalone one-shot screenshot (`python shoot.py out.png`) using mss; used by acceptance test T4. |
| `desktop_bg.py` | Paints the X root window a solid slate color via python-xlib (there is no xsetroot in the toolchain). Called from the Openbox autostart. |

## B.4 `config/openbox/` — the tracked desktop configuration (3 files)

| File | Contents |
|---|---|
| `rc.xml` | Openbox settings: Clearlooks theme, 4 desktops, keybinds (Alt+Return terminal, Alt+r dmenu, Alt+f ranger, Alt+e vim, Alt+b dillo, Alt+F4 close, Alt+Tab cycle, Alt+F11 maximize, Ctrl+Alt+arrows switch desktop), Alt-drag window move/resize, focus policy. |
| `menu.xml` | Right-click desktop root menu: launchers for st, ranger, vim, dillo, dmenu; window list submenus; Reconfigure/Exit. |
| `autostart` | Template with `@VM_ROOT@` placeholders (substituted at install time): runs desktop_bg.py and setxkbmap on session start, best-effort only. |

## B.5 `tests/` — evidence and validation (3 files)

| File | Contents |
|---|---|
| `acceptance.sh` | The 15-check acceptance matrix (T1–T14; T11 has two sub-checks). Writes `evidence/acceptance.txt`. Covers: Xvfb, Xlib, WM (Openbox verified via EWMH `_NET_WM_NAME`), screenshot, terminal launch, input injection, PIL screenshot loop, GUI-created file, dillo, tablet auth (expects HTTP 401 without creds), bpy install + Cycles render, UE5 doc, pin count, ranger. Prints a `PASS=x FAIL=y BLOCKED=z` summary. |
| `blender_smoke.py` | Headless Blender test: builds a cube + red sphere scene with camera/sun, renders a PNG with **Cycles on CPU** (EEVEE needs a GPU/EGL context that does not exist), saves and reopens the .blend and asserts both objects survive. Optional args: output PNG path, output blend path. |
| `collect_env.sh` | Hardware/OS evidence collector (CPU, RAM, disk, /dev/dri, GL libs, distro, toolchain, virtualization hints) used to back feasibility documents. |

## B.6 `docs/` — the written record (5 files)

| File | Contents |
|---|---|
| `STATUS.md` | Current build status, latest acceptance matrix, what works, known limitations, validation history. Read this after SKILL.md for today's state. |
| `architecture.md` | Component diagram and design rationale of the whole stack. |
| `agent-control.md` | How and why the AI controls the desktop (XTEST, vmc.py design). |
| `bison-source-notes.md` | Dead-end analysis of bison source options and why the release mirror won. |
| `ue5-feasibility.md` | Evidence-backed conclusion that Unreal Engine 5 is infeasible here (no GPU/Vulkan/Windows host, unreachable installer) with the best free alternatives. |

## B.7 `evidence/` — durable results (only text is tracked)

`acceptance.txt` (latest run) and `cold-recovery-acceptance.txt` (the
clean-checkout validation run) are committed. PNG/JPG/log/blend outputs are
git-ignored to keep the repo small but are regenerated on every test run.

## B.8 Generated, untracked directories you will see after building

| Dir | Meaning | Safe to delete? |
|---|---|---|
| `local/` | The install prefix: all binaries (`local/bin`), libraries (`local/lib`), the Python venv (`local/venv`), configs (`local/etc`), fonts, themes, noVNC web root, terminfo. | Yes — recover.sh rebuilds it (takes the full build time). |
| `src/` | The ~77 shallow source checkouts. | Yes — fetch.sh re-clones. |
| `build/` | Out-of-tree build directories and `build/markers/` (one empty file per completed stage — the resumption mechanism). | Yes, but deleting `markers/` forces full rebuilds. |
| `run/` | Runtime PIDs, logs, `.Xauthority`, `tablet.cred`. | Yes; services recreate it. |

---

# PART C — From-scratch setup: exactly what to do

## C.1 Get the code

```bash
cd ~   # or wherever you work
git clone https://github.com/mhammedamazil-hub/testtemp.git
cd testtemp
git checkout arena/e86028ee-testtemp
cd arena-vm
ls scripts/recover.sh SKILL.md        # confirms you are in the right place
```

If the sandbox was reset but the clone still exists, skip cloning and
re-sync instead:

```bash
cd /home/user/testtemp
git fetch origin arena/e86028ee-testtemp
git reset --hard origin/arena/e86028ee-testtemp
cd arena-vm
```

## C.2 Build and start — choose one variant

```bash
# Core desktop only (Xvfb + Openbox + st/dmenu/dillo/vim/ranger + agent tools)
bash scripts/recover.sh

# Core + authenticated tablet bridge
bash scripts/recover.sh --tablet

# Everything including Blender (adds ~520 MB wheel download)
bash scripts/recover.sh --tablet --blender
```

Expect 30–60 minutes for a cold core build (mostly X server and glib
compilation), longer with Blender. Output is verbose; a failure ends the
script with a red `[FAIL] stage <name> failed` line. Re-running the same
command after a fix resumes from the failed stage — nothing completed is
rebuilt.

What success looks like (final lines):

```
[VM] starting window manager: openbox
[VM] desktop ready on DISPLAY=:99 (WM=openbox)
  X.Org version: 21.1.16
    dimensions:    1280x800 pixels ...
[VM] recovery complete. Run tests/acceptance.sh to validate.
```

## C.3 Validate

```bash
source scripts/env.sh
bash tests/acceptance.sh
```

- Full stack target: **`PASS=15 FAIL=0 BLOCKED=0`**.
- Core-only run: 12 PASS, with T10 (tablet) and T11 (Blender) as
  NOT_TESTED — those services are opt-in, not failures.
- Persisted copy: `evidence/acceptance.txt`.

## C.4 Start the tablet bridge if you skipped `--tablet`

It is long-running, so background it (or use the platform's process
manager); never run it in a foreground call that must return:

```bash
mkdir -p run
setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 < /dev/null &
sleep 4
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:6080/        # 401 = auth enforced
cat run/tablet.cred                                                      # username:password
curl -s -o /dev/null -w '%{http_code}\n' -u "$(cat run/tablet.cred)" http://127.0.0.1:6080/  # 200
```

## C.5 (Optional) Blender after a core-only build

```bash
bash scripts/build_extras.sh all        # bison, libxkbcommon, libglvnd
bash scripts/install_blender.sh 4.2.0
source scripts/env.sh
local/venv/bin/python tests/blender_smoke.py evidence/blender.png evidence/blender.blend
# expect: BLENDER_SMOKE_OK ... and a non-black PNG (open it to confirm)
```

## C.6 Orientation commands when you inherit an unknown state

```bash
cd /home/user/testtemp/arena-vm
git log --oneline -5                         # code state
ls build/markers 2>/dev/null | wc -l         # how many build stages are complete
pgrep -af "Xvfb :99|bin/openbox" | grep -v pgrep    # is the desktop running?
ss -ltn | grep -E '5900|6080'                # tablet services (5900 loopback, 6080 auth)
source scripts/env.sh && bash tests/acceptance.sh    # ground truth of what works
cat docs/STATUS.md                           # human-readable status
```

---

# PART D — Operating the virtual desktop

## D.1 Environment prologue for every GUI command

```bash
cd /home/user/testtemp/arena-vm
source scripts/env.sh
export DISPLAY=:99
export XAUTHORITY="$PWD/run/.Xauthority"
PY=local/venv/bin/python
```

## D.2 vmc.py full command reference

`$PY control/vmc.py <command> [args]`

| Command | Description |
|---|---|
| `info` | Screen geometry and current pointer coordinates |
| `shot [path.png]` | Screenshot; prints the path |
| `windows` | All viewable X windows including frame internals (id, WxH+X+Y, title) |
| `clients` | EWMH application windows only with UTF-8 titles — **use this for app discovery** |
| `focus <hex_id>` | Raise and focus a window |
| `close <hex_id>` | Graceful close via WM_DELETE_WINDOW |
| `closeall` | Close every managed client — call first to make tests deterministic |
| `move <x> <y>` | Warp the pointer |
| `mousedown [btn]` / `mouseup [btn]` | Hold/release button (1 left, 2 middle, 3 right) |
| `click [btn] [x] [y]` | Click; with coordinates it moves first |
| `doubleclick [x] [y]` | Left double-click |
| `scroll <n>` | Wheel; positive scrolls down, negative up |
| `key <keysym> [...]` | Key chords/presses: `key Return`, `key Tab`, `key ctrl d`, `key alt Return`, `key shift a` |
| `type "text"` | Literal string as keystrokes (handles punctuation via a keysym map) |
| `launch <prog> [args]` | Start a detached GUI program on the display; prints its pid |
| `wait-title <substr> [secs]` | Poll for a window title; prints its hex id or TIMEOUT (exit 1) |
| `exec "<shell>"` | Replace the process with a bash -c command |
| `keysyms` | List available key names |

## D.3 Cookbook A — launch a terminal, type, verify the result

```bash
$PY control/vmc.py closeall; sleep 1
$PY control/vmc.py launch local/bin/st -e bash -c "cat > /tmp/out.txt"
sleep 2
# st retitles itself to the running command, so the window is titled "bash"
WID=$($PY control/vmc.py clients | awk '$3=="bash"{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py type "hello from the agent"
$PY control/vmc.py key ctrl d
cat /tmp/out.txt        # hello from the agent
```

## D.4 Cookbook B — browser and a web-style form (network-free)

External sites are blocked, so demonstrate dillo with a local page:

```bash
cat > /tmp/page.html <<'EOF'
<!doctype html><html><body>
<label>Search:</label><input type="text" name="q" size="40">
<button>Search</button></body></html>
EOF
$PY control/vmc.py launch local/bin/dillo file:///tmp/page.html
sleep 3
WID=$($PY control/vmc.py clients | awk '/Dillo/{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py key Tab                 # focus the input (see quirk below)
$PY control/vmc.py type "what is meant by physics"
$PY control/vmc.py shot evidence/form.png  # open the PNG to verify visually
```

**Known quirk:** synthetic clicks do not reliably focus dillo form fields.
Focus the window, use `Tab` to reach the field, then `type`. Terminals
accept typing immediately after `focus`.

## D.5 Cookbook C — the see → act → verify feedback loop

```bash
$PY control/vmc.py shot evidence/1.png          # inspect the screen yourself
$PY control/vmc.py move 400 300
$PY control/vmc.py click 1
$PY control/vmc.py shot evidence/2.png          # confirm the click's effect
```

## D.6 Openbox keybinds (from config/openbox/rc.xml)

Alt+Return terminal · Alt+r dmenu · Alt+f ranger in st · Alt+e vim in st ·
Alt+b dillo · Alt+F4 close · Alt+Tab / Alt+Shift+Tab cycle windows ·
Alt+space client menu · Alt+F11 maximize · Alt+m minimize ·
Ctrl+Alt+Left/Right switch desktop · hold Alt + left-drag moves a window,
Alt + right-drag resizes · right-click desktop opens the launcher menu.

## D.7 Running programs without injection

```bash
source scripts/env.sh
local/bin/openbox --version | head -1
local/bin/vim --version | head -1
TERM=xterm-256color local/venv/bin/ranger .                 # needs a real TERM + TERMINFO
local/venv/bin/python -c "import bpy; print(bpy.app.version_string)"
```

---

# PART E — Maintenance, repair, and extension

## E.1 The marker system and resuming a failed build

Each successful stage creates `build/markers/<stage>`. On re-run, existing
markers skip the stage. To rebuild one stage:

```bash
rm -f build/markers/pango
bash scripts/build_openbox.sh pango          # stage name = function minus s_
# then re-run the master to continue and restart services:
bash scripts/recover.sh
```

Stage-to-script map: build_stack (X11/Xvfb), build_apps (toolchain/dillo/vim),
build_suckless (ncurses/dwm/st/dmenu), build_openbox (glib chain + WM),
build_extras (Blender libs).

## E.2 Adding a new source dependency

1. Add `name|owner/repo|ref|purpose` to `scripts/manifest.env` (`-` = default
   branch; a tag string pins a release).
2. `bash scripts/fetch.sh name` — clones it and appends `PIN_name=<sha>` to
   pins.env.
3. Add `s_name()` to the relevant build script using a helper from env.sh:
   `conf_build "$VM_SRC/name"` (autotools), `meson_build "$VM_SRC/name" -Dopt=…`,
   `cmake_build "$VM_SRC/name" -DFLAG=…`, or `make_build "$VM_SRC/name"`.
4. Add it to that script's `all)` dispatch in dependency order.
5. Build, then extend/run `tests/acceptance.sh`. Commit the manifest, pins,
   and script changes.

## E.3 Service lifecycle

```bash
bash scripts/stop_desktop.sh :99
bash scripts/start_desktop.sh :99 1280x800x24            # Openbox
bash scripts/start_desktop.sh :99 1280x800x24 dwm        # fallback WM
# A desktop restart kills x11vnc (it attaches to the old Xvfb): relaunch:
setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 < /dev/null &
```

Health checks:

```bash
pgrep -af "Xvfb :99|bin/openbox|x11vnc|websockify" | grep -v pgrep
ss -ltn | grep -E '5900|6080'     # 5900 must be 127.0.0.1 only; 6080 is 0.0.0.0 + auth
```

## E.4 Full disaster recovery (everything deleted)

This is the tested scenario: Git still holds all 41 tracked files. Run
Part C.1 (re-sync), then C.2 (`recover.sh`). A clean-worktree cold build has
been validated end-to-end at PASS=15; two ordering/portability bugs it once
exposed are fixed in the current scripts, so a fresh run should pass — if it
does not, follow E.1 and the table below.

## E.5 Troubleshooting table (real failures, already diagnosed)

| Symptom | Root cause | Remedy |
|---|---|---|
| `autoreconf: command not found` in build_openbox | autotools are built by build_apps | Keep recover.sh order: apps → suckless → openbox |
| `cannot find -lncursesw` / `-ltinfo` | Host image has versioned runtime `.so.6` but no dev symlink | `build_stack.sh syslinks` creates absolute symlinks in local/lib |
| vim configure: `tgetent() NOT FOUND` | Debian splits tgetent into libtinfo; not linked transitively | s_vim picks `--with-tlib=tinfo` when local/lib/libtinfo.so exists |
| ncurses `DSO missing from command line` in test/ | Test binaries assume transitive tinfo linkage | ncurses configured `--without-tests` |
| bison `#error Please port gnulib fseterr.c` | glibc ≥2.32 hides `_IO_ftrylockfile` | bison-fseterr-glibc.patch via apply_patch (idempotent) |
| bison VCS bootstrap fails (flex/autopoint/gnulib) | flex is unobtainable inside the allowlist | Use bison 3.0.4 release mirror; see docs/bison-source-notes.md |
| xkbcommon: `bison found 3.0.4 but need >=3.6` | Hard version gate; grammar is compatible | Gate removed; `parse.error detailed`→`verbose` in build_extras.sh |
| pkgconf `Dependency xft not found` | pkgconf 3.x is case-sensitive; file is Xft.pc | s_pango creates a `xft.pc` symlink |
| meson "not boolean / unknown option" | Feature vs boolean option typing | Use `enabled/disabled/auto` for feature opts, `true/false` for booleans |
| autopoint substitution produced an empty file | Multi-line version string broke a sed expression | s_gettext pipes `--version | head -1` and guards with `[ -s ]` |
| ranger `setupterm: could not find terminfo` | Needs local ncurses DB and a real TERM | env.sh exports TERMINFO; run with `TERM=xterm-256color` |
| bpy `libxkbcommon.so.0 cannot open` then `libGL.so.1` | Wheel dlopens system-style libs at runtime | env.sh LD_LIBRARY_PATH; glvnd built with `-Dglx=enabled` |
| Blender EEVEE `Couldn't open libEGL` | No GPU/EGL in the sandbox | Use Cycles CPU (blender_smoke.py already does); EEVEE is infeasible |
| dillo "Could not establish connection" | Egress allowlist blocks external sites | Expected; demo with file:// pages, never claim web access works |
| Port 6080 returns 401 | HTTP Basic auth challenge | Correct behavior; 200 only with `-u "$(cat run/tablet.cred)"` |
| Form field ignores typed text in dillo | XTEST click doesn't focus the field | Focus window, `key Tab` into the field, then `type` |
| make tries to regenerate configure and fails | Git clones have uniform mtimes | pin_timestamps normalizes source mtimes to 2020-01-01 |
| All binaries gone after a sandbox reset | local/build/src are not persisted | re-sync Git (C.1) and rerun recover.sh (C.2) |
| Tablet bridge dies after desktop restart | x11vnc binds to a specific Xvfb process | Rerun start_tablet.sh after start_desktop.sh |
| `stage x: command not found` with hyphenated names | Dispatch builds `s_$1` from the argument | Use underscore stage names (e.g. `openbox_config`); aliases are wired where needed |

## E.6 What "done" looks like — the acceptance test IDs

T1 Xvfb alive · T2 Xlib connects · T3 Openbox EWMH owner · T4 screenshot ·
T5 st terminal mapped · T6 injected keystrokes recorded · T7 PIL parses
screenshots · T8 file created through the GUI · T9 dillo renders ·
T10 tablet 401-without-creds · T11 bpy import + Cycles render (two checks) ·
T12 UE5 feasibility document exists · T13 pin ledger present ·
T14 ranger runs. Evidence lands in `evidence/acceptance.txt` plus PNGs.

---

# PART F — Rules that must never be broken

1. Spend ₹0. FOSS and the three allowed hosts only; document blockers rather
   than routing around them.
2. No external AI/LLM service — you are the sole controller.
3. The desktop must never be reachable without authentication: x11vnc stays
   loopback-only, only the Basic-auth websockify faces the network.
4. Keep credentials and large/binary artifacts out of Git (the .gitignore
   already encodes this; don't fight it).
5. Every "it works" claim needs a green acceptance check or a real
   screenshot/log. Infeasible features get an evidence-backed NOT_TESTED or
   BLOCKED write-up (model: docs/ue5-feasibility.md).
6. Commit small, reversible, tested changes on `arena/e86028ee-testtemp`;
   record exact source SHAs in pins.env whenever you add a package.

## If you remember nothing else

```bash
cd /home/user/testtemp/arena-vm
git fetch origin arena/e86028ee-testtemp && git reset --hard origin/arena/e86028ee-testtemp
bash scripts/recover.sh --tablet           # rebuild + start, resumable
source scripts/env.sh && bash tests/acceptance.sh    # prove it works
```
