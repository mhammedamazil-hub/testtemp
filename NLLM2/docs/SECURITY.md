# Security model

Forge is a **single-operator** application with an optional access gate.

* **Default (no `data/owner.token`):** there is no authentication, so the network boundary *is*
  the trust boundary. Run it on `127.0.0.1`, or behind your own authenticated reverse proxy.
  `GET /api/auth` and the header badge say `single-user` explicitly.
* **Enforced:** `POST /api/auth/owner-token` stores a token (SHA-256 only, file mode `0600`).
  Every route then requires `Authorization: Bearer`/`X-Forge-Token`, except `GET /api/capabilities`
  and `GET /api/auth` — which a client needs in order to learn what it may do, and which read no
  notebook, source, chunk or count. A share link (`X-Forge-Share`) is bound to one notebook and
  one role (`viewer` = read/ask, `editor` = +write); `manage` actions are owner-only. Revocation
  is checked per request.

There are still no user accounts, no groups, no audit log and no rate limiting on token attempts;
those are non-goals, listed at the bottom rather than implied.

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
| Unauthenticated access to a protected instance | when an owner token exists, `Api::authenticate` runs before dispatch; missing/invalid credentials are `401 unauthorized`. The action a route needs is derived from method+path in one place, and anything unmatched defaults to `write` (never to public) | `auth` (8 tests), e2e `journey_j_share_links_are_scoped_by_role_and_revocable` |
| A share link reading another notebook | the link's notebook id is compared against the notebook the route actually acts on (resolved through sources/notes/artifacts/jobs/shares, not just the URL), and `GET /api/notebooks` is filtered to that one notebook | same test: other notebook → `403 not_in_share`, list length 1 |
| A viewer editing, or an editor deleting | `auth::allow` maps role+action: viewer read only, editor read+write, `manage` owner only | same test: 403 `forbidden_role` on ingest, PATCH, share creation, delete |
| Leaked share token from a dump | only `token_sha256` and a 6-char prefix are stored; the plaintext is returned once at creation and never listed | same test: the shares list contains no `token`, no hash, 6-char prefixes only |
| Timing attack on a credential compare | `auth::token_eq` compares all bytes of the hex digest | `auth::tests::tokens_are_compared_in_constant_time` |
| Revoked link still working | grants are re-read on every request and `revoked_at` is honoured; the streaming endpoint is guarded before its own dispatch | same test: 6 endpoints + the stream return 401 after revocation |
| Media files quoted as evidence | `ingest::unsupported_media` refuses video/audio/image uploads as sources with a reason instead of lossy-decoding them | `journey_e_…` (422 `unsupported_source_kind`) |
| Redirect-based SSRF in discovery | discovery reuses `fetch::fetch`, which re-validates and pins DNS at every hop | `fetch` redirect test; `discovery::tests` for URL resolution |
| Clickjacking / MIME sniffing / referrer leaks | `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, CSP with `object-src 'none'`, `base-uri 'none'`, `form-action 'self'` | `http::responses_carry_security_headers` |

## Deliberate non-goals

* **No accounts, sessions or groups.** One owner token plus per-notebook links is the whole
  model; a half-built login would imply properties this code does not test (`DECISIONS.md` D20).
* **No audit log.** Revocation stops the next request and says nothing about the last one.
* **No rate limiting.** Single operator, local socket — including on token attempts.
* **No CSRF tokens.** State travels in headers, not cookies, so there is no ambient credential
  for a cross-site request to ride on.
* **No sandboxing of parsed files.** Parsers are pure Rust over byte slices; the
  process does not execute document content.
* **No TLS client.** `https://` fetching shells out to `curl`; there are no remote providers.

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
