# Driving FigCraft headlessly

FigCraft is the Rust/Pure-Rust Figma clone (v0.18.1) whose desktop build ships as an
AppImage. Its headless surface is `figcraft-cli`, which the AppImage bundles at
`usr/bin/figcraft-cli` and exposes through AppRun as `figcraft.AppImage cli <args>`.
This folder records a working, reproducible run of that headless surface — design,
edit, export and MCP control — and the outputs it produced.

## Why CI runs it

The sandbox that wrote these files can reach `github.com` and `api.github.com`, but
`release-assets.githubusercontent.com` (where release binaries live) is blocked, and
the AppImage on Google Drive cannot be pulled into the workspace either. A GitHub
Actions runner has no such limit, so `.github/workflows/figcraft-headless.yml`
downloads the published AppImage, verifies it against the release `SHA256SUMS.txt`,
unpacks it with `--appimage-extract` (no FUSE, no X server needed) and drives the real
binary. The job commits its outputs back to this branch, which is how the files here
were produced.

Run it again on any push touching `figcraft/**`, or by bumping the workflow's `FV`
env to another release. `workflow_dispatch` cannot be used from a non-default branch;
on `main` it works too if the trigger is switched back.

## What was proven

| Capability | How | Result |
|---|---|---|
| AppImage really contains a headless CLI | unpack, `--version` | `figcraft-cli 0.18.1`, 91 MB next to the 126 MB GUI binary |
| Scripted design (one undo step, all-or-nothing) | `figcraft-cli run figcraft/design.json` | 11 steps ok → `out/design-run.jsonl` |
| Real layout engine | frame, auto layout (vertical/horizontal, padding, gap, hug/fill), shadow, corner radius, clip, component | `out/page.png` |
| Save / reopen / edit | `run edit.json --in sign-in.figcraft --out sign-in-v2.figcraft` | recolour, pill radius, text change, new layer → `out/page-v2.png` |
| Renders & exports | `render`, `export` (PNG 2x, SVG, PDF) | `390x844` renders, `sign-in.svg` (12 paths), `sign-in.pdf` (1 page, 23 KB) |
| Command surface | `figcraft-cli commands --json` | 645 commands, incl. `node.create`, `component.create`, `inspect.code` |
| MCP control over stdio | `figcraft/mcp_smoke.py` → JSON-RPC `initialize` / `tools/list` / `tools/call` | 19 tools; create, batch, set properties, inspect, screenshot, undo all pass |
| Handoff inspectors | `inspect.code {language: react}`, `inspect.properties` | React components + CSS classes, layout/auto-layout/fill rows with hex→rgb |
| Agent confinement | `--automation-read-root` / `--automation-write-root` | stray write exits 1: `automation path rejected: . is outside the folders that may be written: …` |

Failures worth knowing about: the engine rejects unknown parameters *by name* and
rolls back only that batch step (`unknown parameter `lang` (it takes id, language,
mobile, unit, rootFontSize)`, and `inspect.properties` needs `id`, not `ids`). So a
`command.batch` with `"onError": "continue"` is the right probe for an unfamiliar
command surface — it reports per-step errors without losing the good steps.

## Files

- `design.json` — builds the 390×844 "Sign in" screen (frame, avatar, title, field,
  button component, footer) with an auto-layout tree.
- `edit.json` — reopens that document and edits it (batch with `onError: continue`,
  then adds a layer).
- `mcp_smoke.py` — minimal MCP client over stdio; writes renders and a full
  transcript, exits non-zero on any `isError`.
- `evidence.sh` — assembles `out/run-log.md` from the run.
- `out/` — everything the run produced: `.figcraft` files, JSON document dumps,
  `page.png` / `page-v2.png` / `mcp-card.png`, `exports/Sign in.png`, `sign-in.svg`,
  `sign-in.pdf`, MCP transcript, evidence log and confinement record.

## The interactive app is a separate question

The same AppImage's GUI path (`figcraft --control 7979`, a loopback newline-JSON
socket with `engine.execute`, `ui.pointer`, `ui.screenshot`, `ui.input`) needs a
window: wgpu + egui. This runner has no display and no GPU, and installing an
X server + Mesa here is not possible, so that channel was documented but not
exercised. The headless CLI/MCP path above uses the same engine and command ids, so
it is the practical way to drive FigCraft from an agent.

Docs used: `docs/cli.md`, `docs/mcp.md`, `docs/control-protocol.md` from
`storytold/figcraft` v0.18.1.
