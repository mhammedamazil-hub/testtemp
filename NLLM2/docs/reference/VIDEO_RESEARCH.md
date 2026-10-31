# Video research — method, and why there are no results

## Status: BLOCKED (no results)

Required tooling is absent, verified 2026-10-31: `yt-dlp`, `ffmpeg`, `ffprobe`,
`chromium`, `tesseract`. The allowlist blocks GitHub release downloads, so the binaries
cannot be side-loaded either. No video was downloaded, sampled, transcribed or viewed.

**Therefore this file contains zero frames, zero transcripts and zero timestamps.**
Nothing in it should be cited as an observation of the reference product.

## The method, ready to run the moment the tools exist

`MASTER_FORGE.md` §5 specifies the pipeline. The checks I would run first, in order:

1. `yt-dlp --version`, `ffprobe -version` — confirm the build actually supports
   `--write-auto-subs`, `-f bv*+ba/b`, and the `tile` filter.
2. `yt-dlp --skip-download --dump-single-json` — metadata before any media, so the
   licence/authorisation decision happens before bytes move.
3. `--list-subs` / `--list-formats` — record what the extractor *offered*; a missing
   caption track is a finding, not a reason to write one.
4. contact sheet: 48 frames over the full duration (`fps=48/duration, tile=6x8`), then
   1 fps sheets (5×5 pages), then scene detection at `gt(scene,0.30)` tuned by result.
5. targeted 20-second windows at 2 fps around each UI transition found on a sheet.
6. `ffprobe -show_format -show_streams -show_chapters` for duration, frame rate,
   rotation, audio streams and chapters.
7. audio → 16 kHz mono WAV for listening; captions are preferred over ASR, and any ASR
   transcript is labelled machine-generated.

## Evidence rules that will apply

* A contact sheet proves only the sampled frames; it is not proof of every frame.
* A frame caption is not a verified user action; clicks and keystrokes are invisible.
* Every table row needs `Time | Image evidence | Screen | Action | Observed result |
  Confidence`, with the image path relative to the repository.
* No video is downloaded unless it is the user's own, licensed for it, or permitted by
  the platform's terms. Public visibility is not permission to republish.

## Unblock

`pip install --user yt-dlp` (or an isolated venv) plus a static FFmpeg build in an
isolated prefix. Then this document becomes a result table instead of a plan.
