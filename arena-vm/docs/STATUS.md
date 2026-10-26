# Arena-VM build status — 2026-10-26

> **New here? Read [SKILL.md](../SKILL.md) first** — the complete
> from-scratch setup and desktop-control guide for an AI with no prior
> context (fast-path rebuild commands, vmc.py cookbook, troubleshooting).

Living status document for the AI-controllable virtual graphical Linux
computer. Source of truth for what is built, verified, and still pending.
All paths relative to `arena-vm/`. Acceptance evidence in `evidence/`.

## 1. Acceptance matrix (tests/acceptance.sh)

Latest full run: **PASS=15, FAIL=0, BLOCKED=0** (14 test IDs; T11 has two
sub-checks). WM under test: **Openbox 3.7** (dwm retained as fallback).

| ID | Requirement | Status | Evidence |
|----|-------------|--------|----------|
| T1 | Real X display server (Xvfb) | PASS | Xvfb :99, 1280x800x24 |
| T2 | X client library works | PASS | Xlib 21.1.16 |
| T3 | Window manager running | PASS | Openbox (EWMH `_NET_WM_NAME` verified), dwm fallback |
| T4 | Screenshot capture | PASS | evidence/t4_shot.png |
| T5 | Terminal GUI app launches | PASS | st window mapped under Openbox |
| T6 | Mouse/keyboard input injection | PASS | evidence/t6_gui_input.png |
| T7 | Agent screenshot loop (PIL) | PASS | 1280x800 frames |
| T8 | End-to-end GUI feedback loop | PASS | /tmp/acceptance_gui.txt via injected input |
| T9 | Browser | PASS | dillo renders local page |
| T10 | Authenticated tablet access | PASS | noVNC 6080: 401 without creds, 200 with; x11vnc loopback 5900 |
| T11 | Blender installed + CPU render | PASS | bpy 4.2.0, Cycles CPU, evidence/t11_blender.png + .blend round-trip |
| T12 | UE5 feasibility investigation | PASS | docs/ue5-feasibility.md (hard-blocked, documented) |
| T13 | Recoverable pinned build | PASS | 80 pins in pins.env, marker-gated scripts |
| T14 | File manager | PASS | ranger 1.9.4 inside st, evidence/t_filemanager_ranger.png |

## 2. What is finished and working

### Core desktop (all built from pinned FOSS sources into local/, no root)
- **Xvfb** virtual framebuffer + full X11 client stack (libX11 21.1.16,
  xcb, xkbcomp, xauth, xbitmaps, xset/xrandr utilities).
- **Openbox 3.7** — the required lightweight WM — plus **dwm** fallback,
  **st** terminal, **dmenu** launcher (suckless).
- Fonts (X11 bitmap, fontconfig/freetype/harfbuzz) and US keyboard layout.
- Applications: **dillo** browser (FLTK), **vim 9.2**, st terminal,
  **ranger 1.9.4** terminal file manager (PyPI, FOSS, runs in st).
- **xautomation** + `control/vmc.py` agent tool: screenshot, keystroke,
  mouse move/click, EWMH `clients`/`closeall`, launch/focus/close. The
  Arena agent is the sole controller; no external AI.
- **x11vnc** on 127.0.0.1:5900 only; **noVNC** websockify on 0.0.0.0:6080
  with HTTP Basic auth (creds in run/tablet.cred, never committed).

### Openbox dependency chain (scripts/build_openbox.sh, 8 marker-gated stages)
libffi 3.4.6 → pcre2 10.44 (shared) → glib 2.80.4 LTS → fribidi 1.0.15 →
pango 1.54.0 (Xft backend, no cairo/introspection) → libxml2 2.13.9 →
gettext-runtime 0.23.1 (real autopoint assembled from gettext-tools
template + prebuilt archive.dir.tar) → Openbox 3.7.
Key fixes recorded in the script: pkgconf case-sensitive `xft.pc` symlink;
meson option typing (boolean vs feature); autopoint substitution guard.

### Blender dependency chain (scripts/build_extras.sh)
- **bison 3.0.4** from `tar-mirror/gnu-bison` release mirror (no flex
  needed); one idempotent fseterr.c modern-glibc fix. Dead ends:
  docs/bison-source-notes.md.
- **libxkbcommon**: bison 3.0.4 accepted by dropping the >=3.6 gate and
  mapping `parse.error detailed` → `verbose`; xkbregistry off (no libxml
  was present at that stage).
- **libglvnd** GL dispatch stubs with glx enabled (provides libGL.so.1).
- **bpy 4.2.0** PyPI wheel; smoke test renders Cycles-on-CPU (EEVEE needs
  EGL/GPU context, unavailable). LD_LIBRARY_PATH wired in scripts/env.sh.

### Openbox desktop configuration (config/openbox/, installed by build_openbox.sh)
- `rc.xml`: Clearlooks theme, 4 desktops, keybinds — Alt+Enter terminal,
  Alt+r dmenu, Alt+f ranger, Alt+e vim, Alt+b dillo, Alt+F4 close,
  Alt+Tab cycle, Alt+F11 maximize; Alt-drag move/resize.
- `menu.xml`: right-click root menu with all five launchers, window
  submenus, Reconfigure/Exit. Evidence: evidence/t_openbox_menu.png.
- `autostart`: US keyboard + root background painted by
  control/desktop_bg.py (python-xlib; xsetroot is not in the toolchain).
- Agent-verified end to end: Alt+Enter keystroke injected via vmc.py
  launches st; right-click menu captured in a real screenshot.

### Build engineering
- scripts/recover.sh: one-command, marker-gated, resumable full rebuild
  (venv → fetch → stack → suckless → openbox chain → apps → optional
  `--blender` extras+wheel → services; `--tablet` starts the bridge).
- 80 exact source-commit pins (pins.env) + manifest (scripts/manifest.env).
- No large binaries in Git (src/, build/, local/, run/, evidence media,
  *.blend ignored). Zero-rupee, FOSS-only; egress limited to github.com /
  pypi.org / registry.npmjs.org.

## 3. Known limitations / design decisions

- EEVEE GPU rendering impossible (no GPU/EGL); Cycles CPU is the path.
- UE5 hard-blocked: no GPU, no Windows/macOS host, no Vulkan; evidence in
  docs/ue5-feasibility.md.
- bison is 3.0.4 not 3.8.x: sufficient for xkbcommon's grammar; newer
  bison needs flex, unobtainable inside the egress allowlist (see
  docs/bison-source-notes.md).
- Openbox built without imlib2/librsvg/startup-notification (all optional,
  explicitly disabled): no menu icons; core WM functions unaffected.
- ranger needs a real TERM (xterm-256color) plus TERMINFO pointing at the
  local ncurses database; both are exported by scripts/env.sh/start_desktop.

## 4. Cold-recovery validation (2026-10-26, clean worktree)

A full rebuild was run from a detached worktree containing only tracked
files (zero markers/build dirs). `recover.sh` + `build_extras.sh all` +
`install_blender.sh` rebuilt the whole stack from the 80 pinned sources and
the acceptance suite reported **PASS=15 FAIL=0 BLOCKED=0** on the first
fully clean run after two genuine bugs were caught and fixed during
validation — this is why the cold run matters:

1. **Stage ordering (commit f00a665):** build_openbox ran before
   build_apps, so libffi's `autoreconf` had no autoconf/automake/libtool.
   recover.sh now runs build_apps → build_suckless → build_openbox.
2. **Bison patch never applied (commit 8d52e13):** the inline sed used
   `|` as its delimiter while the C expression contains `||`, silently
   failing in a fresh tree (the working tree had been hand-patched).
   Replaced by scripts/patches/bison-fseterr-glibc.patch through the
   idempotent apply_patch helper.

Resumability was also proven: the failed first invocation left 37
markers, and the fixed run skipped every completed stage. Pins written by
the clean fetch were byte-identical (pin names/SHAs) to this tree.
Cold-run evidence: evidence/cold-recovery-acceptance.txt and
evidence/cold-recovery-blender.png (Cycles CPU render from the rebuild).

## 5. Pending / next ideas

1. Optional: Openbox menu/keyboard customization (rc.xml) and an
   autostart entry for a terminal + ranger.
2. Optional: ranger image previews need a framebuffer/drawable protocol;
   text/metadata preview works out of the box.
3. Optional: Openbox per-user rc.xml tweaks beyond the tracked baseline
   (keybinds/menu/autostart in config/openbox/ already cover common use).
