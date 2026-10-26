# SKILL: Operate the Arena virtual graphical Linux computer

> A complete, self-contained operating guide. It assumes you know **nothing**
> about this project. Follow the sections in order. Every command is
> copy-paste ready. All paths below are inside the project directory
> `/home/user/testtemp/arena-vm` (git root: `/home/user/testtemp`).

---

## 0. What this thing is

A fully functional Linux desktop computer built **entirely from FOSS source
code** inside the sandbox:

- **Xvfb** — a virtual X11 display server (no physical monitor), display `:99`,
  screen `1280x800x24`.
- **Openbox** — the window manager (dwm is kept as a fallback).
- **st** terminal, **dmenu** launcher, **dillo** browser, **vim** editor,
  **ranger** file manager.
- **Blender 4.2** (optional, CPU rendering via the `bpy` wheel).
- **You** (the AI) control it through `control/vmc.py`, which injects real
  keyboard/mouse events via the XTEST extension — applications cannot tell
  this from a human.
- A human can watch/control the same desktop from a tablet browser through an
  authenticated noVNC bridge (port 6080, password required).

Everything is installed into a local prefix `arena-vm/local/` — no system
package installation, no money, no external AI.

---

## 1. Hard environment rules (read before doing anything)

1. **Network egress is restricted to exactly three hosts:** `github.com`,
   `pypi.org`, `registry.npmjs.org` (plus their CDNs). You cannot reach
   google.com, ftp.gnu.org, apt mirrors, or any other site. Design around it.
2. **The sandbox resets.** Directories `local/`, `build/`, `src/`, `run/` are
   git-ignored build artifacts and can disappear at any time. Git-tracked
   files survive. Never keep anything irreplaceable outside Git.
3. **Zero budget:** FOSS only. No paid APIs, tunnels, domains, or cloud VMs.
4. **Never commit credentials.** Tablet passwords live only in
   `run/tablet.cred` (the `run/` directory is git-ignored).
5. **Never switch git branches.** Work only on `arena/e86028ee-testtemp`.
6. **Never fake evidence.** If something is blocked, say BLOCKED and show why.

---

## 2. FAST PATH — from an empty/reset sandbox to a working desktop

Do exactly these steps. A full cold build takes roughly 30–60 minutes;
re-running is fast because every stage is marker-gated and skips completed
work.

### Step 1 — Restore the code if the checkout is stale

```bash
cd /home/user/testtemp
git fetch origin arena/e86028ee-testtemp
git reset --hard origin/arena/e86028ee-testtemp
cd arena-vm && ls scripts/recover.sh docs/STATUS.md   # sanity check
```

### Step 2 — Build everything and start the desktop (one command)

```bash
cd /home/user/testtemp/arena-vm
bash scripts/recover.sh                # core desktop only
# or, for the full stack including tablet access and Blender:
bash scripts/recover.sh --tablet --blender
```

What it does, in order (each phase is resumable):

| Phase | Script | Output |
|---|---|---|
| 1 | venv + `requirements.txt` | meson, ninja, cmake, python-xlib, ranger, … in `local/venv/` |
| 2 | `scripts/fetch.sh` | shallow-clones ~80 pinned repos into `src/`, SHAs recorded in `pins.env` |
| 3 | ldconfig | registers `local/lib` with the dynamic loader (passwordless sudo) |
| 4 | `build_stack.sh all` | X11/Xvfb stack, fonts, base libs, ncurses symlinks |
| 5a | `build_apps.sh all` | autoconf/automake/libtool, openssl, FLTK, dillo, vim, x11vnc |
| 5b | `build_suckless.sh all` | ncurses, dwm, st, dmenu, fonts |
| 5c | `build_openbox.sh all` | libffi→pcre2→glib→fribidi→pango→libxml2→gettext→**Openbox** + its config |
| 5d | `build_extras.sh all` (only with `--blender`) | bison, libxkbcommon, libglvnd |
| 5e | `install_blender.sh` (only with `--blender`) | `bpy==4.2.0` wheel into the venv |
| 6 | `start_desktop.sh` | Xvfb :99 + Openbox live |

### Step 3 — Validate

```bash
cd /home/user/testtemp/arena-vm
source scripts/env.sh
bash tests/acceptance.sh
```

Expected last line with the full stack: **`PASS=15 FAIL=0 BLOCKED=0`**
(core-only run shows 12 PASS + 2 NOT_TESTED for the opt-in tablet/Blender
checks). Results are also written to `evidence/acceptance.txt`.

### Step 4 — (if not using --tablet) start the tablet bridge manually

```bash
# long-running: launch it as a background process, not a plain bash call
setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 &
sleep 3
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:6080/        # 401 = auth working
cat run/tablet.cred                                                      # username:password
```

x11vnc binds to loopback `127.0.0.1:5900` only; websockify exposes
`0.0.0.0:6080` with HTTP Basic auth. A 401 without credentials is the
**correct, passing** security behavior.

---

## 3. Controlling the desktop (the actual "skill")

### 3.1 Required environment for every GUI command

```bash
cd /home/user/testtemp/arena-vm
source scripts/env.sh                     # PATH, LD_LIBRARY_PATH, TERMINFO, PKG_CONFIG paths
export DISPLAY=:99
export XAUTHORITY="$PWD/run/.Xauthority"
PY=local/venv/bin/python
```

`scripts/env.sh` provides: `$VM_ROOT`, `$VM_SRC`, `$VM_BUILD`,
`$VM_PREFIX` (= `…/local`), `$VM_VENV`, plus rpath/linker flags and helper
functions (`conf_build`, `meson_build`, `cmake_build`, `make_build`,
`apply_patch`, `pin_timestamps`).

### 3.2 vmc.py command reference

Run: `$PY control/vmc.py <command> [args]`

| Command | Purpose |
|---|---|
| `info` | screen size and current pointer position |
| `shot [file.png]` | screenshot (prints the path); also `control/shoot.py file.png` |
| `windows` | every viewable X window (id, geometry, title) — includes frame internals |
| `clients` | **EWMH top-level application windows only** (id, geometry, UTF-8 title) — prefer this |
| `focus <hex_id>` | raise and focus a window |
| `close <hex_id>` | send WM_DELETE_WINDOW (graceful quit) |
| `closeall` | close every managed client — use at the start of deterministic tests |
| `move <x> <y>` | warp the pointer |
| `mousedown [btn]` / `mouseup [btn]` | button 1=left 2=middle 3=right |
| `click [btn] [x] [y]` | click; with x/y it moves first |
| `doubleclick [x] [y]` | left double-click |
| `scroll <n>` | wheel; positive = down, negative = up |
| `key <keysym> [more...]` | e.g. `key Return`, `key ctrl shift t`, `key alt Return`, `key Tab`, `key F4` |
| `type "arbitrary text"` | type a literal string as keystrokes |
| `launch <program> [args]` | start a GUI app detached on the display |
| `wait-title <substr> [secs]` | poll until a window title appears; prints hex id or TIMEOUT |
| `keysyms` | list available key names |

### 3.3 Cookbook — common tasks

**Launch a terminal and type into it (reliable pattern):**

```bash
$PY control/vmc.py closeall; sleep 1
$PY control/vmc.py launch local/bin/st -e bash -c "cat > /tmp/out.txt"
sleep 2
WID=$($PY control/vmc.py clients | awk '$3=="bash"{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py type "hello from the agent"
$PY control/vmc.py key ctrl d          # ends cat, flushes the file
cat /tmp/out.txt                        # -> hello from the agent
```

Note: **st retitles itself to the running command** — a shell window is
titled `bash`, not `st`. After `closeall`, the only client is yours, so
matching the first client row is also valid.

**Open the browser on a local page and fill a form (dillo):**

```bash
cat > /tmp/page.html <<'EOF'
<!doctype html><html><body><input type="text" name="q" size="40"></body></html>
EOF
$PY control/vmc.py launch local/bin/dillo file:///tmp/page.html
sleep 3
WID=$($PY control/vmc.py clients | awk '/dillo|Dillo/{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py key Tab             # move keyboard focus INTO the input
$PY control/vmc.py type "what is meant by physics"
$PY control/vmc.py shot evidence/form.png
```

> **dillo focus quirk:** a synthetic mouse click does not reliably focus a
> web form field. Focus the window, press `Tab` until the field is active,
> then `type`. Terminals (st) accept typing immediately after `focus`.

**Screenshot feedback loop (see → decide → act):**

```bash
$PY control/vmc.py shot evidence/step1.png     # then read the PNG yourself
$PY control/vmc.py move 400 300
$PY control/vmc.py click 1
$PY control/vmc.py shot evidence/step2.png     # verify the effect visually
```

**Window manager keybinds (configured in `config/openbox/rc.xml`):**
`Alt+Return` terminal · `Alt+r` dmenu · `Alt+f` ranger · `Alt+e` vim ·
`Alt+b` dillo · `Alt+F4` close · `Alt+Tab` cycle · `Alt+F11` maximize ·
right-click desktop = launcher menu (`config/openbox/menu.xml`).

---

## 4. Running applications directly (no GUI injection needed)

```bash
source scripts/env.sh
local/bin/openbox --version | head -1
local/bin/vim --version | head -1
local/bin/dillo file:///tmp/page.html &        # GUI; needs DISPLAY
TERM=xterm-256color local/venv/bin/ranger .     # file manager (needs a real TERM)
local/venv/bin/python -c "import bpy; print(bpy.app.version_string)"   # Blender check
local/venv/bin/python tests/blender_smoke.py evidence/render.png evidence/render.blend
```

The Blender smoke test builds a scene and renders with **Cycles on CPU**.
EEVEE cannot work — it requires an EGL/GPU context and the sandbox has no
GPU. The missing `libEGL` message is expected and harmless on the Cycles path.

---

## 5. Build system internals (for repairs and additions)

### Layout

```
arena-vm/
├── scripts/
│   ├── env.sh            # sourced by every build script; paths + helpers
│   ├── manifest.env      # name|github-repo|ref(- = default)|purpose, ~80 entries
│   ├── fetch.sh          # clones manifest repos, writes exact SHAs to pins.env
│   ├── pins.env          # PIN_<name>=<full commit SHA> — the reproducibility record
│   ├── recover.sh        # master orchestrator (the only entry point you need)
│   ├── build_stack.sh    # X11/Xvfb + base libs (autotools/meson/cmake)
│   ├── build_apps.sh     # autotools toolchain + dillo/vim/x11vnc
│   ├── build_suckless.sh # ncurses, dwm/st/dmenu, fonts
│   ├── build_openbox.sh  # glib/pango/gettext/openbox chain + config install
│   ├── build_extras.sh   # bison/xkbcommon/glvnd (Blender path)
│   ├── install_blender.sh
│   ├── start_desktop.sh  # Xvfb + WM; 3rd arg selects openbox|dwm
│   ├── stop_desktop.sh
│   ├── start_tablet.sh   # x11vnc loopback + authenticated websockify (blocks)
│   ├── patches/          # unified diffs applied idempotently via apply_patch
│   └── shims/            # makeinfo/help2man/xgettext stand-ins
├── control/vmc.py        # the agent's GUI control surface
├── control/shoot.py      # standalone screenshot
├── control/desktop_bg.py # paints root window (python-xlib; no xsetroot needed)
├── config/openbox/       # tracked rc.xml, menu.xml, autostart
├── tests/acceptance.sh   # 15-check evidence suite
├── tests/blender_smoke.py
├── docs/                 # STATUS.md, architecture, UE5 feasibility, bison notes
└── evidence/             # screenshots and acceptance logs (media git-ignored, .txt tracked)
```

### Markers and resuming

Every successful stage touches `build/markers/<stage>`. If a build fails:

```bash
# fix the problem, then re-run just that stage by name:
bash scripts/build_openbox.sh pango          # markers for other stages skip
# if a stage wrote a marker despite being broken, delete it first:
rm -f build/markers/pango && bash scripts/build_openbox.sh pango
# stage name = function name minus the s_ prefix (s_pango -> pango)
```

Then re-run `bash scripts/recover.sh` — it resumes from the first missing
marker and restarts services.

### Adding a new source dependency

1. Add a line to `scripts/manifest.env`: `name|owner/repo|ref|purpose`
   (`-` means default branch; a tag name pins a release).
2. `bash scripts/fetch.sh name` — clones it and appends `PIN_name=<sha>`.
3. Add an `s_name()` build function and a `stage name s_name` line to the
   appropriate build script, using the `conf_build` / `meson_build` /
   `cmake_build` / `make_build` helpers from env.sh.
4. Build it, then run `bash tests/acceptance.sh`.

### Desktop service lifecycle

```bash
bash scripts/stop_desktop.sh :99             # kills Xvfb + WM (also kills x11vnc)
bash scripts/start_desktop.sh :99 1280x800x24           # restart with Openbox
bash scripts/start_desktop.sh :99 1280x800x24 dwm       # fallback WM
# after a desktop restart, relaunch the tablet bridge (it attaches to the new Xvfb):
setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 &
```

Check health:

```bash
pgrep -af "Xvfb :99|bin/openbox|x11vnc|websockify" | grep -v pgrep
ss -ltn | grep -E "5900|6080"                 # 5900 loopback only, 6080 all interfaces
```

---

## 6. Troubleshooting — real failures already hit, and their fixes

| Symptom | Cause | Fix (already in scripts; here for diagnosis) |
|---|---|---|
| `autoreconf: command not found` in build_openbox | autotools built by build_apps must run first | recover.sh ordering: build_apps → suckless → openbox. Don't reorder. |
| `cannot find -lncursesw` | base image has runtime `.so.6` but no unversioned dev symlink | `build_stack.sh s_syslinks` symlinks system runtimes into `local/lib` |
| vim configure: `tgetent() NOT FOUND` | Debian puts tgetent in libtinfo, not pulled transitively | vim uses `--with-tlib=tinfo` when `local/lib/libtinfo.so` exists |
| ncurses make: `DSO missing from command line` in test/ | test binaries rely on transitive tinfo linking | ncurses configured `--without-tests` (tests aren't shipped) |
| bison `fseterr.c … #error portability trap` | modern glibc hides `_IO_ftrylockfile` | `scripts/patches/bison-fseterr-glibc.patch`, applied via `apply_patch` |
| bison VCS bootstrap fails (flex/gettext/gnulib) | flex unreachable; release mirror used instead | bison 3.0.4 from `tar-mirror/gnu-bison`; see `docs/bison-source-notes.md` |
| xkbcommon meson: `bison found 3.0.4 but need >=3.6` | version gate; grammar itself is compatible | gate removed in build_extras; `parse.error detailed`→`verbose` |
| `ranger: setupterm could not find terminfo` | curses needs the local terminfo DB and a real TERM | env.sh exports `TERMINFO=local/share/terminfo`; run with `TERM=xterm-256color` |
| bpy `libxkbcommon.so.0 cannot open` / `libGL.so.1` | wheel dlopens system-style libs at runtime | env.sh exports `LD_LIBRARY_PATH=local/lib`; glvnd built with glx enabled |
| dillo "Could not establish connection" | egress allowlist blocks all external sites | **expected.** Demonstrate the browser with `file://` pages instead |
| 401 from port 6080 | auth challenge | **correct behavior**; 200 only with `curl -u "$(cat run/tablet.cred)"` |
| Everything gone after sandbox reset | build dirs are not persisted | Section 2: fetch + `recover.sh` rebuilds from pins in one command |
| Tablet bridge dies when desktop restarts | x11vnc attaches to a specific Xvfb PID | rerun `start_tablet.sh` after `start_desktop.sh` |
| `make` tries to regenerate configure and fails | git clones flatten mtimes | `pin_timestamps` sets all source mtimes to 2020-01-01 so remake rules sleep |

---

## 7. Constraints that must never be violated

- No money changes hands: only FOSS, PyPI, and the three allowed hosts.
- The Arena agent is the only brain. Do not wire in external LLM APIs.
- The desktop-control port must never be open without authentication.
  x11vnc stays on loopback; only the auth-gated websockify faces outward.
- No credentials or large binaries in Git (`run/`, `local/`, `build/`,
  `src/`, `*.blend`, evidence media are all ignored).
- Every claim of "working" needs a passing acceptance check or a real
  screenshot in `evidence/`. Record blockers honestly as
  NOT_TESTED / BLOCKED with the reason (see `docs/ue5-feasibility.md` for
  the gold standard: UE5 is hard-blocked — no GPU, no Vulkan, no Windows).

## 8. First five commands to run when you wake up confused

```bash
cd /home/user/testtemp/arena-vm
git log --oneline -3                          # what state is the code in?
ls build/markers 2>/dev/null | wc -l          # how many stages are built?
pgrep -af "Xvfb :99|bin/openbox" | grep -v pgrep    # is the desktop live?
source scripts/env.sh && bash tests/acceptance.sh   # what actually works?
```

Then read `docs/STATUS.md` for the human-readable current state.
