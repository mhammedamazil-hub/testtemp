# Reference index

Every document under `docs/reference/` describes the **reference product**
(Gemini Notebook / NotebookLM), not Forge. Forge's own state lives in
`docs/STATUS.md`.

| Document | Covers | Evidence basis |
|---|---|---|
| `FEATURE_MATRIX.md` | one row per reference feature: evidence, Forge status, tests, gaps | documented + corroborated; no live observation |
| `SOURCE_MATRIX.md` | every documented source type, its limits, and Forge's parser for it | official help page 16215270 |
| `CHAT_AND_CITATIONS.md` | chat controls, citation behaviour, grounding contract | documented + Forge implementation |
| `STUDIO_ARTIFACTS.md` | artifact families and their controls | documented; provider-gated in Forge |
| `OFFICIAL_DOC_INDEX.md` | help-centre pages read, with URL and retrieval date | machine-recorded |
| `VIDEO_RESEARCH.md` | method for video evidence (no results — tools absent) | method only |

Not created yet, deliberately: `SCREEN_CATALOG.md`, `WORKFLOW_CATALOG.md`,
`SHARING_AND_PERMISSIONS.md`, `MOBILE_DIFFERENCES.md`, `LIMITS_AND_AVAILABILITY.md`.
Writing them now would mean inventing observations; the blockers are recorded in
`docs/RESEARCH_BLOCKERS.md`. They are scheduled work, not silent omissions.

## Evidence labels in use

`OFFICIAL_DOCUMENTED` · `CODE_CORROBORATED` · `INFERRED` · `BLOCKED`

`LIVE_OBSERVED` and `VIDEO_OBSERVED` are **not** used anywhere in this tree, because
neither is possible in this environment.
