# Studio artifacts — reference inventory vs Forge

Reference inventory comes from the specification's research list plus help topics named
in `OFFICIAL_DOC_INDEX.md`. **Only the sources page was read in full this session**, so
per-artifact control names are `UNVERIFIED` unless stated. Forge's column is verified by
executing the tests.

| Reference artifact | Documented controls (spec inventory) | Forge | Reality check |
|---|---|---|---|
| Summary | — | **yes** | extractive, cited, persisted |
| Report | templates, custom prompt | **yes** (structured text report) | interactive report templates: no |
| Study guide | — | **yes** | from source headings + definitions |
| FAQ | — | **yes** | questions derived from evidence sentences |
| Briefing / learning overview | — | **partial** (report + study guide cover the shape) | not a separate artifact |
| Mind map | expand/collapse, ask about node, export | **yes** (SVG hierarchy, node→evidence links) | pan/zoom: basic |
| Flashcards | difficulty, quantity, flip, known/unknown | **yes** | spaced repetition: no |
| Quiz | difficulty, count, explanations, score | **yes** (citations per answer) | retake/history: partial |
| Audio Overview | formats, voices, language, custom prompt, player, transcript | **script only** | TTS unavailable — never returns a fake audio file |
| Video Overview | styles, producer notes, render | **storyboard/plan only** | no TTS, no FFmpeg |
| Slides | Detailed Deck / Presenter Slides, outline review, revise, PPTX/PDF export | **no** | needs a renderer; not faked |
| Infographics | language, detail, orientation, style, PNG | **no** | needs an image model |
| Data Tables | describe the table, per-cell evidence, CSV/XLSX | **no** (planned) | genuinely useful and provider-free → high priority |
| Timeline | — | **no** (planned) | derivable from dated evidence |

## The rule Forge applies to itself

`MASTER_FORGE.md` §7.10/§7.11: *do not fake generation with a placeholder, and do not
call a script a podcast.* Forge therefore emits a **narration script** and labels audio
and video as unsupported in `/api/capabilities`, with the reason (`no TTS engine`), and
the capability page renders that reason next to the row.
