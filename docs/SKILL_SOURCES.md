# Skill investigation / integration ledger

The requested `find-skills` discovery was attempted using the skills CLI:
`npx --yes skills find presentation`, `... find blender`, `... find typography`.
All returned **No skills found** in this environment. That is a discovery limitation, not evidence that no specialist skills exist. skills.sh's host is outside the sandbox allowlist. GitHub repositories were fetched directly and inspected instead. No opaque skill installer or unknown script was run.

| Source / revision inspected | License / dependencies | Decision |
|---|---|---|
| pbakaus/impeccable @ c1e0e5492fbb524f5df84d635ecfae7dc438ec45 | Apache-2.0; current launcher downloads a binary, references browser/CDP/native UI context | Read SKILL, audit, critique/typeset references. Adapt audit-vs-critique, contrast, implementation integrity and rendered-truth principles. Do not claim its context launcher/CDP detector ran. |
| vercel-labs/agent-skills @ 1a2c79d4e6c4a2283dc2c614b6538cbe1ed5ca62 | Reviewed web-design-guidelines SKILL; no top-level license found in checkout; guideline URL requires additional fetch; React skills irrelevant to Rust/CLI | No redistribution/installation of uncertain-license content. Independently implement accessible contrast/review controls; no React dependency added. |
| zarazhangrui/frontend-slides @ 9906a34d640d2111f724544cbc50f7f130569ae1 | MIT; zero-dependency HTML authoring; fixed stage; HTML-to-PPTX mismatch | Read SKILL; adapt fixed-stage composition, deliberate art direction, typography and density. Not installed as a misleading HTML renderer for native PPTX. MIT notice retained. |
| anthropics/skills @ 2c5958ecac7cee2b583967a2c4981d0f4a75c579 | Per-skill licensing: pptx LICENSE.txt Apache-2.0; pptxgenjs, office tools and fonts differ from our runtime | Read pptx, canvas-design guidance. Adapt native chart/text, notes, roundtrip/inspect patterns. No claim that preinstalled pptxgenjs or LibreOffice exists. Preserve DeckCraft writer instead. |
| openai/skills @ 49f948faa9258a0c61caceaf225e179651397431 | Per-skill licenses (pdf Apache-2.0); PDF/screenshot/Figma workflows depend on tools and app access | Inspect PDF/screenshot/Figma scope; no unnecessary Figma or browser integration installed. Native renderer gives slide screenshots without Playwright. |

## Activated integration
`.claude/skills/presentation-production/SKILL.md` is an actual project-local adapted skill. `AGENTS.md` explicitly routes future slide tasks to it and the persistent design knowledge. This is **not** a global Arena skill registration or an upstream plugin installation. No tool in this environment offers that registration. Adaptation is explicit rather than pretending incompatible upstream skills are installed.

## Specialist coverage
Editorial/art direction/type: Frontend Slides, Impeccable typeset/critique, Anthropic canvas-design (principles, not its 90%-visual mandate for information slides). Storytelling: structured source/message/sequence contract. Information/data visualization: native mechanism/timeline/comparison/chart adapter, source validation. Graphic design: measured typography, grid, contrast. 3D: existing bpy-oriented project instructions inspected but their retro game constraints are inappropriate here; new native scene generator takes precedence for presentations. Accessibility/visual QA: contrast, structure, fit, alt provenance and a review gate. Specialist Blender-render audit remains necessary; code generation is not a substitute for rendered inspection.

Fonts are retrieved from google/fonts through GitHub's API with their SIL Open Font License notices. Asset licenses and source URLs must accompany future photographs/models. No paid APIs required.
