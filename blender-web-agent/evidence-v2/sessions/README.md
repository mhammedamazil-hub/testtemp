# Session evidence

Each directory is one controller session (`session.json`, `trace.jsonl`, per-step
`plan.json` / `result.json` / `console.log`, restored scene snapshots, and the
artifacts kept by the controller).

| session | what it demonstrates |
|---|---|
| `20261025-105811-358937` | **the model-driven loop**: 5 steps, inspect→correct (cockpit 482→62 verts, wing gap 5.4 cm→0), export, retry-safety test, final delivery |
| `20261025-110306-3cb959` | recovery from `TARGET_NOT_FOUND`: error returned with scene state, corrected plan succeeds |
| `20261025-110242-fc84d8` | recovery from `RUNTIME_TIMEOUT`: no artifacts, no snapshot overwrite, session stays resumable |
| `20261025-110107…110227` | the five delivery scenarios (`s1`–`s5`), one session each |
| `20261025-110009…110047` | the **first** delivery run, which failed 2/5 — kept because it is where the material-purge and zero-thickness bugs were found |

`trace.jsonl` is append-only: every `execution_started` for step *n+1* occurs
after `execution_finished` for step *n*, which is what makes the loop's
"results before decisions" ordering auditable.
