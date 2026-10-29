# Ingestion

Every importer returns the same shape: extracted text, segments that carry
locators, warnings, a status (`ready` / `partial` / `error`) and a human-readable
`statusDetail`. Failures are typed (`IngestionError`) and surfaced in the UI — no
importer returns a silently empty source.

| Input | Parser | Locators | Notes / limits |
|---|---|---|---|
| `.txt`, `.md`, `.rst`, `.log`, code | text decoder | line ranges | UTF-8 with BOM handling; size cap 25 MB, indexed text cap 900k chars |
| `.csv`, `.tsv` | built-in parser | row ranges | header preserved, quoted fields and embedded newlines handled |
| `.json` | `JSON.parse` + pretty print | line ranges | invalid JSON is imported raw with a warning |
| `.html` | DOM/regex extraction | line ranges | scripts, styles, nav, forms removed; headings preserved as `#`; tables flattened to `|` rows |
| `.pdf` | `pdfjs-dist` | **page numbers** | text layer required; scanned PDFs return the explicit error "no text layer" instead of empty content |
| `.docx` | `mammoth` | line ranges | browser build takes an ArrayBuffer, Node build takes a Buffer |
| `.pptx` | `JSZip` + slide XML | **slide numbers** | text frames and notes; images/tables become their text |
| transcript (`.vtt`, `.srt`, `.json3`) | caption parser | **timestamps** | cue text joined, `startMs/endMs` retained |
| pasted text / transcript | direct | line ranges or timestamps | used for YouTube/Instagram fallbacks |
| GitHub file URL | contents API | line ranges | token optional (raises rate limit to 5000/h) |
| GitHub directory/repo URL | git trees API | line ranges per file | 40 files / 6 MB per import; per-file cap 512 KB; binary and huge files skipped with a reason |
| web page | reader proxy, then direct fetch | line ranges | reader (`r.jina.ai` by default) handles non-CORS sites; direct fetch only works with CORS headers; both paths report which was used |
| YouTube URL | reader metadata + optional captions | timestamps | without captions the source is `partial` (metadata only) and says so; pasted captions upgrade it |
| Instagram post/reel | reader | line ranges | public captions only; profiles are rejected; login-walled content is out of scope |
| audio/video file | `<audio>`/`<video>` + provider | timestamps | transcription needs a configured provider; frames sampled on canvas |

## Caps and refusal rules

* 25 MB per uploaded file; 5 MB per fetched web page; 512 KB per GitHub file;
  40 files and 6 MB per GitHub import; 900,000 indexed characters per source
  (truncation adds a warning).
* `localhost`, `127.0.0.0/8`, `10/8`, `172.16/12`, `192.168/16` and `.local` pages are
  refused to avoid SSRF-style probing from a reader service.
* Non-text content types are rejected with the actual content type in the message.
* Every URL is normalised and validated before any request is made.

## GitHub specifics

`parseGithubUrl` recognises `/owner/repo`, `/owner/repo/tree/<ref>/<path>`,
`/blob/<ref>/<path>`, and `raw.githubusercontent.com` URLs. Listing uses the git
trees API with `recursive=1` so a monorepo directory import is one call. Rate-limit
responses (`403` with `x-ratelimit-remaining: 0`) are reported with the reset time
and a hint that a token raises the limit; `404` is reported as "no access or does not
exist" rather than a generic failure. File contents come from the contents API (or
`raw.githubusercontent.com` when no token is configured).

## YouTube specifics

The browser cannot call YouTube's player API (CORS and signatures). The importer
therefore fetches the watch page through the reader proxy, extracts title/channel/
description, and looks for caption cues in the reader output. When no cues are found
the source is imported as `partial` with an explicit note, and the dialog offers
"paste a transcript" (VTT/SRT/timestamped text) which becomes the authoritative text
with real timestamps. Downloading the audio track is deliberately not implemented.

## Instagram specifics

Public post captions are sometimes present in the reader output; the importer
extracts the caption, author and shortcode. Login-walled posts, Reels with no caption
text and profile URLs are refused with the reason. No attempt is made to bypass
authentication or download media.

## Media pipeline

1. User supplies an audio/video file (or a transcript).
2. Audio: sent to the configured transcription provider (OpenAI-compatible
   `/audio/transcriptions`); the response's segments become timestamped source
   segments. Files above 24 MB are rejected with a trimming hint.
3. Video: frames are sampled every 45 s (max 6, ≤640 px wide) on a canvas; optional
   vision analysis describes each frame. Frame extraction requires a real browser and
   reports a typed error where `HTMLVideoElement` is unavailable.
4. Progress and cancellation are exposed through the dialog; a cancelled import
   leaves no partial source behind.

There is no ffmpeg, no yt-dlp and no server-side transcoding, by design: those need a
native helper that a static PWA cannot ship.
