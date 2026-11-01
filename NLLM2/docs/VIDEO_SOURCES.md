# Video and audio sources — what Forge does, and what it refuses

Last updated: 2026-11-01.

## The short version

Forge indexes **text**. For a video it can index a **timed transcript you supply** (`.vtt` or
`.srt`) and cite it by timecode (`00:42`). It does **not** download video, does not call a
platform API, does not transcribe audio and does not look at a single frame.

| Task | State | Evidence |
|---|---|---|
| User-supplied WebVTT captions | **supported**, `timestamp` locators | `ingest::transcript` (4 tests), `journey_e_…` |
| User-supplied SubRip captions | **supported**, same path | `ingest::transcript` |
| Container metadata (duration, codec, tracks, rotation) | **supported**, metadata only | `media` (10 tests), `POST /api/media/inspect` |
| Uploading an `.mp4`/`.mp3`/`.png` as a *source* | **refused** with `422 unsupported_source_kind` and a reason | `journey_e_…`, `ingest::unsupported_media` |
| Downloading a YouTube (or any platform) video or its captions | **unsupported** — needs an authorised extractor and per-source permission | capability page `sources.youtube` |
| Speech-to-text | **unsupported** — no ASR engine offline | capability page `sources.audio_transcript` |
| Frame/diagram/scene analysis | **unsupported** — no decoder in the product, no vision model | capability page `sources.video` |

## Why refusing is the honest choice

Decoding an MP4 with the text adapter produces mojibake. That text would then be retrievable,
citable and quotable — evidence that does not mean anything, presented as if it did. The refusal
carries the reason and points at the supported path:

> video files are not sources: Forge does not download or decode video and has no frame analysis.
> Upload a timed transcript (.vtt/.srt) to cite what was said, or POST the file to
> /api/media/inspect for container metadata only

## What a timecode citation means

`Locator { kind: "timestamp", at_ms: 42_000 }` renders as `00:42` and means **spoken at 00:42**.
It never means "shown at 00:42". The capability payload says this in the same words, so a client
cannot read a visual claim into it.

## Journey E against the spec

§14 Journey E asks for: a public captioned video URL → import transcript with time metadata →
ask about a statement at a known time → expect a timecode citation → if visual analysis is
available, ask about an on-screen diagram → otherwise state the limitation explicitly.

Forge satisfies steps 3–4 and 7, and refuses 1–2 honestly (no downloader, no platform access) and
5–6 (no vision). The test asserts both halves: the timecode citation *and* the explicit
limitation on the capability page.

## Research tooling is not product code

Frame decoding was done once, outside the product, with a static FFmpeg downloaded from PyPI for
a session (see `docs/reference/VIDEO_RESEARCH.md` and `DECISIONS.md` D18). No frame, caption or
transcript from that exercise ships with Forge, and the product never shells out to FFmpeg.
