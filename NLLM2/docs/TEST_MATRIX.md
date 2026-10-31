# Test matrix

## How to run

```bash
export PATH=/tmp/forge-rust/bin:$PATH      # this sandbox's Rust install
cd NLLM2
cargo test --offline                        # 92 tests: 87 lib + 5 e2e
```

UI drive-through (needs a running server and jsdom, dev-only):

```bash
FORGE_ADDR=0.0.0.0:8099 FORGE_DATA_DIR=/tmp/forge-live/data \
  FORGE_WEB_DIR=apps/web ./target/debug/forge-server &
FORGE_URL=http://127.0.0.1:8099 NODE_PATH=/tmp/uitest/node_modules node tests/ui-smoke.js   # 29 assertions
```

## Unit tests (87) — exact names from `cargo test --lib -- --list`

### `json` (6)
`roundtrip_primitives`, `numbers`, `unicode_escapes`, `utf8_passthrough`,
`nested_and_empty`, `rejects_bad_input` — the wire format shared by store, API and
export.

### `domain` (5)
`iso_timestamps_are_correct`, `ids_are_unique_and_prefixed`, `locator_roundtrip`,
`json_roundtrip_message`, `kind_detection_from_filename`.

### `store` (4)
`persists_across_reopen`, `atomic_write_leaves_no_tmp_file`, `backup_writes_a_copy`,
`corrupt_db_does_not_panic`.

### `http` (6)
`parses_query_and_percent_encoding`, `encodes_response_with_content_length`,
`rejects_malformed`, `rejects_oversized_body`, `mime_types`,
`responses_carry_security_headers`.

### `inflate` (2)
`stored_block`, `rejects_garbage`. Fixed/dynamic Huffman are exercised indirectly by
`ingest::zip::reads_stored_and_deflated_entries`, which inflates a real DEFLATE
stream produced by another tool.

### `ingest` (3)
`pdf_fixtures_extract_with_page_locators`, `rejects_non_pdf`,
`chunking_preserves_words_and_locators`.

### `ingest::text` (6)
`plain_text_paragraphs`, `markdown_sections_and_lists`,
`html_strips_scripts_and_decodes_entities`, `entities_and_numeric_refs`,
`csv_rows_are_self_describing`, `csv_handles_quotes_and_commas`.

### `ingest::zip` (2)
`reads_stored_and_deflated_entries`, `rejects_non_zip_and_garbage`.

### `ingest::ooxml` (4)
`docx_yields_paragraphs_with_locators`, `pptx_yields_slides_with_numbers`,
`xlsx_yields_rows_with_header_context`, `non_ooxml_is_rejected_clearly`.

### `retrieve` (6)
`tokenizer_handles_unicode_and_punctuation`, `ranks_the_relevant_chunk_first`,
`exact_identifiers_are_found`, `stopword_only_query_returns_nothing`,
`no_match_returns_empty`, `sentence_selection_prefers_overlap`.

### `grounded` (8)
`answers_with_a_valid_citation`, `abstains_when_evidence_is_absent`,
`abstains_on_empty_notebook`, `degenerate_question_is_detected`,
`synthesises_across_two_sources`, `boilerplate_in_a_scoring_chunk_is_not_cited`,
`citation_numbers_are_contiguous`, `validator_catches_fabricated_quotes_and_ids`.

### `artifacts` (7)
`flashcards_are_grounded_and_cited`, `quiz_options_contain_the_answer`,
`report_covers_every_source`, `mindmap_has_nodes_and_edges`,
`audio_script_is_a_script_not_audio`, `all_supported_kinds_generate`,
`unknown_kind_is_rejected`.

### `security` (6)
`blocks_ssrf_targets`, `allows_public_http`, `ip_classification`,
`path_traversal_is_refused`, `html_escaping_prevents_markup`,
`filenames_are_sanitised`.

### `providers` (4)
`rejects_cleartext_endpoints_and_short_keys`, `extractive_engine_is_always_selected`,
`configured_remote_provider_states_its_limitation`, `secrets_are_private_and_removable`.

### `notebooklm` (16)
`batchexecute_envelope_is_the_documented_shape`, `request_has_rpc_scoped_url_and_headers`,
`url_source_puts_the_url_at_slot_two`, `text_source_carries_title_content_and_type_code`,
`youtube_source_uses_slot_seven`, `audio_artifact_uses_type_1_and_length_code`,
`quiz_and_flashcards_differ_only_by_variant_slot`, `every_artifact_kind_has_a_distinct_code`,
`chat_request_targets_the_streamed_endpoint_with_at_token`,
`parses_length_framed_wrb_fr_responses`, `streaming_chat_takes_the_final_cumulative_chunk`,
`artifact_kickoff_is_decoded`, `transport_is_unavailable_and_says_why`,
`corroboration_counts_match_the_measured_evidence`,
`status_reports_protocol_implemented_but_unreachable`.
These assert the **exact** wire shapes recovered by reverse engineering
(`docs/NOTEBOOKLM_PROTOCOL.md`); they do not — and cannot — prove the service accepts
them, because it is unreachable.

### `web` (3)
`serves_the_shell_and_assets`, `deep_links_fall_back_to_shell`,
`traversal_and_missing_assets_are_refused`.

## End-to-end HTTP tests (5, real sockets)

| Test | Proves |
|---|---|
| `full_research_workflow_over_http` | notebook → text source → grounded answer with citation → cited chunk exists and contains the quote verbatim → off-topic question abstains with zero citations → flashcards → note → export → import into a fresh server → imported notebook answers with citations |
| `rejects_bad_input_and_dangerous_urls` | empty question 422, no-sources 409, four SSRF URLs 422 `url_rejected`, public URL 501 `url_ingest_unavailable`, bad JSON 400, unknown route 404, hostile upload filename sanitised |
| `static_app_is_served_over_http` | `/` and `/app.js` served; `/../../Cargo.toml` not served |
| `imports_office_formats_over_http` | DOCX/PPTX/XLSX upload → correct `kind` and locator kind; a DOCX answer carries a `paragraph` citation |
| `provider_keys_are_write_only_and_never_used_unsafely` | http endpoint refused, valid config stored, list response contains no key, chat falls back with an `engine_note` mentioning TLS, backup created and listed |

## UI assertions (29, jsdom + live server)

Library rendering · engine badge from `/api/capabilities` · notebook creation via the
form · three-pane workspace · paste-import · source list with passage counts ·
grounded answer · citation chip with a number · viewer opens the cited source ·
cited chunk highlighted · viewer closes · flashcards generated and flippable ·
quiz has multiple options and reveals the correct answer · mind-map SVG nodes ·
note saved and listed · report built with citations · capabilities page lists PDF as
supported and audio as unsupported · responsive breakpoints present ·
**no uncaught page errors**.

## Fixtures

| File | Generator | Contents |
|---|---|---|
| `tests/fixtures/simple.pdf` | `make_pdfs.py` | 2 pages, uncompressed text streams |
| `tests/fixtures/flate.pdf` | `make_pdfs.py` | 2 pages, `/FlateDecode` streams |
| `tests/fixtures/sample.docx` | `make_ooxml.py` | 4 paragraphs incl. a multi-run paragraph |
| `tests/fixtures/sample.pptx` | `make_ooxml.py` | 3 slides (1, 2, 10 to prove numeric ordering) |
| `tests/fixtures/sample.xlsx` | `make_ooxml.py` | header + 2 data rows via shared strings |
| `tests/fixtures/apk/AndroidManifest.xml` | `make_apk.py` | compiled AXML: package, sdk, 2 permissions, exported activity, service, debuggable |
| `tests/fixtures/apk/classes.dex` | `make_apk.py` | DEX 035: 18 strings, 3 class defs, 8 methods, crypto/network/root/URL references |
| `tests/fixtures/apk/sample.apk` | `make_apk.py` | ZIP of the two above + META-INF, assets, fake arm64 lib |
| `tests/fixtures/sample.exe` | `make_pe.py` | PE32+ x86-64, real import table (KERNEL32), W+X high-entropy `.evil` section |

Regenerate with `python3 tests/fixtures/make_pdfs.py && python3 tests/fixtures/make_ooxml.py && python3 tests/fixtures/make_apk.py && python3 tests/fixtures/make_pe.py`.

## URL ingestion, refresh and sources (2026-10-31)

| Layer | Test | Proves |
|---|---|---|
| unit ×11 | `fetch::tests` | transport present; HTML fetched and classified; redirect followed hop-by-hop and recorded; redirect loop refused; size cap enforced; 404 reported; PDF detected by magic; private addresses refused when the override is off; bad schemes and embedded credentials refused; canonicalisation of equivalent URLs; body read complete |
| unit ×3 | `retrieve::tests` | decimals (`0.42`), abbreviations (`e.g.`, `Dr.`) and initials do not split sentences; newlines and `?` do; the scored sentence is the one containing the number |
| unit | `security::parse_url` via the above | IPv6 literal, port parsing, fragment stripping, default-port canonicalisation |
| e2e | `url_source_is_fetched_indexed_cited_and_deduplicated` | POST a URL → fetched from a real HTTP origin → title from `<title>` → chunks indexed → grounded answer contains `0.42` → citation points at that source → provenance (`canonical_url`, `http_status`, `content_hash`, `fetched_at`) persisted → same URL and a case/fragment variant both return `409 duplicate_source` naming the original |
| e2e | `refreshing_a_url_source_reindexes_only_when_content_changed` | artifact marked `stale` after a real content change; answer switches from `2.75` to `4.5`; a second refresh with unchanged content reports `changed:false` and re-indexes nothing |
| e2e | `pasted_sources_are_not_refreshable_and_labels_persist` | refresh of a pasted source → `422 not_refreshable`; PATCH renames and labels; both persist on re-read |
| e2e | `rejects_bad_input_and_dangerous_urls` | loopback, `169.254.169.254`, `file://`, `localhost` refused with `url_rejected`; a public host with no route is `502 fetch_failed` (**not** a policy refusal); an unresolvable name is also `fetch_failed` |
| UI | 3 assertions | the Add-from-URL control exists and its input is labelled; capabilities reports the fetch transport; a note offers conversion to a source |
| e2e | `note_can_become_a_cited_source` (Journey D) | note → source with `converted_from` provenance; second conversion refused `409 already_a_source`; a question about a number present only in the note is answered **and cited to the note-source**; deselecting it removes the influence; deleting the note leaves the source intact |

## Binary analysis (31 unit tests + 5 HTTP tests + 6 UI assertions)

Hash vectors (SHA-256/SHA-1 published values), entropy separation, AXML parsing of a real
compiled manifest, DEX inventory + finding categories, PE sections/imports/entropy/anomalies,
ELF identification, APK report assembly, format detection, base64 round-trip, and the HTTP
endpoint (raw bytes, base64 JSON, wrong method, empty body, unknown format).

## Not covered (honest gaps)

* No real browser engine: CSS layout is asserted at the media-query level only.
* No concurrency/load test; store writes are serialised by a mutex.
* No fuzzing of the PDF/ZIP parsers.
* No multi-megabyte corpus test.
