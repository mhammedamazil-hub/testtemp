# Persistent presentation design knowledge

## Before composition
Write the audience, decision/learning objective, emotional intent, core message, complexity and source boundaries. A deck has a beginning (question), middle (evidence/mechanism/trade-off) and end (implication/action). Do not present generic motivational headlines as if they were findings. The author remains responsible for verifying sources; a validator cannot decide truth.

Choose a direction because it supports the subject. `studio/tokens.py` defines ten starting directions: Swiss, luxury editorial, architectural, cinematic, scientific, corporate, brutalist, museum, industrial, magazine. They include typographic pairing, palette and direction rules, not a promise that colour changes alone create a new identity. Direction-specific layout and asset systems remain an area for further development.

## Composition
One principal message per slide. Select a hero when the object is evidence; a timeline for chronology; a mechanism for ordered causal steps; a comparison for a real contrast; a chart for sourced numerical data; an editorial statement for synthesis. Containers must encode grouping, not fill space. Do not use card grids by default. Preserve grid logic while varying scale and visual mass.

Use relevant real photographs or illustrations with licenses and captions. Raster assets must declare path, source, license, alt text. An original Blender product concept is not a photograph of an existing product. Diagrams must distinguish schematic geometry from measured/physical claims.

## Type / tokens
A 960×540 point stage is rendered at 1920×1080. Typeface choices: IBM Plex Sans for explanatory prose, Cormorant Garamond for historical/editorial display, Barlow Condensed for industrial display. They are OFL fonts bundled with their licenses. Use 20–25pt body, 36–56pt display, at least 12pt metadata. Keep metadata secondary, not essential evidence. Measured wrapping must fail on overflow instead of silently shrinking to unreadability. Optical alignment and final text appearance still require visual judgment; Pillow metrics and DeckCraft shaping differ.

## Real 3D
Use only when it explains the subject: membranes, assemblies, spaces. bpy must create geometry, PBR materials, lights and cameras. CPU Cycles is the default; optional GPU discovery is opportunistic. Keep PNG alpha, original .blend, spec and generator. Missing tool/libraries must stop asset production. Never substitute a CSS gradient, fabricated placeholder render, or a flat fake shadow and call it 3D.

## Quality criteria / release benchmark
- Zero out-of-stage elements, text/text intersections, missing media, bad relationships or wrong-aspect exports.
- Measured text fit with conservative line height; no body text shrunk below the defined scale.
- Contrast ≥4.5 for normal text, ≥3 for display text ≥24pt. This is an internal screen-slide gate, not full WCAG certification.
- No layout used for over half of a deck ≥4 slides without a documented design exception (not yet supported; revise composition instead).
- Native text in every slide; native chart parts when numerical charts are used.
- Render every exported slide at full resolution. Inspect slides and the deck-wide contact sheet. Review hierarchy, truthful content, asset relevance, image crop/alpha, collisions, font substitutions and rhythm.
- Technical audit and aesthetic critique are different. Automated pass means **draft ready for review**, not professional completion.
- Repair the source brief/composition and regenerate; do not hide problems by flattening the deck.
- Release requires review evidence tied to artifact hashes, no unresolved blockers, and an explicit statement of which Office applications were actually tested.
