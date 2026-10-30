# REA investigation — evidence index

## Verdict

**BLOCKED.** The REA (reverse-engineering-agent) investigation workflow could not be
run against Gemini Notebook / NotebookLM from this environment. No observation of
the live product is recorded in this project, and no parity row is based on it.

## What was attempted

| Attempt | Command / action | Result |
|---|---|---|
| Install the REA agent package | `npm view rea-agents versions` → `6.8.0`; `npm install rea-agents` | **refused**: package requires Node ≥ 22.23.4; sandbox has **v22.22.3** |
| Use the PyPI `rea` package as a substitute | `pip download rea` | **not applicable**: the PyPI package named `rea` is an unrelated astronomy application |
| Reach the live product | no browser session, no authenticated Google account, no outbound route to `notebooklm.google.com` | **no access**; the earlier VM session's Browserless/noVNC path is DOWN |
| Fetch public product docs | outbound network limited to GitHub/PyPI/npm endpoints | **no general web fetch** in the sandbox |

Exact evidence commands (reproducible):

```bash
node --version                       # v22.22.3
npm view rea-agents version engines  # 6.8.0, engines.node >=22.23.4
```

## Consequence for this project

* The parity matrix (`FEATURE_PARITY.md`) is built from the brief's requirement
  text and generally documented product behaviour, labelled
  **REFERENCE-DOCUMENTED**.
* Every statement about Forge itself is verified by the tests named in
  `TEST_MATRIX.md`.
* If REA becomes runnable (Node upgrade or a reachable product session), the
  investigation procedure is:

  1. Record the product surface (source types, buttons, menus) with screenshots and
     the exact UI strings.
  2. For each capability, capture observable behaviour for a known input.
  3. Diff against `FEATURE_PARITY.md` and correct any REFERENCE-DOCUMENTED row to
     VERIFIED, or downgrade Forge's claim.
  4. Store artefacts under `docs/rea/<date>/` with hashes.

## What is NOT claimed

No screenshot, DOM dump, HAR file or session recording of NotebookLM exists in this
repository. Any document implying otherwise would be fabricated evidence.
