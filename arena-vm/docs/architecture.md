# arena-vm architecture

A real, AI-controllable Linux graphical desktop built entirely from source
inside an unprivileged Debian 12 container with **no apt access** (Debian
mirrors are blocked) and **no GPU**.

```
                         Arena agent (the only "brain", no external LLM)
                                     |
                       bash + python control/vmc.py (XTEST, Xlib, mss)
                                     |
   +------------------- Xvfb :99  (real X11 server, in-RAM framebuffer) ----+
   |                                                                         |
   |  dwm (window manager)   st (terminal)   dillo (browser)  vim  dmenu    |
   |                                                                         |
   +---------------------------- X11 socket /tmp/.X11-unix/X99 --------------+
                                     |
                      x11vnc (attached to :99, loopback 127.0.0.1:5900)
                                     |
                 websockify + noVNC (0.0.0.0:6080, HTTP Basic Auth)
                                     |
                            tablet browser (Android)
```

## Layer choices and why

| Layer | Choice | Reason |
|---|---|---|
| Display server | **Xorg-server 21.1.x built with only the Xvfb backend** | no kernel DRM/GPU; Xvfb needs none and still speaks full X11 incl. XTEST, RANDR, DAMAGE, XFIXES, RENDER, Composite |
| Window manager | **dwm 6.4** | tiny, source build with only libX11/Xft/fontconfig deps |
| Terminal | **st 0.9** | same philosophy; real PTY, runs vim/bash |
| Browser | **dillo 3.3-rc + FLTK 1.3 + mbedTLS** | lightest real GUI browser that builds from source in ~4 GB RAM; HTTPS works for allowlisted hosts |
| Input injection | **XTEST via python-xlib** (`control/vmc.py`) | indistinguishable from physical input to applications |
| Screenshots | **mss + Pillow over X11** | real framebuffer pixels, no mockup |
| Remote view | **x11vnc → websockify → noVNC** | FOSS, zero cost, works in any tablet browser |
| 3D | **Blender 4.2 (bpy wheel), CPU render** | PyPI-distributable, no GPU needed for headless rendering |

## Directory layout

```
arena-vm/
  scripts/      all build/start/fetch scripts (committed)
  control/      vmc.py (agent control surface), shoot.py
  tests/        acceptance.sh, blender_smoke.py, collect_env.sh
  config/       reserved for app configs
  docs/         architecture, UE5 feasibility, agent control
  requirements.txt, pins.env, manifest.env
  src/          shallow git checkouts (gitignored, re-fetched on recovery)
  build/        out-of-tree build dirs + stage markers (gitignored)
  local/        install prefix: bin lib include share, venv, noVNC (gitignored)
  evidence/     screenshots, logs, acceptance report (gitignored)
  run/          pids, logs, tablet credentials (gitignored)
```

## Dependency order (resumable via `build/markers/`)

1. `pkgconf`, then zlib/libpng/expat/freetype/harfbuzz/gperf
2. xorgproto, xtrans, libXau, libXdmcp, xcb-proto, libxcb, libX11
3. 19 X extension libraries (libXrender carries a source patch,
   `scripts/patches/libxrender.patch`)
4. pixman, libfontenc, libXfont2, libxkbfile, fontconfig, hand-mesoned libXft
5. xkeyboard-config, xkbcomp, xauth, xbitmaps, libmd (SHA1), xorg-server→Xvfb
6. x11-utils, x11-xserver-utils, xautomation, libXxf86vm, libxkbcommon, glvnd
7. ncurses → dwm/st/dmenu + DejaVu fonts
8. autotools (m4/autoconf/automake/libtool) → OpenSSL, FLTK, mbedTLS, dillo,
   libjpeg-turbo, libvncserver, x11vnc, vim
9. `start_desktop.sh` boots Xvfb+dwm; `start_tablet.sh` adds noVNC

## Known constraints

- Egress allowlist: github.com, pypi.org, registry.npmjs.org only. General web
  browsing from inside dillo only reaches allowlisted destinations; this is a
  sandbox policy, not a software defect (verified: `example.com` fails,
  `pypi.org` renders).
- No GPU: no hardware GL/Vulkan/CUDA. glvnd provides link stubs only.
- The sandbox can wipe `src/build/local` at any time; everything regenerates
  via `scripts/recover.sh` using the pinned SHAs in `pins.env`.
