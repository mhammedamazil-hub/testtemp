# Research blockers

Everything here was **tested**, not assumed. Each entry states the attempt, the exact
result, and what would unblock it. A blocker is never a reason to invent a finding.

Last verified: 2026-10-31.

## 1. Live product access — BLOCKED

| Attempt | Result |
|---|---|
| `curl https://notebook.google.com` from the sandbox | HTTP **000** (no route) |
| `curl https://support.google.com/gemininotebook/` | HTTP **000** (no route) |
| agent page-fetch tool → `support.google.com` | **works** (this is how official docs were read) |
| browser for REA to attach to | none installed (`chromium` absent), and no authenticated session exists |
| user credentials | none available, and none will be requested in chat |

**Unblock:** the operator runs a browser with an authorised session and exposes a
loopback CDP endpoint, or grants a screen/recording; or the agent's page tool is
extended to the product origin.

**Consequence:** no `LIVE_OBSERVED` rows exist in `docs/reference/FEATURE_MATRIX.md`.
Every reference behaviour there is `OFFICIAL_DOCUMENTED`, `CODE_CORROBORATED`, or
explicitly `INFERRED`.

## 2. Video research — PARTIAL (tooling unblocked, footage still BLOCKED)

| Tool | Status 2026-10-31 |
|---|---|
| `yt-dlp` | **installed** 2026.10.26 from the PyPI wheel |
| `ffmpeg` | **installed** 7.0.2 static (`imageio-ffmpeg` wheel; needs `chmod +x`) |
| `ffprobe` | absent (that wheel ships `ffmpeg` only) |
| `chromium` | absent |
| `tesseract` | absent |

The earlier entry here said no package route existed. That was wrong in one direction:
**PyPI is reachable** even though crates.io and GitHub release assets are not, so
`pip install --target=/tmp/mediapy yt-dlp imageio-ffmpeg` works.

What is still blocked is the evidence that matters: there is no authorized recording, no
route to any platform host, and no permission to download or republish third-party footage.
So `docs/reference/VIDEO_RESEARCH.md` now contains measurements of a clip **we generated**
(container metadata cross-checked against `ffmpeg -i`, mean-RGB timelines, a contact sheet)
and an explicit statement that none of it is evidence about another product. It records one
reusable finding: FFmpeg's `scene` filter is luma-based and missed a red→green cut on our own
clip, so cut detection cannot rely on it.

**Consequence:** no video row in the feature matrix may be labelled `VIDEO_OBSERVED` about a
reference product. The label is used only for our fixture, and the matrix says so.

## 3. Broad web research / search providers — BLOCKED

Sandbox egress is allowlisted to `api.github.com`. There is no search API key, and no
provider is reachable. Forge therefore implements **no** web-search provider today; the
Deep/Fast Research workflows are specified but unimplemented rather than faked.

**Note:** Forge *can* fetch URLs it is given (via `curl`) — the guard and the fetcher are
tested against a real local HTTP origin — but in this sandbox any public host fails at
connect, returning `502 fetch_failed` honestly.

## 4. Providers for generation (LLM, TTS, ASR, vision, embeddings) — BLOCKED

No API keys, no credits, and the zero-budget constraint forbids paid services. Forge
ships provider *interfaces* (`providers.rs`, keys write-only and `0600`) and the local
extractive engine keeps working without any provider. Anything requiring a model is
marked `unsupported` in `/api/capabilities` with a reason.

## 5. REA

`rea-agents@6.8.0` installs and runs, but `analyze` needs ghidra/hopper/ida/kuna (none
present, one is paid, one macOS-only) and there is no browser target.
`inspect-artifact` (hashing/inventory) works. Full detail: `docs/REA_EVIDENCE_INDEX.md`.

## 6. What is *not* blocked

* official help-centre documentation (agent page tool);
* pinned public repositories via the GitHub API (`api.github.com` is reachable);
* local HTTP/HTTP fixtures, so network-facing code can be tested end-to-end;
* the full Rust toolchain (installed per session into `/tmp/forge-rust`).
