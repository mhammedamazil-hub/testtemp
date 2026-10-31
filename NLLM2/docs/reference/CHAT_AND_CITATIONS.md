# Chat and citations

## Reference behaviour

`OFFICIAL_DOCUMENTED` (page 16215270, read 2026-10-31): the model answers from the
sources in the notebook; sources can be selected individually in the Source panel to
scope a chat; summaries can be requested in chat or read from the **Source Guide**;
"mentioning source names in your query helps … narrow its search". The rename
announcement and help topic list describe chat styles (Default / Learning Guide /
Custom) and response lengths — **not verified in this session** (`INFERRED` from the
specification's inventory, `UNVERIFIED` for exact current labels).

`CODE_CORROBORATED`: the streamed-chat framing recorded in `docs/rea/` and implemented
in `crates/forge-core/src/notebooklm.rs` (quarantined, not used for Forge's own chat).

## Forge's contract (implemented)

1. **Scope**: only sources the user has selected; if none, the request is refused
   (`409 no_sources`) rather than answered from nothing.
2. **Retrieval**: lexical scoring over chunks, then **sentence-level** selection from
   the best chunk. A chunk is not quoted just because it matched.
3. **Evidence floor**: a sentence must overlap enough of the question's content words
   (`grounded::MIN_OVERLAP`) and live inside the chunk text, otherwise it is dropped.
4. **Abstention**: if nothing clears the floor, the answer says so — it never invents.
5. **Citations**: each claim carries `source_id`, `chunk_id`, the **exact quote**, and a
   locator (page / paragraph / row / section) that reopens the source at that spot.
6. **Verification**: the quoted span is checked against the chunk before the answer is
   emitted; a citation that fails verification is dropped, not shown.
7. **No outside knowledge**: the extractive engine adds nothing that is not in a source.

### Sentence segmentation (fixed 2026-10-31)

The splitter used to break on every `.`, so `"The evidence floor is 0.42"` became
`"The evidence floor is 0"` — a citation that loses the number the user asked for.
`retrieve::split_sentences` now keeps decimals (`0.42`), abbreviations (`e.g.`, `Dr.`,
`etc.`), initials, and URLs intact, and treats `;` as a continuation, not a boundary.
Verified by `split_sentences_keeps_decimals_and_abbreviations_intact` and by the
end-to-end URL test, which asserts the answer to "What is the evidence floor?" contains
`0.42`.
