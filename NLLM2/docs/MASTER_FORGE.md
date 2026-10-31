# MASTER_FORGE.md
## Forge — Independent Research, Knowledge, and Learning Workspace
### Master product specification, reverse-engineering research plan, implementation contract, and acceptance programme

**Document type:** Authoritative build brief for the Arena coding agent  
**Product:** Forge  
**Reference product:** NotebookLM, officially renamed **Gemini Notebook** by Google on 16 July 2026  
**Reference URL:** https://notebook.google.com/  
**Research baseline:** 30 October 2026  
**Implementation philosophy:** Evidence-first, Rust-first, source-grounded, independently engineered, production-oriented  
**Status:** Living specification. Update it as evidence changes. Do not silently delete requirements.

---

# 0. Read this first — non-negotiable mission

You are building **Forge**, a complete, independent, production-quality research and knowledge workspace inspired by the product experience and capabilities of NotebookLM / Gemini Notebook.

You are **not** building an agent-engineering workbench, an operating system, a generic chatbot with file upload, a UI mock-up, a static demo, a CLI-only proof of concept, a Google NotebookLM API wrapper, or a clone that depends on undocumented Google endpoints.

Forge must become a useful application in its own right. It should reach feature parity where practical, then continue beyond parity with capabilities that make it more useful, reliable, extensible, and transparent.

The core product promise:

> Bring together sources, understand them, ask questions grounded in those sources, inspect the evidence, discover connections, and turn the knowledge into useful outputs.

The user must be able to trust the distinction between:
1. what a source explicitly says;
2. what multiple sources jointly support;
3. what the model infers;
4. what is uncertain or unsupported.

**Never fabricate a source, quotation, citation, page, timestamp, result, test, integration, or capability.** An honest unsupported result is better than a plausible invented one.

## 0.1 Work autonomously, but do not fake certainty

Research, inspect, implement, test, document, and improve Forge continuously. Do not stop after producing a plan or a feature matrix. Do not ask the user to approve every ordinary implementation decision.

Stop and request the user's decision only when a decision is genuinely user-owned or risky, such as spending money, activating a paid service, using a personal account or credentials that the user has not authorized, making a public release or irreversible destructive change, changing product direction, or removing established functionality.

Do not wait for approval to fix ordinary bugs, add tests, improve documentation, or implement a clearly specified feature.

## 0.2 Existing project must be preserved

The most recent user-provided progress report described:
- a Rust-first codebase under `NLLM2/`;
- a `forge-core` crate and an additional `bininspect` crate;
- source ingestion, grounded answer and artifact functionality;
- an HTTP server and browser UI;
- a NotebookLM protocol experiment in `crates/forge-core/src/notebooklm.rs`;
- a binary-inspection capability;
- a reported baseline of 128 passing tests and 36 passing UI smoke assertions;
- reported commits `3ad0ce8` and `a3b6bd7`, pushed to `arena/e86028ee-testtemp`.

These are **historical claims supplied by the user from an agent report**, not independently verified facts about the current checkout. First inspect the actual repository, branch, remotes, commit graph, working tree, tests, and running service. The sandbox may have been recycled.

Before changing anything:
1. run `pwd`, `git status --short --branch`, `git remote -v`, `git log --oneline -12`, and inspect the current branch;
2. identify uncommitted work and backup refs;
3. verify that the expected project directory exists;
4. inspect the actual `Cargo.toml` workspace and crate structure;
5. inspect current documentation and tests;
6. record the actual baseline in `docs/STATUS.md`;
7. preserve all user and agent work.

**Never reset, clean, force-push, overwrite, or re-clone over uncommitted work.** If the sandbox was reset, recover from the remote carefully and verify file hashes before moving refs or replacing a checkout.

## 0.3 Reference product naming

Google announced that NotebookLM was renamed **Gemini Notebook** on 16 July 2026. The product remains the relevant standalone research application, with additional connections to the wider Gemini ecosystem.

Use both names in research:
- “Gemini Notebook” for the current official name and official documentation;
- “NotebookLM” for historical documentation, older videos, existing community terminology, and protocol research.

Do not assume every old NotebookLM tutorial describes the current product. Date every finding.

## 0.4 Build independently

The reference product is an evidence source, not a software dependency.

- Do not copy Google's proprietary frontend, private code, protected assets, branding, or internal services.
- Do not make Forge depend on undocumented Google RPCs.
- Do not bypass authentication, quotas, access controls, DRM, or paywalls.
- Do not use private source data or session credentials without authorization.
- Do not misrepresent the relationship between Forge and Google.
- Implement equivalent user value using Forge-owned code, documented APIs, properly licensed libraries, and user-authorized providers.

The experimental NotebookLM RPC work may remain as a quarantined research module, but it must not become Forge's primary backend.

---

# 1. Product definition and quality bar

## 1.1 Product definition

Forge is a persistent, multi-source research workspace. A user creates notebooks, imports material, organizes sources, asks source-grounded questions, saves notes, generates study and presentation artifacts, and revisits the complete research history.

It must support:
- persistent notebooks and projects;
- source ingestion from multiple formats and origins;
- robust document parsing and normalization;
- source-level and notebook-level understanding;
- hybrid search and source retrieval;
- evidence-grounded chat with precise citations;
- notes and annotations;
- generated study materials and reports;
- generated audio and visual explainers where actual engines are available;
- web research and source discovery;
- source management, refresh, and deduplication;
- export, backup, restore, and sharing;
- reliable operation when optional providers are unavailable.

## 1.2 Product values

1. **Evidence before eloquence.** The answer must be supported by selected sources.
2. **Exact citations.** A citation must take the user to the exact relevant evidence, not merely to a source's first page.
3. **Source transparency.** Show the origin, extraction status, date, and limitations of every source.
4. **Honest capabilities.** Never label an unavailable, mocked, stubbed, or untested feature as complete.
5. **User control.** Users choose source scope, provider, output format, sharing level, and deletion.
6. **Persistent work.** Notebooks, notes, citations, artifacts, and conversation state survive restarts.
7. **Low-friction UX.** Powerful features should not make basic use confusing.
8. **Modularity.** New parsers, providers, retrieval algorithms, and artifact generators should be pluggable.
9. **Security by default.** Imported material is untrusted data. Never execute it.
10. **Independent product identity.** Forge should have original product design, code, and branding.

## 1.3 Definition of production-ready

A feature is not production-ready merely because a route exists, a button renders, a test fixture passes, the backend returns HTTP 200, a model generated plausible text, or the feature appears in a capability table.

A production feature must have:
- a real backend implementation;
- a real UI workflow;
- persistent data model where persistence is expected;
- validation and error handling;
- security review appropriate to its risk;
- unit and integration tests;
- user-visible loading, empty, success, and failure states;
- documentation;
- evidence from representative real inputs;
- graceful degradation when dependencies are absent.

---

# 2. Reference intelligence: current Gemini Notebook / NotebookLM

This is a **research inventory**, not a claim that every feature is already fully understood. Official Google Help Center pages and release announcements are the primary source of truth for documented capabilities. Actual UI inspection and controlled testing are required to understand current workflow details.

## 2.1 Product shell and notebook lifecycle

Research and reproduce the complete observable lifecycle:
- home / notebook library;
- create notebook;
- rename notebook;
- notebook icon / emoji or visual identity;
- notebook title and metadata;
- recent notebooks;
- sort, search, and filter;
- open notebook;
- notebook loading and empty states;
- delete notebook and confirmation;
- duplicate/copy behaviour if available;
- notebook ownership;
- notebook access levels;
- notebook sharing;
- viewer and editor roles;
- public notebook sharing;
- chat-only / focused view links;
- featured and public notebooks;
- notebooks appearing in Gemini Apps;
- notebook sync or cross-product availability;
- account and plan indicators;
- usage/quota indicators;
- settings;
- language preferences;
- feedback and reporting controls;
- mobile vs desktop differences.

For each workflow record screen dimensions, responsive states, visible controls, keyboard behaviour, click sequence, modal/menu contents, confirmation behaviour, navigation changes, persistence after reload, error behaviour, access restrictions, actual screenshot, and source evidence.

Do not infer that hiding an item in a “chat view” revokes access to it. Examine actual sharing semantics and permissions.

## 2.2 Sources: import, discovery, processing, organization

Research every currently supported source type. Google's public help currently lists a broad set that includes:
- PDF;
- Microsoft Word / DOCX;
- PowerPoint / PPTX;
- Google Docs;
- Google Slides;
- Google Sheets;
- CSV;
- TXT;
- Markdown;
- pasted text;
- web URLs;
- public YouTube video URLs;
- audio files;
- images;
- ePub;
- eligible Play Books / textbook sources;
- Gemini chat threads;
- files from Google Drive.

Exact availability and limits can vary by account, plan, country, age, rollout, and device. **Inspect and record values visible in the current test environment; do not hard-code Google's published limits as Forge limits.**

For every source type, research:

### A. Add flow
- Add Sources button;
- drag and drop;
- file picker;
- multiple-file selection;
- URL input;
- pasted text;
- Drive picker;
- web search;
- research question input;
- YouTube URL;
- audio upload;
- image upload;
- import progress;
- duplicate-source behaviour;
- cancellation;
- retry.

### B. Validation and restrictions
- extension and MIME acceptance;
- maximum size;
- word/token/page/slide limits;
- source count limits;
- unsupported or damaged file behaviour;
- password-protected documents;
- paywalled websites;
- private or unlisted media;
- public YouTube caption requirements;
- newly uploaded video delay;
- unsupported languages;
- OCR requirements;
- audio without speech;
- file permission changes;
- deleted upstream files;
- plan and device restrictions.

### C. Processing lifecycle
Capture every visible state: queued, uploading, fetching, extracting, parsing, transcribing, indexing, summarizing, ready, partial, failed, retrying, and removed.

Determine whether import is a fixed snapshot, periodically synchronized, manually refreshable, linked to an upstream file, or reprocessed on changes.

Google's help documentation describes Drive-imported sources as syncing periodically and records source-specific caveats for URLs and YouTube. Verify current behaviour rather than extrapolating to every source type.

### D. Source list and organization
Research source cards/icons; selected/unselected state; select all/clear selection; per-source menu; rename; remove; refresh/sync; labels and categories; automatic categorization; manual label create/rename/delete; source count; source guide/summary; source opening and preview; source status; failed-source diagnostics; filters; selection persistence; source selection for chat versus artifacts.

### E. Source preview and evidence navigation
Research source viewer layout, text rendering, page/slide navigation, zoom, search within source, source summary, citation highlighting, citation jump position, image display, table rendering, page number display, quote hover cards, and source-specific viewer behaviour.

## 2.3 Chat and evidence-grounded answering

Research the full chat experience:
- initial prompt suggestions;
- prompt entry and send;
- stop generation;
- continue generation;
- revise prompt while running;
- loading/thinking indicators;
- streaming response;
- answer formatting;
- inline citations;
- citation hover preview;
- citation click navigation;
- source selection controls;
- use all selected sources;
- answer from a specific subset;
- conversational follow-up;
- regenerate;
- copy;
- feedback;
- save response as note;
- chat history;
- delete history;
- error/retry;
- empty-context behaviour;
- unsupported-question behaviour;
- long-answer behaviour;
- tables and structured outputs;
- multilingual responses;
- references to contradictory material;
- answer style and length preferences.

Officially documented styles have included Default, Learning Guide, and Custom instructions/custom style. Officially documented response lengths have included Default, Longer, and Shorter. Verify current labels and exact behaviour. Research any newer Custom Chat controls, role/persona controls, analytics, advanced reasoning, tool use, and provider/model selection.

### Required Forge chat capabilities

Implement notebook-scoped conversation threads, source-scoped and notebook-scoped context, explicit active-source indicator, conversation persistence, streaming, cancellation, follow-up questions, configurable response style/length, user custom instructions, copy/save actions, exact evidence citations, citation feedback, answer regeneration, and clear “not enough evidence” state.

### Grounding contract

Every factual claim presented as source-grounded must point to one or more evidence spans. Never attach a citation to a paragraph merely because the source is broadly related.

The answer engine must:
1. parse the question;
2. identify active source scope;
3. retrieve candidate chunks;
4. rerank them;
5. select evidence;
6. generate an answer under a grounded prompt;
7. validate citations and source coverage;
8. remove unsupported claims or abstain;
9. render citations and evidence previews.

Support direct quote, paraphrase, multi-source synthesis, comparison, chronology, definition, extraction, table answers, image/diagram answers where available, “what is missing?” questions, conflict/contradiction reporting, and “show the evidence” follow-ups.

Citations should carry source ID, chunk ID, exact text span, page/slide/time/cell location, and enough information to reopen the source at the correct place.

## 2.4 Source discovery and research

Investigate current official source-discovery tools and how they interact with notebooks.

### Fast Research
Research Web vs Drive source search, query entry, result cards, source relevance explanation, title/snippet, open original source, select/deselect results, import one or many, cancellation, partial results, duplicate handling, and source attribution.

### Deep Research
Research where it appears; account/age/plan restrictions; research prompt; research scope; progress and intermediate state; report format; sources discovered; cited vs uncited sources; review and selection; import into notebook; cancel/minimize; partial import on usage limits; follow-up research; provenance and citations.

Do not state that Forge's web search is Deep Research unless it genuinely plans and performs a multi-step research task.

### Forge research implementation goals
- provider-independent web search adapter;
- multiple search providers where feasible;
- source credibility and freshness metadata;
- query planning;
- sub-question decomposition;
- domain diversification;
- page retrieval and readable extraction;
- duplicate and near-duplicate detection;
- claim/evidence mapping;
- contradiction detection;
- source review UI;
- import-selected-results workflow;
- source attribution;
- explicit source date and retrieval date;
- research report with references;
- cancellation and budget controls.

Treat web pages, comments, PDFs, and source text as untrusted input. Prompt injection found in source material must not override system instructions, access controls, or user settings.

## 2.5 Notes and noteboard

Research create note, edit note, paste text, save chat response, preserve tables and citation links, notes appearing in source list, notes selected as context, notes used for Q&A, notes used for artifacts, old Studio notes, convert note to source, convert all notes, export to Google Docs, note title/rename, save state, delete/recovery, citation preservation, and mobile limitations.

Forge must provide rich text/Markdown notes, autosave with clear persistence feedback, user-authored notes, saved-answer notes, source-linked annotations, pinned notes, labels, note search, note-to-source conversion, notes in retrieval/artifact generation, and export to Markdown, HTML, PDF, and DOCX where implemented.

## 2.6 Studio: every artifact family

Studio is a major part of the reference product. Investigate each artifact as a separate product workflow. Record customizations, source scope, prompt fields, output variants, generation lifecycle, viewer, editing, export, download, sharing, deletion, regeneration, loading previous artifacts, and known limits.

### A. Audio Overview
Investigate available formats; default format; Deep Dive and other formats shown in the current UI; host style and conversation structure; language; custom prompt/instructions; source selection; generation queue; background generation; concurrent artifact generation; player; play/pause; playback speed; seek; duration; transcript/caption availability; download; sharing; load previous result; delete; regenerate; view custom prompt; accessibility; interactive mode if present; and mobile behaviour.

Audio must be grounded in selected notebook sources. Where multiple hosts are used, investigate voices, host roles, interruptions, transitions, and format customization.

### B. Video Overview
Investigate Explainer, Short, Cinematic, current age/plan restrictions, supported narration languages, visual styles, custom styles, steering prompt, source selection, producer notes/outline review if present, generation now vs later, background jobs, player, playback speed/seeking, fullscreen, download/sharing, feedback, generation failure, visual inaccuracies/narration glitches, and artifact re-opening.

Treat video generation as a real multimedia pipeline, not a slideshow with a video label.

### C. Mind Maps
Investigate generation entry points, topic hierarchy, node and edge types, expansion/collapse, zoom/pan, node selection, ask chat about a node, regeneration, download, source association, persistence, loading an old map, and error/large-map behaviour.

Forge mind maps must support node-to-evidence links and preserve provenance.

### D. Flashcards
Investigate customization, difficulty, prompt/instructions, quantity, audience, source scope, card generation, front/back, flip, next/previous, shuffle, progress, known/unknown, incorrect answers, revise/regenerate, persistence, export, and mobile interactions.

### E. Quizzes
Investigate difficulty, question count, question formats, prompt customization, source scope, answer options, submit, feedback, explanation, cited evidence, correct/incorrect state, score, retake, question navigation, progress, regeneration, and export.

Do not create questions that cannot be answered from selected sources. Every correct answer and explanation must be evidence-backed.

### F. Reports
Investigate all report formats, including structured text-only reports, interactive reports, learning overview, briefing document, study guide, FAQ, timeline, comparison, other current templates, embedded Studio artifacts, custom prompt, output structure, editing, export, share, regenerate, delete, source scope, and age/plan restrictions.

Do not assume an old list of report templates is complete. Discover currently visible templates.

### G. Infographics
Investigate output language, detail level, orientation, visual style, custom style, prompt, source selection, generated image dimensions, image quality, text legibility, downloadable PNG, sharing, regeneration, editing/revision, feedback, and age/plan restrictions.

### H. Slide Decks
Investigate Detailed Deck vs Presenter Slides or current formats; language; short/default/long length; prompt; outline generation; review outline notes; revise outline before final render; slide ordering; slide count; visual style; slide content; source citations/source-checking; edit existing slides; revise text/layout/visuals; add/delete/restore/reorder slide; pending changes; regenerate revised deck; fullscreen slideshow; keyboard navigation; download PDF; download PPTX; share; load prior deck; custom prompt; limits and restrictions.

A Forge deck editor must not pretend source verification was done when a revision workflow does not use the original sources.

### I. Data Tables and structured extraction
Investigate whether Data Tables are present in the current plan/account; how the user describes the requested table; row/column inference; schema editing; source selection; citation/evidence per cell; export; rerun; partial/missing values; data type validation; CSV/XLSX compatibility; custom fields; and limits.

Forge should make structured extraction auditable: every extracted value should have a source reference or an explicit missing/unknown value.

### J. Other Studio outputs
Discover all currently offered output types. Do not limit research to this list. Add newly discovered tools to `docs/reference/FEATURE_MATRIX.md` and this master specification.

## 2.7 Connected apps, Gemini integration, and code execution

Google's 2026 documentation describes connected apps in chat and notebooks appearing in Gemini Apps. The July 2026 rename announcement also discussed code execution and notebook synchronization in the wider Gemini ecosystem.

Research connected-app catalogue; app names and supported actions; enable/disable at global and notebook level; permission prompts; app action execution; data sent to each app; output confirmation; error states; app connection removal; custom chat settings; shared notebook semantics; Gemini chat sources; notebook availability in Gemini Apps; differences in source grounding between Gemini Notebook and Gemini Apps; code execution location/isolation; downloadable outputs; chart/file/image generation; execution trace visibility; security; privacy; quotas; and plan restrictions.

Forge must use a pluggable connector system. Each connector must declare provider, scopes/permissions, operations, input schema, output schema, user approval requirement, data-sharing disclosure, rate limits, and error semantics.

Never silently call a connected app. Show what operation will be performed and what information will be sent when the action is consequential.

Code execution is a separate sandboxed capability. Never run imported source code on the host. If Forge later executes generated code, use an isolated environment with resource limits and filesystem/network controls.

## 2.8 Sharing, collaboration, and permissions

Research private notebook, invite by email, viewer/editor roles, public link, public notebook discovery, share artifact link, chat view link, access revocation, permission inheritance, what a viewer can actually access, what an editor can change, source visibility, artifact visibility, public sharing restrictions by account type, group sharing, collaborator count, ownership transfer if supported, deletion of owner/collaborator, and link invalidation after artifact deletion.

Forge permissions must be explicit and enforced server-side. UI hiding is not authorization.

## 2.9 Mobile, responsive, and accessibility

Research desktop, tablet, narrow phone, touch interactions, keyboard shortcuts, navigation, source viewer, chat, Studio, artifact player, downloads, file picker, offline state, background processing, sync delay, theme, font scaling, screen reader labels, focus order, contrast, and reduced motion.

Forge should be responsive and usable on a tablet. Do not make desktop-only workflows inaccessible without stating the limitation.

## 2.10 Accounts, plans, limits, privacy, and availability

Research standard/free access, paid tiers, account type differences, age restrictions, region restrictions, usage quotas, rolling reset windows, notebook/source limits, feature-specific generation limits, compute-based limits, queued/generate-later operations, automatic suggestions, watermarking, access to advanced models, model availability, privacy/data handling, training policy, human review policy, Workspace/Education distinctions, retention/deletion, and user feedback handling.

**Never copy a quota table into Forge as a hard-coded product limit.** Google's limits change. Forge's own resource limits must be independently designed and configurable.

---

# 3. Evidence system: how to know what is true

## 3.1 Required evidence labels

| Label | Meaning | Allowed wording |
|---|---|---|
| `LIVE_OBSERVED` | Directly observed in live application using an authorized account/session | “Observed in the current UI” |
| `OFFICIAL_DOCUMENTED` | Explicitly described by official Google source | “Google documents…” |
| `VIDEO_OBSERVED` | Visible/audible in recorded demonstration, with timestamp | “Shown at 02:14 in this video” |
| `COMMUNITY_REPORTED` | Community report; useful lead, not authoritative | “A user reports…” |
| `CODE_CORROBORATED` | Supported by publicly available code or trace | “This implementation suggests…” |
| `INFERRED` | Plausible interpretation not directly verified | “Likely…” / “Inferred…” |
| `UNVERIFIED` | Not enough evidence | “Unknown; needs investigation” |
| `BLOCKED` | Could not investigate due to environment/access constraints | Explain exact blocker |

Do not upgrade a status because multiple websites repeat the same copied claim.

## 3.2 Independent corroboration

When a claim is supported by several codebases or articles, check whether they share the same upstream library, whether they copied the same blog post or captured trace, inspect commit history and publication dates, distinguish independent experiments from forks, cite the exact file/line/commit or capture, and record the number of genuinely independent confirmations.

Three repositories copied from one reverse-engineering project count as one underlying evidence lineage, not three independent confirmations.

## 3.3 Evidence record template

```yaml
feature_id: SRC-YOUTUBE-001
name: Import public YouTube video
status: OFFICIAL_DOCUMENTED
checked_at: 2026-10-30
product_version_or_date: "current web UI as observed"
account_context:
  plan: "record actual plan or UNKNOWN"
  country: "record only if relevant"
  device: "desktop/tablet/mobile"
steps:
  - "Exact sequence"
expected_result: "..."
actual_result: "..."
limitations:
  - "..."
evidence:
  - type: official_documentation
    url: "https://support.google.com/gemininotebook/answer/16215270"
  - type: screenshot
    path: "research/evidence/screenshots/source-youtube-import.png"
  - type: video
    path: "research/evidence/videos/<id>.mp4"
    timestamp: "00:42-01:13"
confidence: high
open_questions:
  - "..."
forge_requirement: "..."
acceptance_test: "..."
```

Do not store cookies, access tokens, private user data, or unredacted authorization headers in evidence.

## 3.4 Required living research documents

Create and keep current:

```text
docs/
  STATUS.md
  DECISIONS.md
  USER_REQUIREMENTS.md
  FEATURE_PARITY.md
  RESEARCH_PLAN.md
  RESEARCH_LOG.md
  RESEARCH_BLOCKERS.md
  UNKNOWN_AND_LIMITATIONS.md
  SECURITY.md
  TEST_MATRIX.md
  ARCHITECTURE.md
  DATA_MODEL.md
  API_CONTRACT.md
  DEPLOYMENT.md
  PRIVACY.md
  ROADMAP.md
  reference/
    REFERENCE_INDEX.md
    FEATURE_MATRIX.md
    SCREEN_CATALOG.md
    WORKFLOW_CATALOG.md
    SOURCE_MATRIX.md
    CHAT_AND_CITATIONS.md
    STUDIO_ARTIFACTS.md
    SHARING_AND_PERMISSIONS.md
    MOBILE_DIFFERENCES.md
    LIMITS_AND_AVAILABILITY.md
    PROTOCOL_EVIDENCE.md
    VIDEO_RESEARCH.md
    REA_CAPABILITY_REPORT.md
    evidence/
      screenshots/
      contact-sheets/
      frames/
      transcripts/
      metadata/
      network-shapes/
      rea/
```

Use a separate `research/` root instead if that matches the existing repository. Do not create duplicate documentation trees unnecessarily. Update existing documents when they already serve the same purpose.

## 3.5 Feature matrix schema

Every reference feature must appear in a single canonical matrix with: feature ID; category; user value; UI entry point; prerequisites; inputs; outputs; workflow; options; states; error cases; account/plan/device restrictions; evidence labels; evidence links; Forge implementation module; implementation status; tests; known gaps; and priority.

No feature is “complete” if its error states, persistence, and tests are missing.

---

# 4. Research workflow — investigate the real product systematically

## 4.1 Source priority

Research in this order:
1. **Live product** — authorized, direct observation.
2. **Official Google Help Center** — detailed feature workflow and restrictions.
3. **Official Google product announcements and Workspace documentation** — launches, changes, rollout, plan differences.
4. **Official demonstrations and events** — visual behaviour.
5. **Independent current tutorials** — workflows not covered by official documentation.
6. **Community discussions** — bugs, edge cases, user pain points, hidden behaviour.
7. **Public GitHub projects and code** — technical corroboration, parsers, protocol research, examples.
8. **Reverse-engineering and captured traces** — only when collected lawfully and with authorization.

Do not begin from a random blog and treat it as canonical.

## 4.2 Mandatory official starting points

Use these exact starting points and follow related links:

- Gemini Notebook Help Center: https://support.google.com/gemininotebook/
- Learn about Gemini Notebook: https://support.google.com/gemininotebook/answer/16164461
- Create a notebook: https://support.google.com/gemininotebook/answer/16206563
- Add or discover sources: https://support.google.com/gemininotebook/answer/16215270
- Use chat: https://support.google.com/gemininotebook/answer/16179559
- Create and add notes: https://support.google.com/gemininotebook/answer/16262519
- Mind Maps: https://support.google.com/gemininotebook/answer/16212283
- Audio Overview: https://support.google.com/gemininotebook/answer/16212820
- Video Overview: https://support.google.com/gemininotebook/answer/16454555
- Flashcards / Quizzes: https://support.google.com/gemininotebook/answer/16958963
- Slide Decks: https://support.google.com/gemininotebook/answer/16757456
- Infographics: https://support.google.com/gemininotebook/answer/16758265
- Connected apps: https://support.google.com/gemininotebook/answer/18784282
- Public / featured notebooks: https://support.google.com/gemininotebook/answer/16322204
- Plans and limits: https://support.google.com/gemininotebook/answer/16213268 and https://support.google.com/gemininotebook/answer/17670842
- Notebooks in Gemini Apps: https://support.google.com/gemininotebook/answer/17003757
- Google rename announcement: https://blog.google/innovation-and-ai/products/gemini-notebook/notebooklm-gemini-notebook/
- Flexible usage limits announcement: https://blog.google/innovation-and-ai/products/gemini-notebook/new-flexible-usage-limits/

Some official pages may be region-specific or rate-limited. Try equivalent official locale URLs, search by exact page title, and record any retrieval failure. A failed fetch is not evidence that the feature does not exist.

## 4.3 Controlled browser research

If the environment provides a browser:
1. determine whether the browser is installed and can open the product;
2. determine whether a persistent profile is available;
3. use an authorized account/session only;
4. let the user complete login and any multi-factor verification;
5. never ask the user to paste passwords or authentication codes into the research log;
6. keep cookies and credentials in the browser's protected profile; do not export or print them;
7. browse the product screen by screen;
8. record screenshots at every significant UI state;
9. record exact click paths and visible text;
10. test ordinary, authorized product actions;
11. repeat with different source types and account/device states where possible.

If the agent cannot access the live product, record the exact network/browser blocker. Continue with official docs and public demonstrations. Do not pretend live inspection occurred.

### Browser investigation checklist

- [ ] Product home
- [ ] Empty notebook
- [ ] New notebook
- [ ] Notebook menu
- [ ] Add-source panel
- [ ] Each source type
- [ ] Source processing state
- [ ] Source list and selection
- [ ] Source preview
- [ ] Source Guide / summary
- [ ] Chat configuration
- [ ] Chat response
- [ ] Citation hover
- [ ] Citation click/jump
- [ ] Save response
- [ ] Notes
- [ ] Fast Research
- [ ] Deep Research
- [ ] Every Studio artifact
- [ ] Artifact customizations
- [ ] Artifact viewer
- [ ] Artifact edit/revise
- [ ] Artifact export/download
- [ ] Sharing dialog
- [ ] Viewer role test
- [ ] Editor role test
- [ ] Settings
- [ ] Usage/quota
- [ ] Mobile/tablet
- [ ] Errors and unsupported cases

### Test notebook corpus

Create a small, lawful, controlled reference corpus with:
- a 3-page PDF with known page text;
- a DOCX with headings, tables, footnotes, and hyperlinks;
- a PPTX with text and images;
- a spreadsheet/CSV with known values;
- a web page with headings, links, and a table;
- a public captioned YouTube video;
- a public video with little/no speech if legally usable;
- a short audio recording with known transcript;
- two sources that agree;
- two sources that contradict;
- a source with a fact deliberately absent;
- a source with prompt-injection text;
- a multilingual source;
- a large source;
- a malformed source.

Use this corpus to compare actual behaviour, but never upload private or copyrighted material without appropriate rights.

---

# 5. YouTube video research: yt-dlp + FFmpeg + visual inspection

This section is mandatory for tutorial/demonstration videos that are useful evidence of UI behaviour. It is not a general permission to download any YouTube video.

## 5.1 Legal and platform boundaries

Use `yt-dlp` only for material Arena is authorized to download and analyze: videos owned by the user; videos for which the user has permission; videos under a licence permitting the intended download and analysis; or other material where downloading is lawful and compliant with applicable platform terms.

Prefer official downloadable source files when provided.

Do **not** bypass DRM or technical protection; bypass private/unlisted access restrictions; download paid or access-controlled material without authorization; use stolen cookies or session files; access content the operator is not authorized to access; redistribute downloaded videos or extracted media without rights; or interpret public visibility as automatic permission to republish.

When in doubt, use the official page, embedded player, available captions, screenshots taken during authorized viewing, and timestamped notes rather than downloading the video.

## 5.2 Check tools first

```bash
command -v yt-dlp
yt-dlp --version
command -v ffmpeg
ffmpeg -version
command -v ffprobe
ffprobe -version
```

If missing, inspect the current OS and installation options. Do not blindly install packages globally in an immutable/restricted sandbox. Use an isolated environment or available package manager where appropriate.

Official documentation:
- yt-dlp repository / README: https://github.com/yt-dlp/yt-dlp
- yt-dlp options: `yt-dlp --help`
- FFmpeg documentation: https://ffmpeg.org/documentation.html
- FFmpeg filters: https://ffmpeg.org/ffmpeg-filters.html
- FFprobe documentation: https://ffmpeg.org/ffprobe.html

Check version-specific flags before relying on them.

## 5.3 Inspect metadata before download

For a permitted public video:

```bash
mkdir -p research/evidence/videos
yt-dlp --no-playlist --skip-download --dump-single-json \
  "VIDEO_URL" \
  > research/evidence/videos/video-metadata.json
```

Record video ID, title, channel/uploader, canonical URL, upload date, duration, description, chapters, available formats, available subtitle languages, licence metadata if present, and retrieval date. Metadata is evidence about what the extractor reported at that time; it is not a guarantee that the content's licence is valid.

## 5.4 Download permitted media and preserve metadata/captions

For authorized videos only:

```bash
mkdir -p research/evidence/videos
yt-dlp \
  --no-playlist \
  --write-info-json \
  --write-thumbnail \
  --write-subs \
  --write-auto-subs \
  --sub-langs "en.*,en" \
  --convert-subs srt \
  -f "bv*+ba/b" \
  --merge-output-format mp4 \
  -o "research/evidence/videos/%(id)s.%(ext)s" \
  "VIDEO_URL"
```

Notes:
- `bv*+ba/b` prefers a video-containing best stream plus best audio when needed, with a best combined fallback.
- `--write-info-json` preserves extractor metadata.
- `--write-subs` requests uploader-provided subtitles.
- `--write-auto-subs` requests auto-generated subtitles.
- `--sub-langs` controls requested language patterns.
- If subtitles are unavailable, record that fact. Do not invent a transcript.
- Inspect available languages first:

```bash
yt-dlp --no-playlist --list-subs "VIDEO_URL"
yt-dlp --no-playlist --list-formats "VIDEO_URL"
```

If authorized, select other language patterns explicitly. Preserve original subtitle files as evidence. Convert copies for convenience if needed.

Do not use cookies by default. Do not retrieve browser cookies into research files. If an authorized workflow genuinely requires an authenticated session, have the account owner operate the browser and keep credentials inside the protected browser session. Do not export the session.

## 5.5 Inspect the actual media with FFprobe

```bash
VIDEO="research/evidence/videos/VIDEO_ID.mp4"

ffprobe -v error \
  -show_format \
  -show_streams \
  -show_chapters \
  -of json \
  "$VIDEO" \
  > research/evidence/videos/VIDEO_ID.ffprobe.json
```

Record duration, frame width/height, average and real frame rate, codec, pixel format, audio streams, sample rate, channel layout, subtitle streams, chapters, rotation/display matrix, and missing/corrupt packets if detectable. Do not assume extension accurately describes the stream/container.

## 5.6 Generate full-video visual contact sheets

The purpose is to visually inspect the whole demonstration, not just read its transcript.

Get duration:

```bash
DURATION="$(
  ffprobe -v error \
    -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 \
    "$VIDEO"
)"
printf 'Duration: %s seconds\\n' "$DURATION"
```

Generate a coarse, full-duration 48-frame overview, sampled across the timeline, as a 6-column by 8-row contact sheet:

```bash
RATE="$(awk -v d="$DURATION" 'BEGIN {
  if (d > 0) printf "48/%f", d;
  else print "1";
}')"

ffmpeg -hide_banner -y -i "$VIDEO" \
  -vf "fps=${RATE},scale=480:-1,tile=6x8" \
  -frames:v 1 \
  research/evidence/videos/VIDEO_ID.contact-48.jpg
```

This is an overview, not proof that every frame was inspected. For long or information-dense videos, create denser samples.

Sample one frame per second and generate paginated 5×5 contact sheets:

```bash
mkdir -p research/evidence/videos/VIDEO_ID-1fps

ffmpeg -hide_banner -y -i "$VIDEO" \
  -vf "fps=1,scale=480:-1,tile=5x5:nb_frames=25" \
  -fps_mode vfr \
  research/evidence/videos/VIDEO_ID-1fps/sheet-%04d.jpg
```

If the FFmpeg build does not support an option, inspect `ffmpeg -h filter=tile` and `ffmpeg -h full`, then use the equivalent supported option. Verify multiple sheets are produced and the final partial sheet is not silently lost.

For very long videos, use 0.2 or 0.5 fps for the overview, then extract detailed frames around feature transitions.

## 5.7 Scene-change extraction

Extract likely scene changes to locate navigation between product screens, modal openings, source processing transitions, menus, artifact changes, different visual styles, error states, and before/after UI comparisons.

```bash
mkdir -p research/evidence/videos/VIDEO_ID-scenes

ffmpeg -hide_banner -y -i "$VIDEO" \
  -vf "select='gt(scene,0.30)',scale=1280:-1" \
  -fps_mode vfr \
  -q:v 3 \
  research/evidence/videos/VIDEO_ID-scenes/scene-%05d.jpg
```

`0.30` is a starting threshold, not a universal truth. Tune it if too many or too few frames are returned. Scene detection can miss meaningful UI changes with small pixel differences. Combine it with regular temporal sampling and manual inspection.

## 5.8 Extract a targeted time range

When a contact sheet identifies a relevant moment, extract a short interval at higher frame density:

```bash
ffmpeg -hide_banner -y \
  -ss 00:02:10 -to 00:02:30 \
  -i "$VIDEO" \
  -vf "fps=2,scale=1280:-1" \
  -q:v 2 \
  research/evidence/videos/VIDEO_ID-detail-%04d.jpg
```

For a single timestamp:

```bash
ffmpeg -hide_banner -y \
  -ss 00:02:14 \
  -i "$VIDEO" \
  -frames:v 1 \
  -q:v 2 \
  research/evidence/videos/VIDEO_ID-at-00-02-14.jpg
```

Check seeking accuracy, especially with long-GOP video. If exact frame accuracy matters, use frame-based verification.

## 5.9 Extract audio for listening/transcription

```bash
ffmpeg -hide_banner -y -i "$VIDEO" \
  -vn -ac 1 -ar 16000 \
  -c:a pcm_s16le \
  research/evidence/videos/VIDEO_ID-audio.wav
```

Then use original captions if available; listen to audio around feature demonstrations; if speech recognition is available and permitted, generate a transcript; label machine-generated transcripts as machine-generated; compare transcript against audio for important names/settings/technical terms; preserve timestamps.

Do not claim to have listened to a video merely because a transcript file exists. Do not claim a transcript is original if generated by speech recognition.

## 5.10 Visual inspection requirements

The agent must inspect actual contact sheet/frame images using its available visual understanding. Do not just create image files and declare the video analyzed.

For each relevant segment, record timestamp, screenshot/frame path, visible screen/panel, visible text, controls and selected states, action being performed, before/after state, narration summary, uncertainty, related official documentation, and Forge requirement.

Maintain `docs/reference/VIDEO_RESEARCH.md` with a table:

| Time | Image evidence | Screen | Visible action | Observed result | Confidence |
|---|---|---|---|---|---|
| 00:42 | `...jpg` | Source panel | User selects a PDF | Upload begins | VIDEO_OBSERVED |

A video demonstration can omit clicks, settings, delays, errors, and account limitations. A screenshot proves only what is visible in that frame. A contact sheet is a sampling method, not proof of every frame. Use direct browser observation to verify important workflows.

---

# 6. REA: use it deeply, but only where its capabilities are real

REA is a reverse-engineering toolkit connected to the coding agent. Current public REA materials describe work across native binaries, JavaScript/Electron applications, browser/runtime activity, application workflows and code paths, and network/browser evidence and screenshots depending on the installed version and provider capabilities.

Official starting points:
- REA site: https://rea.tools/
- REA guides: https://rea.tools/guides/
- REA GitHub: https://github.com/Eng-RSMY/REA
- REA setup example: `npx rea-agents@latest setup`

## 6.1 Initial REA capability audit

Before using it:
1. locate installed REA package and version;
2. inspect its own `--help`;
3. run `rea doctor --json` if that command exists in this version;
4. read bundled/current REA docs and skill;
5. identify available providers and missing engines;
6. inspect whether the agent is configured for CLI, MCP, or both;
7. inspect available browser/runtime targets;
8. produce `docs/reference/REA_CAPABILITY_REPORT.md`.

Do not assume a tool is installed merely because the skill or package is mentioned in a prompt. Verify it. Use exact syntax documented by the installed version; commands may change between releases.

## 6.2 REA browser investigation

Use REA for browser observation when a supported browser is running and the user has authorized access.

Preferred safe approach:
- a real Chrome/Chromium browser;
- a persistent profile controlled by the authorized user;
- local loopback CDP endpoint;
- exact approved origin list;
- passive capture or approved observation workflow;
- redaction of sensitive values;
- no cookie/token export;
- no hidden authentication bypass.

The official REA materials and public guides describe browser observation and application workflow tracing. Verify the current version's exact feature names and consent configuration.

If a version uses environment variables for approved browser observation, inspect current REA documentation rather than copying stale values from an unrelated fork or older release. Constrain observation to exact origins, e.g. current notebook origin and required official help origin, not unrestricted browsing.

Potential origins to investigate only when necessary:
- `https://notebook.google.com`
- `https://notebooklm.google.com` (legacy)
- `https://support.google.com`
- `https://gemini.google.com`
- `https://drive.google.com`

Do not add all origins automatically. Approve only origins required for a specific task.

REA browser inspection should help answer which application route is active; which scripts/modules are loaded; what UI structure is visible; which controls exist; what observable request metadata is associated with a user action; what response shape/application state appears; what changed between runs; and which evidence maps a visible action to an implementation path.

Do not capture or print raw session cookies, bearer tokens, passwords, authorization headers, private messages, or unrelated browsing activity.

## 6.3 REA static JavaScript/application analysis

When public application bundles or legally accessible scripts are available:
- identify entrypoints and chunks;
- enumerate modules;
- inspect source maps only when accessible;
- map route/component relationships;
- identify feature flags and lazy-loaded feature bundles;
- trace event handlers and state transitions;
- identify network call sites and response consumers;
- map relevant data models;
- search error strings and capability flags;
- compare code versions where available.

Do not call obfuscated/minified implementation details “understood” merely because strings were found. Trace actual references and execution paths where possible.

## 6.4 REA workflow tracing

For each feature:
1. identify the user action;
2. capture visible before-state;
3. perform authorized action in the browser;
4. capture resulting state;
5. use REA to inspect relevant browser/runtime evidence;
6. map feature to code/request path where possible;
7. compare multiple runs;
8. document known, inferred, and unknown points;
9. implement Forge's independent equivalent;
10. test Forge against the same workflow.

Use REA as much as it helps. If no browser, runtime, native-analysis engine, or provider is available, record that limitation and continue with other evidence. Do not spend the whole project trying to install an unavailable engine.

## 6.5 Protocol research isolation

The existing project reportedly contains a NotebookLM protocol experiment in `crates/forge-core/src/notebooklm.rs`, based on public reverse-engineering research. Audit current code and documents.

Requirements:
- keep it isolated and clearly labelled experimental;
- identify public evidence and exact commit/version provenance;
- distinguish request serialization tests from actual live-service verification;
- check whether sources are genuinely independent;
- check authentication and request prerequisites;
- never extract or store user's cookies;
- never bypass Google authentication;
- never claim a working integration if no authenticated request and response were tested;
- do not make Forge's normal source/chat/artifact workflows dependent on private/undocumented endpoints;
- do not run unapproved requests against Google services.

The protocol client must remain disabled unless a legitimate, authorized, stable use is established. Official APIs and Forge-owned implementations take precedence.

---
# 7. Forge functional requirements

The following is the target product inventory. Investigate, implement, test, and maintain its status. Some items are foundational; some may be staged due to dependencies and resource limits.

Priority:
- **P0:** essential for the application to be useful and trustworthy.
- **P1:** core NotebookLM-class functionality.
- **P2:** advanced experience / differentiation.
- **P3:** optional or high-cost expansion.

## 7.1 Workspace and notebooks — P0

- persistent notebook creation;
- rename;
- delete with confirmation;
- open/close;
- recent notebook list;
- search notebooks;
- sort;
- notebook icon/emoji or original visual identifier;
- notebook metadata;
- empty state;
- source count;
- last updated;
- persistent selection;
- export notebook;
- backup and restore;
- duplicate notebook if safe and useful;
- local project organization;
- notebook deletion that correctly removes or retains related records according to documented policy.

## 7.2 Source ingestion — P0

Implement real import paths for:
- pasted text;
- Markdown;
- TXT;
- PDF;
- DOCX;
- PPTX;
- XLSX;
- CSV/TSV;
- HTML;
- public web URL;
- image files;
- audio files;
- public YouTube URL with permitted transcript retrieval.

Then expand:
- Google Drive/Docs/Slides/Sheets;
- ePub;
- GitHub repositories/issues/releases/README;
- additional web connectors;
- RSS feeds;
- authorized video files;
- additional audio formats;
- other source types discovered in research.

For each parser provide format validation, metadata extraction, text extraction, tables and structured data, page/slide/sheet/cell locators, image/figure handling, OCR if needed, error reporting, size/resource caps, test fixtures, and real-world samples where rights permit.

Do not report a source as indexed until extraction and indexing completed. Store an import job state and diagnostics.

## 7.3 Source processing and canonical representation — P0

Create a common source representation with stable source ID, notebook ID, original name/title, original URL or local origin, MIME type, content hash, file size, import time, source date if known, refresh/update state, parser and parser version, raw extraction metadata, normalized text, text blocks/chunks, image references, tables, structure hierarchy, locator map, extraction warnings, language, and indexing state.

Maintain links from every normalized block back to the original source location. Avoid flattening every format into one text blob and losing precise citations.

## 7.4 Search, indexing, retrieval — P0

Implement lexical search, full-text search, chunk-level retrieval, semantic/vector retrieval when an embedding provider is available, hybrid scoring, query rewriting, source filters, active-source filtering, metadata filters, page/title/section weighting, reranking, duplicate chunk suppression, diversity across sources, retrieval explanations for debugging, index rebuild, incremental updates, and cache invalidation.

Do not require a local LLM or local embedding model for basic operation. Keep provider adapters modular. If semantic search is unavailable, use the available lexical strategy honestly.

## 7.5 Grounded chat and citations — P0

- persistent conversation;
- streaming answer;
- stop;
- retry;
- follow-up prompts;
- configurable source scope;
- configurable style/length;
- citations to exact evidence;
- quotation preview;
- exact source location opening;
- citations for tables and images where supported;
- unsupported answer abstention;
- contradiction warnings;
- answer copy;
- save response to note;
- conversation history management;
- user feedback;
- model/provider status;
- model failures and rate limits;
- token/usage accounting if available.

A citation must never be generated only because a source was retrieved. It must support the associated claim.

## 7.6 Notes, annotations, noteboard — P1

- rich-text / Markdown notes;
- autosave;
- save chat answer as note;
- edit;
- rename;
- pin;
- labels;
- search;
- source links;
- highlighted quotes;
- annotations;
- convert note to source;
- include/exclude note from chat;
- include notes in artifacts;
- export;
- delete;
- persistence across reloads.

## 7.7 Study and learning tools — P1

- flashcards;
- quiz generator;
- practice mode;
- difficulty;
- prompt customization;
- question count;
- explanation;
- cite supporting evidence;
- score/history;
- shuffle;
- revisit missed cards/questions;
- export;
- regenerate.

Add spaced repetition, study sessions, progress analytics, learning objectives, glossary, key terms, concept explanations, and compare/contrast exercises. These additional learning tools must be source-grounded, not generic filler.

## 7.8 Knowledge visualization — P1

- mind maps;
- hierarchical concept graph;
- node expansion/collapse;
- pan/zoom;
- node-to-chat question action;
- node-to-source evidence link;
- concept relationship labels;
- citations on nodes/edges where possible;
- download/export;
- large-map performance;
- keyboard accessibility.

Later: timelines; entity/relation graph; source-to-claim map; contradiction map; data visualizations; knowledge graph exploration.

## 7.9 Documents and reports — P1

- summaries;
- study guides;
- briefings;
- FAQ;
- research reports;
- comparisons;
- structured extraction;
- timelines;
- learning overviews;
- interactive reports;
- user templates;
- export Markdown/HTML/PDF/DOCX;
- source citations in output;
- artifact history;
- regenerate;
- revise;
- download;
- share.

## 7.10 Visual artifacts — P1/P2

- infographics;
- slide decks;
- Detailed Deck;
- Presenter Slides;
- outline preview;
- review/edit outline;
- per-slide revision;
- reorder;
- delete/restore slides;
- PPTX/PDF export;
- fullscreen slideshow;
- image generation where an actual authorized image model is configured;
- text legibility validation;
- source-to-slide traceability.

Do not fake image generation with a static placeholder or claim an artifact is AI-generated if it is actually a template.

## 7.11 Audio — P1/P2

- audio overview script;
- multiple-speaker dialogue format;
- text-to-speech generation through a real provider;
- configurable language;
- configurable style and length;
- audio player;
- background job;
- generation history;
- download;
- transcript;
- citation/source traceability for each segment where feasible.

Audio generation requires a real speech provider or installed model. If unavailable, provide a script and label audio generation unsupported. Do not return an empty or silent file as a successful podcast.

## 7.12 Video understanding and video generation — P1/P2

Two separate capabilities.

### Video as a source
- public URL metadata;
- caption/transcript ingestion;
- local authorized video ingestion;
- FFprobe metadata;
- audio extraction;
- ASR where available;
- scene-change sampling;
- keyframe extraction;
- timestamped visual captions/observations;
- visual embeddings where available;
- video player / timestamp jump;
- citations to time ranges;
- transcript and visual evidence kept distinct.

### Video as an output
- storyboard;
- producer notes;
- narration script;
- visual scene plan;
- generated narration audio;
- generated/selected visuals;
- captions;
- final rendering via FFmpeg;
- export;
- job progress;
- asset provenance;
- source validation.

Do not describe a narration script as a finished video. Do not describe transcript ingestion as full visual understanding.

## 7.13 Deep Research and web discovery — P1/P2

- search query planning;
- multiple sub-queries;
- search provider adapters;
- URL retrieval;
- readable page extraction;
- canonical URLs;
- publication date and author where available;
- content date vs crawl date;
- duplicate detection;
- source quality indicators;
- claim/evidence table;
- citation graph;
- contradictions;
- import selected results;
- research report;
- cancellation;
- progress display;
- limits/cost controls;
- source review.

Research must not silently trust generated summaries without checking original sources.

## 7.14 Integrations and connected apps — P2

Design a connector registry for Google Drive, Google Docs/Slides/Sheets, GitHub, other storage providers, calendar/task providers if relevant, approved external APIs, and user-configured connectors.

Each connector requires OAuth/API key handling or an explicitly scoped authorization flow. Never request secrets in a chat message or commit them to the repository.

MCP may be used as an optional tool interface if it materially helps; it is not the product architecture and not a required dependency. A connector can also be implemented through a normal Rust interface or HTTP client.

## 7.15 Collaboration and sharing — P2

- user identity;
- owner/editor/viewer;
- notebook invitation;
- share link;
- revoke;
- public/private;
- artifact share;
- source access enforcement;
- collaborator activity;
- audit trail;
- conflict handling;
- concurrent edit strategy;
- export shared workspace.

Implement server-side authorization before public sharing.

## 7.16 Advanced research quality — P2/P3

- contradiction detection;
- claim extraction;
- evidence graph;
- source reliability annotations;
- temporal reasoning;
- cross-source comparison;
- missing-information detection;
- structured dataset extraction;
- formula-aware spreadsheet analysis;
- chart extraction;
- visual document understanding;
- multi-modal retrieval;
- citation quality scoring;
- answer faithfulness evaluation;
- research reproducibility;
- source freshness alerts.

## 7.17 Settings and user control — P0/P1

- provider configuration;
- model selection;
- BYO API keys;
- safe key storage;
- default language;
- chat style;
- response length;
- theme;
- appearance;
- storage;
- export/backup;
- privacy;
- usage display;
- data deletion;
- source-processing preferences;
- connector permissions;
- keyboard shortcuts;
- accessibility options.

---

# 8. Multimedia understanding architecture

## 8.1 Do not confuse transcript with video

A video contains speech, non-speech audio, text visible on screen, UI layout, diagrams and images, transitions, animations, cursor movement, user interaction, and timing/sequence.

A transcript contains only spoken/written text that has been transcribed. It does not explain all visual content.

Forge's evidence model must distinguish:
- `TRANSCRIPT_SPAN`;
- `VIDEO_FRAME`;
- `SCENE`;
- `AUDIO_EVENT`;
- `OCR_REGION`;
- `VISUAL_CAPTION`;
- `USER_OBSERVATION`.

All video evidence must carry timecodes.

## 8.2 Audio understanding

Processing stages:
1. ingest authorized audio/video;
2. validate codec and duration;
3. extract audio with FFmpeg;
4. normalize channels/sample rate when required;
5. detect speech/silence;
6. transcribe with timestamps;
7. detect language;
8. optionally diarize speakers;
9. preserve source transcript separately from machine transcript;
10. map transcript segments to timecodes;
11. index segments;
12. expose timecode citations.

## 8.3 Visual understanding

Processing stages:
1. probe stream metadata;
2. sample frames at adaptive intervals;
3. detect scene changes;
4. extract keyframes;
5. optionally detect slide boundaries;
6. run OCR if required and available;
7. run image understanding with a configured vision provider if available;
8. record visual descriptions and confidence;
9. connect observations to the timeline;
10. index timestamped visual evidence.

OCR alone is not visual understanding. A frame caption alone is not a verified UI action. Keep evidence provenance.

## 8.4 Document images and diagrams

For PDFs and slides: detect embedded images; extract them with location metadata; render pages when layout matters; OCR scanned pages; inspect charts and diagrams; preserve relationships between OCR text and page coordinates; cite page and region; store generated previews separately from originals.

## 8.5 Image and video model providers

Implement optional provider interfaces for image understanding, image OCR, speech recognition, speaker diarization, embeddings, text-to-speech, image generation, and video generation.

Do not hard-code one vendor into the ingestion pipeline. Each provider should declare capabilities, limits, costs if known, supported MIME types, privacy implications, and failure modes.

---

# 9. Technical architecture — Rust-first and modular

## 9.1 Architecture rule

Inspect the existing architecture first. Preserve working code unless there is strong evidence for a change. Do not rewrite the whole application because a new architecture looks cleaner.

Keep the product Rust-first. Use non-Rust tools only when a mature, safe library or external program is the pragmatic option.

Conceptual module map:

```text
Forge
├── domain
│   ├── notebook
│   ├── source
│   ├── conversation
│   ├── evidence
│   ├── note
│   ├── artifact
│   ├── permissions
│   └── jobs
├── ingestion
│   ├── file validators
│   ├── parsers
│   ├── URL fetcher
│   ├── YouTube importer
│   ├── audio/video pipeline
│   ├── OCR adapters
│   └── normalization
├── indexing
│   ├── chunking
│   ├── lexical index
│   ├── embeddings
│   ├── vector index
│   ├── reranking
│   └── retrieval
├── answer
│   ├── provider interface
│   ├── prompt builder
│   ├── evidence selector
│   ├── citation validator
│   └── abstention logic
├── research
│   ├── search providers
│   ├── page fetchers
│   ├── research planner
│   ├── source evaluator
│   └── report generator
├── artifacts
│   ├── summaries
│   ├── reports
│   ├── flashcards
│   ├── quizzes
│   ├── mind maps
│   ├── slide decks
│   ├── infographics
│   ├── audio
│   └── video
├── integrations
│   ├── provider registry
│   ├── Google Drive
│   ├── GitHub
│   └── optional connectors
├── persistence
├── security
├── server
└── frontend
```

This is a conceptual map, not an instruction to create one crate per bullet. Choose boundaries based on actual coupling and compile/test speed.

## 9.2 Provider architecture

Define traits/interfaces for chat completion, structured generation, embeddings, reranking, image understanding, OCR, transcription, speech synthesis, image generation, search, URL extraction, and optional code execution.

The core application must not depend on one model vendor. Possible adapters can include configured Gemini, OpenAI-compatible, OpenRouter, or other providers, but verify current APIs and SDKs before implementation. Do not invent model names or endpoints.

BYO-key requirements:
- secrets stored outside source control;
- restrictive filesystem permissions or platform secret store;
- never return full keys in API responses;
- never log secrets;
- never write secrets to browser local storage;
- explicit delete/rotate;
- clear error for missing credentials;
- no surprise paid usage.

Cloud-first is acceptable. A local LLM is not required. Local parsers and retrieval should still work without a model provider.

## 9.3 Persistence

Inspect the existing data layer. Use a durable database and migrations. SQLite is acceptable for local/single-user deployment if concurrency and backup behaviour are properly handled; do not use in-memory state for persistent user content.

Persistent entities should include users or local profiles, notebooks, notebook settings, source records, source versions, raw file metadata, extracted blocks, chunks, index state, citation/evidence records, conversations, messages, notes, artifacts, artifact versions, background jobs, connector configurations, sharing grants, feedback, and audit events.

Use stable IDs. Use migrations. Test backup/restore. Define deletion and retention semantics.

## 9.4 Background jobs

Long-running tasks must not block HTTP request workers:
- source ingestion;
- OCR;
- ASR;
- embedding;
- web research;
- artifact generation;
- audio/video rendering;
- exports;
- sync.

Every job needs ID, state, created/started/finished timestamps, progress, cancellation, retry policy, idempotency, failure reason, resource limit, and result association.

UI must distinguish queued, running, partially complete, succeeded, failed, and canceled.

## 9.5 Frontend

Keep the UI responsive and usable. Research the reference layout but use original Forge design.

Core application areas: notebook library; notebook workspace; source panel; source preview; chat panel; notes; Studio/artifacts; artifact viewers/editors; settings; source discovery; job/status indicators.

Provide desktop/tablet/mobile layouts, loading skeletons, empty states, clear error messages, keyboard navigation, focus visibility, accessible labels, reduced motion, coherent theme, no inert buttons, and no placeholder controls presented as working.

## 9.6 Resource constraints

The development sandbox and user's laptop may be resource-constrained. Verify actual CPU, RAM, disk, and available system packages each time the environment changes.

Prioritize no mandatory local LLM, streaming, incremental parsing, bounded concurrency, file-size limits, memory-aware page rendering, temporary-file cleanup, optional heavy pipelines, small efficient UI bundles, no unnecessary Docker dependency, and no heavyweight local database service if embedded storage suffices.

Never solve an environment constraint by silently removing a feature. Record the actual limitation and provide a fallback.

---

# 10. Source parsing and locator contract

## 10.1 Canonical evidence locators

Each extracted evidence block should preserve the best possible original location.

Examples:
- PDF: page number, block index, optional bounding box;
- DOCX: heading path, paragraph index, table row/cell;
- PPTX: slide number, shape ID, shape text, image location;
- XLSX: sheet name, cell range, formula/value;
- CSV: row and column;
- HTML: canonical URL, heading, DOM selector or stable text offset;
- YouTube: video ID, start/end time, transcript segment;
- audio: time range and speaker if known;
- video visual: frame timestamp and image hash;
- image: image hash, OCR region/bounding box;
- Markdown: heading path and line range;
- code/GitHub: repository, commit hash, path, line range.

Never substitute a generic “source 1” locator when a precise locator can be preserved.

## 10.2 Chunking

Chunk along document structure: headings, paragraphs, tables, slides, pages, transcript segments, and visual scenes.

Avoid splitting sentences and table rows unnecessarily. Store parent-child relationships and overlapping context where useful.

Every chunk should have stable chunk ID, source ID/version, normalized text, original text span, locator, token estimate, language, and index metadata.

## 10.3 Source updates

When a source is updated: compare content hash; determine changed blocks; reprocess changed content; preserve stable links where feasible; mark old citation versions if they no longer resolve; update retrieval index; mark downstream artifacts as potentially stale; do not silently mutate old evidence snapshots.

## 10.4 Deduplication

Detect exact duplicate files, same URL variants, duplicate text, same YouTube video ID, near-identical documents, and overlapping imported pages.

Do not destroy source records during deduplication. Preserve provenance and let the user understand which sources were merged or treated as duplicates.

---
# 11. Security, privacy, and abuse resistance

Security is a release blocker, not a polish task.

## 11.1 Untrusted inputs

Treat all imported material as untrusted: PDFs; Office documents; archives; images; audio/video; web pages; transcripts; source code; extracted metadata; and model outputs.

Never execute uploaded files. Never execute macros. Never run shell commands derived from source text. Never allow source prompt injection to override application instructions, access controls, or user settings.

## 11.2 File/parser safety

Implement:
- upload size limits;
- decompressed size limits;
- archive entry count limits;
- nested archive depth limits;
- parser timeouts;
- memory limits;
- path traversal prevention;
- MIME sniffing;
- extension validation;
- malformed-file handling;
- integer overflow checks;
- out-of-bounds protection;
- temp-directory isolation;
- no arbitrary file write;
- no automatic execution.

Do not trust user-supplied filenames. Generate internal storage names.

## 11.3 URL and SSRF safety

For URL ingestion:
- allow only supported schemes;
- block localhost/private/link-local/multicast/reserved IP ranges;
- validate DNS and revalidate after redirects;
- prevent DNS rebinding;
- cap redirects;
- cap response size;
- set timeouts;
- restrict ports where appropriate;
- reject `file://`, `data://`, and dangerous custom schemes;
- reject credentials embedded in URLs;
- prevent access to cloud metadata endpoints;
- validate every redirect target;
- do not forward private cookies or credentials.

## 11.4 Browser and connector security

- exact origin allowlists;
- explicit permission scopes;
- secret redaction;
- no session cookie logging;
- no arbitrary page execution;
- no broad browser activity capture;
- no silent connected-app actions;
- no credentials in evidence;
- no secrets in source control;
- safe OAuth state handling.

## 11.5 Web application security

Audit authentication, authorization, CSRF, XSS, SQL injection, path traversal, SSRF, content security policy, file upload endpoints, rate limiting, access checks for every source and artifact, secure session handling, API schema validation, error sanitization, secret handling, and dependency vulnerabilities.

## 11.6 Model prompt injection and data leakage

Retrieved sources may contain malicious instructions. They are data, not instructions.

- keep system/developer instructions separate from retrieved content;
- quote and delimit source material;
- never give retrieved content authority to call tools;
- require explicit approval for external actions;
- do not transmit all notebook content to every provider by default;
- disclose provider data sharing;
- support data minimization;
- provide deletion/export controls.

## 11.7 Privacy

Document what data is stored, where it is stored, which provider receives it, retention, deletion, backups, logs, telemetry, model training policy of configured providers (as documented, not assumed), and whether user content is sent outside the device.

Do not make absolute privacy promises that depend on third-party provider terms.

---

# 12. Testing and verification programme

## 12.1 Test layers

### Unit tests
- parsers;
- chunking;
- locator mapping;
- source normalization;
- retrieval;
- citation validation;
- serialization;
- permissions;
- job state machine;
- provider adapters;
- API schema.

### Integration tests
- upload → parse → index;
- select source → retrieve → answer;
- answer → citation preview;
- save response → note;
- note used as source;
- source update and re-index;
- artifact generation;
- export;
- backup and restore;
- background job cancellation;
- permission enforcement.

### End-to-end UI tests
- create notebook;
- upload source;
- view source;
- ask question;
- open citation;
- save note;
- create artifact;
- edit/revise;
- export;
- delete source;
- restart server and reopen notebook.

### Security tests
- malformed files;
- zip bombs;
- path traversal;
- SSRF;
- redirects;
- huge uploads;
- decompression bombs;
- malicious HTML;
- prompt-injection sources;
- XSS;
- unauthorized notebook access;
- revoked share link;
- leaked credentials;
- concurrent requests.

### Regression tests

Maintain known fixtures and expected evidence locators. Do not weaken tests to make a new parser pass.

## 12.2 Grounding evaluation corpus

Create a gold corpus with known answers and evidence:
- questions answerable from one source;
- questions requiring multiple sources;
- questions with contradictory sources;
- questions absent from all sources;
- ambiguous questions;
- questions requiring exact numbers;
- questions requiring date interpretation;
- tables and spreadsheet data;
- image/diagram content;
- transcript and timecode evidence;
- prompt injection inside sources;
- multilingual content.

Metrics:
- answer correctness;
- citation precision;
- citation recall;
- evidence entailment;
- unsupported-claim rate;
- abstention correctness;
- source coverage;
- locator accuracy.

A fluent answer with the wrong citation is a failure.

## 12.3 Test evidence

For every test run, record command, environment, commit hash, exit code, summary, relevant output, failures, and skipped tests with reason.

Do not write “all tests pass” when tests were skipped, offline, or not run. Distinguish locally executed, CI executed, agent-reported, and not executed.

## 12.4 Real-world files

Use representative legal/public/open test files. Record origin, hash, licence/permission basis, size, file type, and expected behaviour.

Self-created fixtures are necessary but insufficient. Include real-world samples after sanitization and permission review.

Do not upload user-provided APKs/EXEs to external services. Treat binary analysis as a separate static-analysis feature and never execute untrusted binaries.

---

# 13. Feature implementation roadmap

Research and implementation should overlap. Do not let research become an endless phase, but do not build large features from guesses.

## Phase 0 — repository recovery and baseline

Deliver actual repository state report, environment report, existing feature inventory, test baseline, known architecture, current limitations, and preserved work/backup strategy.

**Gate:** no changes until actual project state is understood.

## Phase 1 — reference intelligence

Deliver official feature inventory, live UI investigation if available, video analysis corpus and timestamped notes, REA capability report, evidence catalogue, feature parity matrix, and explicit unknowns/blockers.

**Gate:** all major feature families must be represented. Unknowns are acceptable if explicitly recorded.

## Phase 2 — core product integrity

Verify/improve persistence, migrations, notebook lifecycle, ingestion job model, source model, source viewer, parser reliability, exact locators, index/retrieval, citation system, provider abstraction, and error handling.

**Gate:** user can create a notebook, import supported material, restart, reopen, ask a question, and verify the answer against exact evidence.

## Phase 3 — source coverage and source UX

Expand formats, URL ingestion, public YouTube transcript, audio transcript, source guide, labeling, source filters, source refresh, duplicate management, and visual document extraction.

**Gate:** all source claims in the UI match actual parsers and tests.

## Phase 4 — chat and research quality

Implement source selection, custom chat, streaming/cancel, follow-ups, evidence validator, abstention, contradictions, Fast Research, and Deep Research workflow.

**Gate:** grounding evaluation meets explicit quality thresholds; no fabricated citations.

## Phase 5 — Studio and learning

Implement notes, flashcards, quizzes, mind maps, reports, summaries, study guides, FAQ, and structured tables.

**Gate:** each artifact is persisted, re-openable, exportable, and traceable to source evidence.

## Phase 6 — visual, audio, and slide artifacts

Implement infographics, slide decks, outline/revision workflow, audio overviews, narration, and video storyboard/rendering.

**Gate:** each is a real output generated by a real pipeline/provider. Unsupported providers must be reported clearly.

## Phase 7 — research and integrations

Implement web source discovery, research planner, connected apps, Drive/GitHub adapters, source sync, citations, and provenance.

**Gate:** safe authorization, permissions, failure handling, and provider documentation.

## Phase 8 — sharing, device polish, release readiness

Implement roles and access control, public/private sharing if suitable, artifact sharing, responsive tablet/mobile UX, accessibility, backup/restore, privacy and security audit, deployment/install docs, and release tests.

**Gate:** release checklist passes and limitations are published.

## Phase 9 — beyond parity

After reference parity work, prioritize differentiated strengths:
- stronger source-to-claim traceability;
- more reliable multimodal evidence;
- reproducible Deep Research;
- cross-notebook knowledge;
- better source versioning;
- research lineage;
- claim conflict explorer;
- advanced learning analytics;
- portable open formats;
- user-controlled local/cloud workflows.

---

# 14. Acceptance tests: user-visible journeys

Implement exact repeatable tests for these journeys.

## Journey A — first notebook
1. Open Forge.
2. Create a notebook.
3. Rename it.
4. Close and reopen it.
5. Confirm persistence.
6. Confirm no hidden demo data is shown as user data.

## Journey B — PDF evidence
1. Upload a multi-page PDF.
2. Wait for actual parse completion.
3. Inspect extracted pages.
4. Ask a question whose answer is on page 2.
5. Confirm response citation points to page 2.
6. Click citation.
7. Confirm exact passage is highlighted or visible.
8. Ask an unsupported question.
9. Confirm the system abstains rather than inventing an answer.

## Journey C — multi-source reasoning
1. Import two sources with overlapping claims.
2. Ask a question needing both sources.
3. Confirm both sources are cited.
4. Import a source contradicting one claim.
5. Ask for a comparison.
6. Confirm the contradiction is surfaced, not silently reconciled.

## Journey D — note as a source
1. Create a note.
2. Save a chat response as a note.
3. Edit the note.
4. Include it in chat scope.
5. Ask a question requiring note content.
6. Confirm citation to the note.
7. Exclude it and confirm it no longer influences the answer.

## Journey E — YouTube/video source
1. Add a public captioned video URL.
2. Import available transcript with source/time metadata.
3. Ask a question about a statement at a known time.
4. Confirm a timecode citation.
5. If visual analysis is available, ask about an on-screen diagram.
6. Verify answer cites visual-frame evidence, not only transcript.
7. If visual analysis is unavailable, state that limitation explicitly.

## Journey F — flashcards and quiz
1. Generate flashcards with a chosen difficulty.
2. Review and flip cards.
3. Record known/unknown.
4. Generate a quiz.
5. Answer correctly and incorrectly.
6. Verify explanations against citations.
7. Reopen the artifact after restart.

## Journey G — mind map
1. Generate mind map.
2. Expand/collapse nodes.
3. Select a node.
4. Ask a question about it.
5. Open its source evidence.
6. Export it.
7. Reopen it after restart.

## Journey H — slide deck
1. Select sources.
2. Set audience and output.
3. Generate outline.
4. Review and revise.
5. Generate deck.
6. Edit one slide.
7. Reorder and delete/restore a slide.
8. Export PPTX/PDF.
9. Confirm persisted artifact version.

## Journey I — research discovery
1. Submit a research question.
2. Search multiple independent domains.
3. Review results.
4. Open original sources.
5. Select imports.
6. Generate a report.
7. Confirm references are real and correspond to claims.
8. Export the report.

## Journey J — access control
1. Create a private notebook.
2. Share with viewer.
3. Confirm viewer cannot edit.
4. Share with editor.
5. Confirm editor can perform only permitted actions.
6. Revoke access.
7. Confirm revoked user cannot access source, note, chat, or artifact endpoints.

## Journey K — resilience
1. Start an ingestion job.
2. Stop/restart server during processing.
3. Verify recovery or clear failure.
4. Retry safely.
5. Confirm duplicate records are not created.
6. Verify database consistency.
7. Verify job and UI states.

---

# 15. UI/UX direction

Create an original visual design. Do not copy Google's protected assets or exact visual identity.

Target experience:
- calm, focused research environment;
- clear notebook navigation;
- compact, readable source panel;
- high-quality document viewer;
- readable chat with strong citation affordances;
- Studio artifact shelf;
- obvious selected-source context;
- visible processing status;
- attractive but restrained visualizations;
- responsive tablet support;
- fast initial load.

Design principles:
- functional hierarchy over decoration;
- avoid crowded sidebars;
- prevent important actions from being hidden in menus;
- distinguish source facts from AI-generated outputs;
- citations must be visually obvious;
- errors must be understandable;
- avoid excessive animation;
- keep the interface accessible.

Perform screenshot comparisons of layout and workflows against authorized reference screenshots, but do not copy logos, proprietary illustrations, or protected product artwork.

---

# 16. Documentation and release discipline

Every implementation change must update appropriate documents.

`docs/STATUS.md`:
- current commit;
- branch;
- running service;
- features genuinely implemented;
- known blockers;
- tests run;
- next task.

`docs/FEATURE_PARITY.md`:
- one row per reference feature;
- reference evidence;
- Forge status;
- test evidence;
- gaps.

`docs/UNKNOWN_AND_LIMITATIONS.md`:
- unsupported features;
- missing dependencies;
- blocked research;
- inferred behaviours;
- unverified APIs;
- known quality limitations.

`docs/SECURITY.md`:
- threat model;
- upload limits;
- parser sandbox;
- SSRF controls;
- secret handling;
- authorization;
- prompt-injection controls;
- known residual risks.

`docs/TEST_MATRIX.md`:
- feature;
- unit tests;
- integration tests;
- UI tests;
- security tests;
- real-world fixtures;
- result.

`docs/DECISIONS.md`:
- decision;
- date;
- alternatives;
- reason;
- consequences;
- revisit condition.

## 16.1 Git discipline

- Work on the expected branch.
- Commit in coherent, reviewable increments.
- Never claim a push succeeded unless the push command succeeded and remote state was verified.
- Never force-push without explicit user approval.
- Preserve working changes.
- Record commit hashes in status.
- Run tests before commit.
- Inspect `git diff --check`.
- Ensure generated files and secrets are not accidentally committed.

## 16.2 Honest capability page

The UI capability page must distinguish working and tested; working but partially tested; provider required; dependency required; unsupported; experimental; and blocked by environment.

Do not use an attractive feature card to disguise a stub.

---

# 17. Arena execution instructions: how to proceed now

Perform these steps in order.

## Step 1 — establish reality

Inspect the current repository, branch, commit, working tree, remotes, filesystem, toolchain, and test baseline. Do not assume the sandbox persists from a previous run.

## Step 2 — audit existing Forge before rebuilding

Read existing status and decisions, user requirements, current feature parity, architecture, source parser implementations, retrieval and answer flow, citation implementation, frontend, HTTP endpoints, tests, NotebookLM protocol experiment, binary inspection code, and test claims.

Identify actual working features and missing features. Preserve current working behaviour.

## Step 3 — discover available research tools

Inspect available web search, browser automation, REA, GitHub, terminal, yt-dlp, FFmpeg/FFprobe, screenshot/visual inspection, OCR, speech recognition, and image understanding.

Check versions and actual availability. Do not claim an unavailable tool is installed. If a task-specific skill library exists, search it for relevant research, browser, testing, Rust, parsing, UI, accessibility, and security skills. Use skills as guidance; do not let them replace verification.

## Step 4 — build the evidence map

Research all sections in §2. Record evidence and unknowns in the required files. Use official Google materials first. Use browser and REA when available. Use authorized YouTube videos with yt-dlp and FFmpeg visual inspection when useful.

## Step 5 — create the feature parity matrix

Every major feature family must have evidence, user workflow, Forge requirement, implementation location, acceptance test, status, and blocker.

## Step 6 — choose next implementation task from highest value gap

Prefer the smallest complete, testable vertical slice. Do not add many disconnected buttons. A complete source path is more valuable than ten mock features.

## Step 7 — implement and test

For each slice: inspect existing code; state intended changes in a short work note; implement; run unit/integration/UI tests; add regression tests; update docs; review security; commit; verify remote push if available.

## Step 8 — repeat toward parity

Continue across the roadmap. Do not stop after research documents are complete. Do not stop after the next commit. Keep building until blocked by a real constraint or the work is complete.

## Step 9 — report truthfully

At each checkpoint report what was researched; what was directly observed; what was only documented; what was inferred; what was implemented; what test commands ran; exact pass/fail counts; commit and push status; remaining blockers; and next best task.

No fabricated screenshots, tests, citations, browser sessions, file paths, or success claims.

---

# 18. Definition of done for the overall product

Forge is not “complete” merely because it resembles Gemini Notebook visually.

The first serious release is acceptable when:
- users can create, organize, and reopen persistent notebooks;
- core source formats import reliably;
- source extraction preserves precise locators;
- the source viewer works;
- notebook-scoped chat is grounded;
- citations open exact evidence;
- unsupported questions trigger honest abstention;
- notes work as persistent sources;
- core study artifacts work and persist;
- users can export their work;
- security controls are in place;
- the UI works on desktop and tablet;
- optional providers fail gracefully;
- documentation accurately lists gaps;
- tests cover critical journeys.

A broader parity release must also include applicable research, multimedia, Studio, sharing, and integration features discovered in the current reference inventory.

The long-term product should exceed the reference in citation reliability, reproducibility, source provenance, multimodal evidence, exportability, user control, privacy transparency, cross-source reasoning, and extensibility.

---

# 19. Current baseline research notes (30 October 2026)

The following is a starting map based on official sources checked while preparing this document. It is not a replacement for current in-product verification.

1. Google announced the NotebookLM → Gemini Notebook rename on 16 July 2026.
2. The official Help Center currently lists topics for chat, source discovery, notes, mind maps, Audio Overviews, Video Overviews, flashcards/quizzes, infographics, slide decks, reports, public notebooks, mobile app, connected apps, plans, and usage limits.
3. Official documentation describes source types including common documents, Google Workspace files, images, ePub, audio, web URLs, public YouTube URLs, and Gemini chats. It also records source limits and source-specific caveats; verify current visible values.
4. Google documents source selection in chat and citations that can show quoted text and navigate to the passage.
5. Google documents Default, Learning Guide, and Custom chat styles, with response-length settings; verify current availability.
6. Google documents Fast Research from Web or Drive and a separate Deep Research workflow that reviews and imports results.
7. Google documents notes as sources that can participate in Q&A and artifact generation.
8. Google documents background generation for at least some Studio artifacts.
9. Audio Overviews and Video Overviews have separate workflows and customization controls.
10. Mind Maps support interaction such as zoom/pan, expand/collapse, and asking about a selected node.
11. Slide Decks have format/length/language/prompt controls and revision workflows; official docs describe Detailed Deck and Presenter Slides.
12. Infographics have language, detail, orientation, style, and prompt controls.
13. Official help describes connected apps and Gemini chat threads as notebook context, with differences in source grounding between Gemini Notebook and Gemini Apps.
14. Plan limits, age restrictions, mobile limitations, and feature rollouts vary. Never assume all accounts see every control.

Treat this list as a baseline for what to investigate, not as an exhaustive final inventory.

---

# 20. Primary research references

Use these links as first-stop sources and check them again because Google may change documentation and features.

## Google / Gemini Notebook

- Product rename announcement (16 July 2026): https://blog.google/innovation-and-ai/products/gemini-notebook/notebooklm-gemini-notebook/
- Gemini Notebook Help Center: https://support.google.com/gemininotebook/
- Learn about Gemini Notebook: https://support.google.com/gemininotebook/answer/16164461
- Create a notebook: https://support.google.com/gemininotebook/answer/16206563
- Add or discover sources: https://support.google.com/gemininotebook/answer/16215270
- Use chat: https://support.google.com/gemininotebook/answer/16179559
- Create and add notes: https://support.google.com/gemininotebook/answer/16262519
- Mind Maps: https://support.google.com/gemininotebook/answer/16212283
- Audio Overview: https://support.google.com/gemininotebook/answer/16212820
- Video Overview: https://support.google.com/gemininotebook/answer/16454555
- Flashcards and quizzes: https://support.google.com/gemininotebook/answer/16958963
- Slide Decks: https://support.google.com/gemininotebook/answer/16757456
- Infographics: https://support.google.com/gemininotebook/answer/16758265
- Connected apps: https://support.google.com/gemininotebook/answer/18784282
- Public/featured notebooks: https://support.google.com/gemininotebook/answer/16322204
- Plans: https://support.google.com/gemininotebook/answer/16213268
- Usage limits: https://support.google.com/gemininotebook/answer/17670842
- Notebooks in Gemini Apps: https://support.google.com/gemininotebook/answer/17003757
- Flexible usage limits announcement: https://blog.google/innovation-and-ai/products/gemini-notebook/new-flexible-usage-limits/
- Workspace admin guide and plan limit table: https://knowledge.workspace.google.com/admin/generative-ai/gemini-notebook/turn-gemini-notebook-on-or-off-for-users

## REA

- REA main site: https://rea.tools/
- REA guides: https://rea.tools/guides/
- REA source repository: https://github.com/Eng-RSMY/REA

Read the installed version's current documentation. Public GitHub forks, mirrors, and examples may be useful but are not automatically authoritative for the installed build.

## Video tooling

- yt-dlp source repository and README: https://github.com/yt-dlp/yt-dlp
- FFmpeg documentation: https://ffmpeg.org/documentation.html
- FFmpeg filters: https://ffmpeg.org/ffmpeg-filters.html
- FFprobe documentation: https://ffmpeg.org/ffprobe.html

## Additional code/research sources

- `notebooklm-py` and other public protocol research may help identify hypotheses, request shapes, or observed behaviour. Pin the exact repository revision, record its licence, inspect its source and history, and independently validate claims. It is secondary research, not a Google-supported API.
- Official SDK/API documentation must be consulted before implementing a provider integration.
- GitHub issues and community discussions are discovery leads. Verify consequential claims against primary evidence.

---

# 21. Final instruction

Build Forge as a complete, reliable, independent research product.

Investigate the reference product down to the smallest observable details. Use official documents, real browser workflows, screenshots, video analysis, yt-dlp, FFmpeg, FFprobe, REA, source-code research, tests, and real files wherever they are available and authorized.

Use every tool that genuinely helps. Do not spend days fighting a blocked tool when another evidence route is available. Do not mistake a tool's presence for proof that it can inspect the target.

**Research thoroughly. Preserve evidence. Separate facts from inference. Implement the real workflow. Test the complete path. Be honest about every limitation. Keep going.**
