#!/usr/bin/env bash
# Generates docs/FROM-ZERO-GUIDE.md: a fully self-contained, zero-clone
# bootstrap manual that embeds the COMPLETE verbatim content of every
# tracked file, plus the operational runbook (download apps, Blender,
# restarts, repairs). Run from arena-vm/:  bash scripts/gen_from_zero_guide.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="docs/FROM-ZERO-GUIDE.md"

# fence language hint per file type
hint() { case "$1" in
  *.sh|shims/*|autostart) echo bash ;;
  *.py) echo python ;;
  *.md) echo markdown ;;
  *.xml) echo xml ;;
  *.patch|*.diff) echo diff ;;
  *meson.build) echo meson ;;
  *.env|pins.env|*.cred|*.txt) echo text ;;
  *) echo text ;;
esac; }

purpose() { case "$1" in
  .gitignore) echo "Keeps every generated artifact (src/build/local/run, media, creds) out of Git." ;;
  requirements.txt) echo "Exact Python packages for the local venv (build tools, X control, screenshots, ranger, websockify)." ;;
  pins.env) echo "Reproducibility ledger: full commit SHA of all 80 source checkouts." ;;
  SKILL.md) echo "Compact zero-context operating guide (companion to this document)." ;;
  scripts/env.sh) echo "Sourced by every build script: path variables, compiler flags, helper functions." ;;
  scripts/fetch.sh) echo "Clones every manifest repo into src/ and records resolved SHAs in pins.env." ;;
  scripts/manifest.env) echo "Bill of materials: name|GitHub repo|ref|purpose for every source package." ;;
  scripts/recover.sh) echo "Master orchestrator: the single command that builds and starts everything." ;;
  scripts/build_stack.sh) echo "33-stage X11/Xvfb stack, base libraries, fonts and input-injection tools." ;;
  scripts/build_apps.sh) echo "Autotools toolchain, dillo, vim, libvnc/x11vnc and friends." ;;
  scripts/build_suckless.sh) echo "ncurses, fonts, dwm/st/dmenu." ;;
  scripts/build_openbox.sh) echo "libffi->pcre2->glib->fribidi->pango->libxml2->gettext->Openbox chain + config." ;;
  scripts/build_extras.sh) echo "Blender-only libs: bison, libxkbcommon, libglvnd." ;;
  scripts/build_xft.sh) echo "Hand-written meson wrapper for libXft (upstream is autotools-only)." ;;
  scripts/install_blender.sh) echo "Installs the bpy wheel (full Blender) into the venv." ;;
  scripts/start_desktop.sh) echo "Starts Xvfb and the window manager (Openbox default, dwm fallback)." ;;
  scripts/stop_desktop.sh) echo "Stops Xvfb and the window manager for a display." ;;
  scripts/start_tablet.sh) echo "Loopback x11vnc + authenticated websockify/noVNC bridge (long-running)." ;;
  scripts/patches/bison-fseterr-glibc.patch) echo "Makes old gnulib fseterr.c compile on modern glibc." ;;
  scripts/patches/libxrender.patch) echo "Missing-semicolon fix for the pinned libXrender source." ;;
  scripts/shims/makeinfo) echo "texinfo stand-in: touches the requested manual output." ;;
  scripts/shims/help2man) echo "help2man stand-in: touches the requested man page." ;;
  scripts/shims/xgettext) echo "gettext stand-in for NLS refresh rules." ;;
  scripts/shims/msgfmt) echo "gettext stand-in that creates empty .mo files." ;;
  scripts/xft/meson.build) echo "Hand-written Meson definition for libXft 2.3.2." ;;
  control/vmc.py) echo "THE agent control surface: XTEST keyboard/mouse injection, screenshots, windows." ;;
  control/shoot.py) echo "One-shot screenshot helper (mss)." ;;
  control/desktop_bg.py) echo "Paints the X root window background via python-xlib." ;;
  config/openbox/rc.xml) echo "Openbox theme, keybinds, mouse bindings and focus policy." ;;
  config/openbox/menu.xml) echo "Right-click desktop launcher menu." ;;
  config/openbox/autostart) echo "Session autostart template (@VM_ROOT@ substituted at install)." ;;
  tests/acceptance.sh) echo "The 15-check evidence suite; writes evidence/acceptance.txt." ;;
  tests/blender_smoke.py) echo "Headless Blender scene build + Cycles CPU render + .blend round-trip." ;;
  tests/collect_env.sh) echo "Hardware/OS evidence collector for feasibility documents." ;;
  docs/STATUS.md) echo "Living build status and acceptance history." ;;
  docs/architecture.md) echo "Component architecture and design rationale." ;;
  docs/agent-control.md) echo "How the AI drives the desktop (XTEST design)." ;;
  docs/bison-source-notes.md) echo "Dead-end analysis behind the bison source choice." ;;
  docs/ue5-feasibility.md) echo "Evidence-backed UE5 infeasibility conclusion." ;;
  evidence/acceptance.txt) echo "Latest committed acceptance run output." ;;
  evidence/cold-recovery-acceptance.txt) echo "Acceptance output from the clean-checkout cold rebuild." ;;
  scripts/gen_from_zero_guide.sh) echo "This generator: embeds all files verbatim into FROM-ZERO-GUIDE.md." ;;
  *) echo "(project file)" ;;
esac; }

section() { printf '\n---\n\n# %s\n\n' "$1" >> "$OUT"; }

cat > "$OUT" <<'PROSE0'
# FROM ZERO — the complete, self-contained bootstrap manual

> **Audience:** a brand-new AI in a brand-new session, with NO repository
> checkout available. This document contains the *complete verbatim content*
> of every project file, followed by the full runbook: how to recreate the
> directory tree, write every file, download every app, build the stack, run
> Blender, restart services, and repair failures. Nothing is paraphrased in
> the file sections — the code blocks are letter-for-letter copies.
>
> **How to use it:** Part 0 gives the assumptions; Part 1 the mental model;
> Part 2 the skeleton; Part 3 is the entire source tree (recreate files from
> these blocks); Parts 4–10 are operations and repair. If you DO have the
> Git repo available, you can skip the manual recreation and use the one-
> command path in Part 5 directly.
>
> Generated by `scripts/gen_from_zero_guide.sh`. Regenerate after any file
> change so the verbatim copies never drift.

# PART 0 — Starting assumptions

You begin with only a generic Linux sandbox (Debian-family, x86_64, GCC
toolchain, Python 3.11, git, curl, passwordless sudo for `ldconfig` only).
Nothing related to this project exists yet.

Hard rules of the environment:

1. **Outbound network reaches only `github.com`, `pypi.org`,
   `registry.npmjs.org`** (and their CDNs). No Google, no GNU FTP, no apt
   mirrors, no arbitrary sites. All "downloading" therefore means
   `git clone` from GitHub or `pip` from PyPI.
2. The sandbox may be wiped; only files you commit to Git are durable.
3. Zero budget — FOSS only, no external AI services, no paid tunnels.
4. No credentials in files that get committed.
5. All installs live under the project prefix `local/`; the only privileged
   operation is registering that library path with the dynamic loader.

Before doing anything, verify the base image:

```bash
git --version && gcc --version | head -1 && python3 --version
python3 -m venv /tmp/vprobe && rm -rf /tmp/vprobe && echo venv-ok
sudo -n true && echo sudo-ok
curl -sS -o /dev/null -w 'github %{http_code}\n' --max-time 15 https://github.com
curl -sS -o /dev/null -w 'pypi   %{http_code}\n' --max-time 15 https://pypi.org
df -h . | tail -1     # need ~8 GB free for the full stack including bpy
```

If the venv module is missing or either network probe fails, STOP and report
BLOCKED with the output — there is no in-rules workaround.
PROSE0

cat >> "$OUT" <<'PROSE1'

# PART 1 — Mental model (what you are building)

```
arena-vm/
├── scripts/   build orchestration, service lifecycle, patches, shims
├── control/   the AI's hands (vmc.py: inject real key/mouse events; shoot.py)
├── config/    Openbox window-manager configuration
├── tests/     acceptance suite + Blender smoke test + env collector
├── docs/      design and status documents
├── evidence/  test outputs (text tracked; media git-ignored)
├── src/       (generated) shallow clones of ~77 upstream repos
├── build/     (generated) out-of-tree builds + markers/ resume flags
├── local/     (generated) the install prefix: bin, lib, venv, etc, fonts…
└── run/       (generated) PIDs, logs, .Xauthority, tablet credentials
```

Runtime topology: **Xvfb** is a virtual monitor on display `:99`
(1280x800x24). **Openbox** is its window manager. GUI apps (st, dillo, vim,
ranger) draw into that virtual display. `control/vmc.py` injects XTEST
keyboard/mouse events and takes screenshots, closing the perception/action
loop. **x11vnc** mirrors the display on loopback port 5900; **websockify +
noVNC** expose it on port 6080 behind HTTP Basic auth for a tablet browser.
Blender runs headless through the `bpy` Python wheel and renders on CPU with
Cycles (no GPU exists, so EEVEE/EGL paths are impossible by design).

# PART 2 — Recreate the directory skeleton

```bash
mkdir -p arena-vm/{scripts/{patches,shims,xft},control,config/openbox,tests,docs,evidence}
cd arena-vm
mkdir -p src build/markers local run
```

Then create every file in Part 3 with the exact paths and contents shown.
After that, Part 5 builds the generated directories for you.
PROSE1

section "PART 3 — EVERY PROJECT FILE, COMPLETE VERBATIM CONTENT"

# logical groups; every tracked file except this generated guide is embedded
groups=(
  "TOP LEVEL|.gitignore requirements.txt pins.env SKILL.md"
  "SCRIPTS — ORCHESTRATION|scripts/env.sh scripts/fetch.sh scripts/manifest.env scripts/recover.sh"
  "SCRIPTS — BUILD ENGINES|scripts/build_stack.sh scripts/build_apps.sh scripts/build_suckless.sh scripts/build_openbox.sh scripts/build_extras.sh scripts/build_xft.sh scripts/install_blender.sh"
  "SCRIPTS — SERVICES|scripts/start_desktop.sh scripts/stop_desktop.sh scripts/start_tablet.sh"
  "SCRIPTS — PATCHES, SHIMS AND MISC|scripts/patches/bison-fseterr-glibc.patch scripts/patches/libxrender.patch scripts/shims/makeinfo scripts/shims/help2man scripts/shims/xgettext scripts/shims/msgfmt scripts/xft/meson.build scripts/gen_from_zero_guide.sh"
  "CONTROL SURFACE|control/vmc.py control/shoot.py control/desktop_bg.py"
  "OPENBOX CONFIGURATION|config/openbox/rc.xml config/openbox/menu.xml config/openbox/autostart"
  "TESTS|tests/acceptance.sh tests/blender_smoke.py tests/collect_env.sh"
  "DOCUMENTS|docs/STATUS.md docs/architecture.md docs/agent-control.md docs/bison-source-notes.md docs/ue5-feasibility.md"
  "EVIDENCE (COMMITTED TEXT)|evidence/acceptance.txt evidence/cold-recovery-acceptance.txt"
)

for g in "${groups[@]}"; do
  title="${g%%|*}"; files="${g#*|}"
  printf '\n## %s\n' "$title" >> "$OUT"
  for f in $files; do
    [ -f "$f" ] || { echo "MISSING $f" >&2; exit 1; }
    lang=$(hint "$f")
    {
      printf '\n### File: `%s`\n\n' "$f"
      printf '_%s_\n\n' "$(purpose "$f")"
      printf '````%s\n' "$lang"
      cat "$f"
      # add a newline only if the source file lacks one, then close the fence
      [ -n "$(tail -c1 "$f")" ] && printf '\n'
      printf '````\n'
    } >> "$OUT"
  done
done

cat >> "$OUT" <<'PROSE4'

# PART 4 — How to download apps (the only two legal channels)

## 4.1 GitHub source packages — `scripts/fetch.sh` + `scripts/manifest.env`

Every compiled component is described by one manifest line:

```text
name|owner/repo|ref|purpose
```

- `name` becomes the checkout directory `src/<name>` and the pin variable
  `PIN_<name>`.
- `ref` of `-` means "default branch"; anything else (e.g. `v3.4.6`,
  `2.80.4`, `pcre2-10.44`) is checked out as a tag for a stable, reproducible
  build.
- fetch.sh runs `git clone --depth 1` (adding `--branch <tag>` when needed),
  then writes the resolved 40-character SHA into `pins.env`. That file is the
  evidence that every rebuild compiles the identical upstream commits.

Fetch everything, or one package:

```bash
bash scripts/fetch.sh                 # all manifest entries
bash scripts/fetch.sh glib pango      # just these two
```

A failed clone makes fetch.sh exit 2 and print `CLONE FAILURES:` — the only
legal response is to retry (network blip) or choose another GitHub mirror;
never reach for a non-allowlisted host.

### Adding a new app from source

1. Append `myapp|some-org/myapp|-|what it is for` to `scripts/manifest.env`.
2. `bash scripts/fetch.sh myapp` — creates `src/myapp` and its pin.
3. Add an `s_myapp()` function to the appropriate build script using the
   env.sh helpers:
   - autotools release tree: `conf_build "$VM_SRC/myapp" --prefix=…` (the
     helper already supplies prefix/libdir and parallel make);
   - meson: `meson_build "$VM_SRC/myapp" -Dopt=value`;
   - cmake: `cmake_build "$VM_SRC/myapp" -DFLAG=ON`;
   - plain makefile (suckless style): `make_build "$VM_SRC/myapp"`.
4. Put `stage myapp s_myapp` into that script's `all)` dispatch in
   dependency order, then run `bash <script> myapp`.

## 4.2 Python packages — pip into the local venv

`recover.sh` phase 1 creates `local/venv` and installs exactly what
`requirements.txt` lists, from PyPI: meson/ninja/cmake (build tools),
python-xlib (GUI control), pillow/mss (screenshots), matplotlib (also
supplies DejaVu TTFs), websockify (tablet bridge), ranger-fm (file
manager). Add Python tooling by appending a pinned line to
requirements.txt and re-running the venv phase (an existing venv is left
untouched, so delete `local/venv` to force a clean reinstall).

## 4.3 What is intentionally NOT downloaded

- Anything outside the three allowed hosts (Google, GNU FTP, apt repos…).
- A standalone Blender binary — the supported path is the PyPI `bpy` wheel
  (Part 6).
- flex/gettext release tarballs — unavailable in the allowlist; the build
  uses a bison release mirror and builds gettext from a GitHub source clone
  instead (see docs/bison-source-notes.md).

# PART 5 — Build order and the one-command bootstrap

With all files from Part 3 in place and the skeleton from Part 2:

```bash
cd arena-vm
bash scripts/recover.sh                 # core desktop
bash scripts/recover.sh --tablet        # core + authenticated tablet bridge
bash scripts/recover.sh --tablet --blender   # everything, ~adds 520 MB wheel
```

Phase map (each writes a marker file under `build/markers/`, so re-runs
resume instead of rebuilding):

1. **venv** — Python toolchain from requirements.txt (skipped if present).
2. **fetch** — all GitHub sources, SHAs into pins.env.
3. **ldconfig** — `echo <prefix>/lib | sudo tee /etc/ld.so.conf.d/…` then
   `sudo ldconfig` (the only sudo use).
4. **build_stack.sh all** — 33 stages: pkgconf, shims, ncurses symlinks,
   zlib/png/expat/freetype/harfbuzz, the full XCB/X11 protocol stack,
   pixman, fontconfig, Xft, xkb data/utils, the X.Org **Xvfb** server,
   xautomation, Xxf86vm.
5. **build_apps.sh all** — m4, autoconf, automake, libtool, openssl, FLTK,
   mbedtls, dillo, libjpeg, libvncserver, x11vnc, vim. **Must precede**
   build_openbox because Openbox's bootstrap calls autoreconf/libtoolize.
6. **build_suckless.sh all** — ncurses (without tests), fonts, dwm, st,
   dmenu.
7. **build_openbox.sh all** — libffi, pcre2 (shared), glib, fribidi, pango
   (Xft backend), libxml2, gettext with a hand-assembled autopoint, Openbox
   itself, then installs config/openbox/* into local/etc/xdg/openbox.
8. With `--blender` only: **build_extras.sh all** (bison 3.0.4 release
   mirror + fseterr patch, libxkbcommon, libglvnd with GLX) then
   **install_blender.sh** (bpy 4.2.0 wheel).
9. **start_desktop.sh** — Xvfb :99 + Openbox (falls back to dwm if the
   binary is missing). `--tablet` additionally backgrounds start_tablet.sh.

Cold-build time is roughly 30–60 minutes depending on CPU. Success ends
with `desktop ready on DISPLAY=:99 (WM=openbox)` and an X.Org version line.
Validate immediately:

```bash
source scripts/env.sh
bash tests/acceptance.sh        # target: PASS=15 FAIL=0 BLOCKED=0
```

# PART 6 — Blender: install, open, render

Blender is the `bpy` PyPI wheel — the full Blender engine usable from
Python; there is no separate GUI binary to launch in this headless design.

```bash
# one-time (or use recover.sh --blender, which also builds the native libs)
bash scripts/build_extras.sh all
bash scripts/install_blender.sh 4.2.0

# verify it imports and see the version
source scripts/env.sh
local/venv/bin/python -c "import bpy; print(bpy.app.version_string)"
```

The native libraries bpy dlopens at import time (libxkbcommon.so.0,
libGL.so.1…) are provided by build_extras and found through
`LD_LIBRARY_PATH` exported by env.sh — do not unset it.

Run the smoke test, which builds a cube + red sphere + camera + sun scene,
renders a PNG, and round-trips a .blend:

```bash
local/venv/bin/python tests/blender_smoke.py evidence/blender.png evidence/blender.blend
# success line: BLENDER_SMOKE_OK render=… blend=… objects=['AgentCube', 'AgentSphere', 'Camera', 'Sun']
```

Open the rendered PNG yourself to confirm the scene is visible (not black).
Rendering uses **Cycles on CPU** with 16 samples. EEVEE needs an EGL/GPU
context; the sandbox has no GPU or vendor GL driver, so a
`Couldn't open libEGL.so.1` message on an EEVEE attempt is expected —
switch the engine to `CYCLES`, set `scene.cycles.device = "CPU"`, exactly as
tests/blender_smoke.py does. To script your own Blender work, write a Python
file using the bpy API and run it with `local/venv/bin/python yourscript.py`.

# PART 7 — Restarting things

| What you want | Command |
|---|---|
| Stop the desktop | `bash scripts/stop_desktop.sh :99` |
| Start/restart the desktop (Openbox) | `bash scripts/start_desktop.sh :99 1280x800x24` |
| Start with dwm instead | `bash scripts/start_desktop.sh :99 1280x800x24 dwm` |
| Restart the tablet bridge (it is killed with the desktop because x11vnc attaches to Xvfb) | `setsid bash scripts/start_tablet.sh :99 6080 > run/tablet.log 2>&1 < /dev/null &` |
| Rebuild one failed stage | `rm -f build/markers/<stage> && bash scripts/<script>.sh <stage>` |
| Re-run everything, resuming from markers | `bash scripts/recover.sh` |
| Full wipe and clean rebuild | `rm -rf src build local run && bash scripts/recover.sh --tablet --blender` |
| Tell Openbox to reload its config | `DISPLAY=:99 local/bin/openbox --reconfigure` |

Health checks:

```bash
pgrep -af "Xvfb :99|bin/openbox|x11vnc|websockify" | grep -v pgrep
ss -ltn | grep -E '5900|6080'     # 5900 = 127.0.0.1 only; 6080 = 0.0.0.0, auth-gated
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:6080/                      # 401 good
curl -s -o /dev/null -w '%{http_code}\n' -u "$(cat run/tablet.cred)" http://127.0.0.1:6080/  # 200 good
ls -la /tmp/.X11-unix/             # X99 socket proves Xvfb is up
tail -n 30 run/Xvfb.log run/openbox.log run/tablet.log
```

# PART 8 — Fixing anything that breaks

## 8.1 Debugging method

1. The build stops with `[FAIL] stage <name> failed`. Note the stage name.
2. Find its function `s_<name>` in the matching script and its build
   directory: autotools → `build/b-<name>/config.log`, meson →
   `build/m-<name>/meson-logs/meson-log.txt`, cmake → `build/c-<name>/`.
3. Fix the root cause (table below covers every failure seen so far).
4. Delete the stage marker if one was written, re-run the single stage,
   then re-run `recover.sh` to continue from where it stopped.
5. Re-run `tests/acceptance.sh`; never claim success without it.

## 8.2 Symptom → cause → exact fix

| Symptom | Cause | Fix |
|---|---|---|
| `autoreconf: command not found` in build_openbox | autotools come from build_apps | Keep recover.sh order: apps → suckless → openbox; run `bash scripts/build_apps.sh all` first |
| `cannot find -lncursesw` / `-ltinfo` | Base image ships versioned runtime `.so.6` but no unversioned dev symlink | `bash scripts/build_stack.sh syslinks` (absolute symlinks into local/lib) |
| vim configure `tgetent() NOT FOUND` | tgetent lives in libtinfo and is not linked transitively | s_vim selects `--with-tlib=tinfo` when local/lib/libtinfo.so exists; ensure the syslinks stage ran |
| ncurses make `DSO missing from command line` in test/ | Test binaries assume transitive tinfo linkage | ncurses is configured `--without-tests`; do not re-enable tests |
| bison `#error Please port gnulib fseterr.c` | glibc ≥2.32 hides `_IO_ftrylockfile` | scripts/patches/bison-fseterr-glibc.patch is applied idempotently via apply_patch in build_extras.sh |
| bison VCS bootstrap failures (flex/autopoint/gnulib) | flex is unobtainable inside the allowlist | Use the bison 3.0.4 release-tarball mirror (tar-mirror/gnu-bison); rationale in docs/bison-source-notes.md |
| xkbcommon meson `found 3.0.4 but need >=3.6` | Hard version gate; the grammar is compatible with 3.0.4 | build_extras.sh strips the gate and maps `parse.error detailed`→`verbose` |
| pkgconf `Dependency "xft" not found` | pkgconf 3.x is case-sensitive; the file is Xft.pc | s_pango symlinks `xft.pc → Xft.pc` in local/lib/pkgconfig |
| meson "value disabled is not boolean" / "unknown option" | feature options take enabled/disabled/auto; booleans take true/false | Match the type in meson_options.txt before retrying the stage |
| autopoint step produced an empty script | A multi-line `--version` string broke a sed expression | s_gettext uses `head -1` and an `[ -s ]` guard; rerun `build_openbox.sh gettext` |
| ranger `setupterm: could not find terminfo database` | curses needs the local terminfo DB and a real TERM | env.sh exports TERMINFO; launch with `TERM=xterm-256color` |
| bpy import: `libxkbcommon.so.0 cannot open` then `libGL.so.1` | The wheel dlopens system-style libs at runtime | env.sh sets LD_LIBRARY_PATH to local/lib; glvnd is built with `-Dglx=enabled` |
| Blender `Couldn't open libEGL.so.1` | No GPU/EGL in the sandbox | Use Cycles CPU (the smoke test already does); EEVEE is infeasible, not broken |
| dillo "Could not establish connection" | Egress allowlist blocks external websites | Expected behavior; demonstrate the browser with `file://` pages |
| Port 6080 returns 401 | HTTP Basic auth challenge | Correct; 200 requires `-u "$(cat run/tablet.cred)"` |
| dillo form field ignores typed text | XTEST clicks do not focus web inputs reliably | Focus the window, `vmc.py key Tab` into the field, then `vmc.py type` |
| make tries to regenerate configure and fails | Git clones flatten mtimes, triggering remake rules | pin_timestamps normalizes source mtimes to 2020-01-01; helpers already call it |
| All binaries vanish after a sandbox reset | src/build/local/run are not persisted | Re-sync Git and rerun recover.sh — the whole design exists for this |
| Tablet bridge dies after a desktop restart | x11vnc binds to a specific Xvfb process | Rerun start_tablet.sh after start_desktop.sh |
| `stage x: command not found` for a hyphenated stage | Dispatch evaluates `s_$1` literally | Use underscore names (openbox_config); hyphen aliases are wired where needed |
| A source repo needs manual edits on every fresh fetch | Clones are disposable | Encode the edit as a scripts/patches/*.patch plus apply_patch, like the bison fix |

## 8.3 Log and state locations

- Build failure detail: `build/b-*/config.log`, `build/m-*/meson-logs/`,
  `build/c-*/CMakeFiles/CMakeOutput.log`.
- Runtime logs: `run/Xvfb.log`, `run/openbox.log`, `run/dwm.log`,
  `run/x11vnc.log`, `run/tablet.log`.
- Resume flags: empty files in `build/markers/` (one per completed stage).
- Credentials: `run/tablet.cred` (never commit; run/ is git-ignored).
- Pin ledger: `pins.env`; package list: `scripts/manifest.env`.

# PART 9 — Daily control of the desktop

Prologue for every GUI command:

```bash
cd arena-vm && source scripts/env.sh
export DISPLAY=:99 XAUTHORITY="$PWD/run/.Xauthority"
PY=local/venv/bin/python
```

`$PY control/vmc.py <command>` reference: `info`, `shot [png]`, `windows`,
`clients` (EWMH app windows — use this one), `focus <id>`, `close <id>`,
`closeall`, `move x y`, `mousedown/mouseup [btn]`, `click [btn] [x] [y]`,
`doubleclick`, `scroll n`, `key <keysym…>`, `type "text"`,
`launch <prog> [args]`, `wait-title <substr> [secs]`, `keysyms`.

Recipes:

```bash
# terminal round-trip
$PY control/vmc.py closeall; sleep 1
$PY control/vmc.py launch local/bin/st -e bash -c "cat > /tmp/out.txt"
sleep 2
WID=$($PY control/vmc.py clients | awk '$3=="bash"{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py type "hello from the agent"
$PY control/vmc.py key ctrl d && cat /tmp/out.txt

# local browser form (external sites are blocked)
$PY control/vmc.py launch local/bin/dillo file:///tmp/page.html
sleep 3
WID=$($PY control/vmc.py clients | awk '/Dillo/{print $1; exit}')
$PY control/vmc.py focus "$WID"
$PY control/vmc.py key Tab
$PY control/vmc.py type "what is meant by physics"
$PY control/vmc.py shot evidence/form.png
```

Openbox keybinds: Alt+Return terminal, Alt+r dmenu, Alt+f ranger, Alt+e
vim, Alt+b dillo, Alt+F4 close, Alt+Tab cycle, Alt+F11 maximize,
Ctrl+Alt+arrows change desktop, Alt+left-drag moves, Alt+right-drag
resizes, right-click desktop opens the launcher menu.

# PART 10 — The acceptance contract

`tests/acceptance.sh` is the definition of done. T1 Xvfb alive · T2 Xlib
connects · T3 Openbox owns the WM check (EWMH verified) · T4 screenshot ·
T5 st maps a window · T6 injected keystrokes land · T7 PIL parses frames ·
T8 a file is created through GUI automation · T9 dillo renders · T10 tablet
returns 401 without credentials · T11 bpy imports and Cycles renders (two
sub-checks) · T12 UE5 feasibility document exists · T13 pin ledger present ·
T14 ranger runs. Full stack: **PASS=15 FAIL=0 BLOCKED=0**; core-only runs
legitimately show T10/T11 as NOT_TESTED. Output is mirrored to
`evidence/acceptance.txt`.

## Emergency cheat sheet

```bash
cd arena-vm
bash scripts/recover.sh --tablet            # build/resume + start everything
source scripts/env.sh && bash tests/acceptance.sh
$PY control/vmc.py shot evidence/now.png    # see the current screen
bash scripts/stop_desktop.sh :99 && bash scripts/start_desktop.sh :99 1280x800x24
```
PROSE4

echo "wrote $OUT ($(wc -l < "$OUT") lines, $(wc -c < "$OUT") bytes)"
embedded=$(grep -c '^### File: ' "$OUT")
echo "embedded files: $embedded"
