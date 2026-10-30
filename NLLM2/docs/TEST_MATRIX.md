# Test matrix

## How to run

```bash
export PATH=/tmp/forge-rust/bin:$PATH      # this sandbox's Rust install
cd NLLM2
cargo test --offline                        # 75 tests: 70 lib + 5 e2e
```

UI drive-through (needs a running server and jsdom, dev-only):

```bash
FORGE_ADDR=0.0.0.0:8099 FORGE_DATA_DIR=/tmp/forge-live/data \
  FORGE_WEB_DIR=apps/web ./target/debug/forge-server &
NODE_PATH=/tmp/uitest/node_modules node tests/ui-smoke.js    # 27 assertions
```

## Unit tests (70) — exact names from `cargo test --lib -- --list`

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

### `grounded` (6)
`answers_with_a_valid_citation`, `abstains_when_evidence_is_absent`,
`abstains_on_empty_notebook`, `degenerate_question_is_detected`,
`synthesises_across_two_sources`, `validator_catches_fabricated_quotes_and_ids`.

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

## UI assertions (27, jsdom + live server)

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

Regenerate with `python3 tests/fixtures/make_pdfs.py && python3 tests/fixtures/make_ooxml.py`.

## Not covered (honest gaps)

* No real browser engine: CSS layout is asserted at the media-query level only.
* No concurrency/load test; store writes are serialised by a mutex.
* No fuzzing of the PDF/ZIP parsers.
* No multi-megabyte corpus test.
