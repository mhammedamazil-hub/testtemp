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
