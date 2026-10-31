# Security model

Forge is a **single-operator** application. It has no login, so the network
boundary *is* the trust boundary: run it on `127.0.0.1`, or behind your own
authenticated reverse proxy. This is stated rather than hidden — there is no
"secure multi-user mode".

## Threats and controls

| Threat | Control | Test |
|---|---|---|
| SSRF via URL ingestion (cloud metadata, internal services) | `security::check_url`: http/https only; blocks `localhost`, `.local`, link-local, private, CGNAT 100.64/10, multicast, unspecified; **resolves DNS and rejects if any address is non-public**; length cap 2048 | `security::tests::blocks_ssrf_targets`, `allows_public_http`, `ip_classification`; e2e `rejects_bad_input_and_dangerous_urls` |
| Path traversal in static hosting | `security::safe_join` strips leading `/`, rejects `..`, `\`, `:`, then canonicalises and requires the result to stay under the root | `security::path_traversal_is_refused`, `web::traversal_and_missing_assets_are_refused`, e2e static test |
| Hostile uploaded filename | `security::clean_filename` takes the basename, keeps `[A-Za-z0-9._ -()]`, caps at 120 chars; uploads never write to disk under a user path | e2e uploads `../..%2Fetc%2Fpasswd` and asserts the stored title is sanitised |
| Stored XSS in source text / notes | the UI builds DOM with `textContent` (no `innerHTML` for user data); `security::escape_html` exists for any future HTML path; CSP `script-src 'self'` blocks inline scripts | `security::html_escaping_prevents_markup`, `http::responses_carry_security_headers`, UI smoke "no uncaught page errors" |
| Prompt injection in a document | no model is called in this build, so injected instructions cannot execute. If a provider is added, answers stay extractive-validated: `grounded::validate_citations` rejects any quote not literally present, so injected text cannot become a fake citation | `grounded::validator_catches_fabricated_quotes_and_ids` |
| API key leakage | keys are stored in `data/secrets.json` with mode `0600`, never returned (`key_set` only), never logged, never included in notebook export (`export` emits notebooks/sources/chunks/notes/artifacts only) | `providers::secrets_are_private_and_removable`, e2e `provider_keys_are_write_only_and_never_used_unsafely` |
| Cleartext key transmission | `providers::validate` refuses any `base_url` not starting with `https://` | `providers::rejects_cleartext_endpoints_and_short_keys` |
| Oversized request bodies | header cap 64 KiB, body cap 32 MiB at the HTTP layer; `MAX_SOURCE_BYTES` 25 MiB, `MAX_TEXT_BYTES` 8 MiB at the API | `http` tests; e2e empty/oversize upload |
| Oversized worksheets (memory/JSON blow-up) | `MAX_TABLE_ROWS` 20,000 rows per sheet; the grid is marked `truncated` and aggregations **refuse** instead of totalling partial data | `tables::tests::oversized_sheets_are_truncated_and_aggregations_refuse` |
| Table rows leaking into an HTTP response | `GET /api/sources/:id/tables?rows=N` caps rows (default 200, max 2,000) while `row_count` reports the true size | e2e `spreadsheet_questions_are_computed_over_real_cells` |
| Hostile media files | container parsing is bounds-checked at every read (`u32`/`u64` box sizes, `0`-size guard, truncation tolerated and reported); no allocation is sized from an unvalidated box length; body capped at `MAX_SOURCE_BYTES` (`413 too_large`), empty body `400` | `media` tests (`truncated_file_is_reported_not_guessed`, `sixty_four_bit_box_sizes_are_understood`), e2e `media_metadata_is_read_without_decoding_frames` |
| Hidden worksheets used as evidence | hidden sheets are stored for inspection but excluded from retrieval, aggregation and citations | `tables::tests::hidden_sheets_are_never_used_for_answers` |
| Fabricated numbers from a spreadsheet | non-numeric/empty cells are `Empty` (never `0`), mixed units are refused, cached column totals are excluded and named, ambiguous separators are rejected | `tables` tests (4 refusal cases) |
| Malformed JSON / unknown routes | strict parser → `400 bad_json`; unknown route → `404 not_found` with a code | e2e `rejects_bad_input_and_dangerous_urls` |
| Zip bombs / hostile OOXML | entries are read through the central directory with declared sizes; the ZIP reader rejects ZIP64, unknown compression and out-of-bounds offsets; DOCX/PPTX/XLSX extraction reads only named parts (not every entry) | `ingest::zip::rejects_non_zip_and_garbage`, `ingest::ooxml::non_ooxml_is_rejected_clearly` |
| Data corruption / partial writes | every mutation rewrites a temp file and `rename`s it; a corrupt `db.json` decodes to an empty database instead of crashing | `store` tests incl. corrupt-db safety |
| Clickjacking / MIME sniffing / referrer leaks | `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, CSP with `object-src 'none'`, `base-uri 'none'`, `form-action 'self'` | `http::responses_carry_security_headers` |

## Deliberate non-goals

* **No authentication.** Not a bug: adding a half-baked login would imply a
  security property that is not tested. Bind to localhost or use your proxy.
* **No rate limiting.** Single operator, local socket.
* **No sandboxing of parsed files.** Parsers are pure Rust over byte slices; the
  process does not execute document content.
* **No TLS.** Therefore no URL fetching and no remote providers.

## Residual risks

1. The PDF object scanner is heuristic; a malformed PDF yields an empty source with
   a warning rather than a crash, but a pathological file could still be slow.
2. `data/` contains the full corpus in plain text. Protect the directory; it is
   git-ignored, and `secrets.json` is owner-only.
3. A reverse proxy with its own header limits should sit in front if exposed beyond
   localhost.

## Reporting

This is a local project with no deployed service. Record findings in
`docs/UNKNOWN_AND_LIMITATIONS.md` and add a failing test before fixing.

## URL fetching controls (added 2026-10-31)

| Control | Implementation |
|---|---|
| scheme allowlist | `http`, `https` only, at parse time and again via `--proto`/`--proto-redir` |
| credentials in URL | refused explicitly (`user:pass@host` → 422 `url_rejected`) |
| userinfo forwarding | never sent; curl receives no `-u`, no cookies, no proxy |
| host resolution | Forge resolves, then requires **every** address to be public |
| DNS rebinding | defeated by pinning curl to the validated IP with `--resolve` |
| redirects | curl does not follow; each hop re-validated and re-pinned, max 5 |
| response size | 20 MiB (`--max-filesize` + a second check after read) |
| time | 30 s (`--max-time`) |
| metadata endpoints | `169.254.169.254`, `metadata.google.internal` blocked by IP and hostname |
| loopback/private | blocked unless `FORGE_FETCH_ALLOW_PRIVATE=1`, which is reported in capabilities |
| shell injection | `Command::new("curl").args([…])` — no shell, no interpolation, no user-controlled argv flags |
| temp files | per-request unique names in the system temp dir, removed after every request including failures |

Residual risk: `curl` itself is third-party code parsing hostile HTTP. It runs as the
server's user, in a subprocess, with output restricted to two temp files. A compromised
curl is out of scope for this layer.
