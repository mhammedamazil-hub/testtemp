from pathlib import Path

FONT_ROOT = Path(__file__).resolve().parents[1] / "assets/fonts"
FONTS = {
    "sans": ("IBM Plex Sans", FONT_ROOT / "ibmplexsans/IBMPlexSans[wdth,wght].ttf"),
    "serif": (
        "Cormorant Garamond",
        FONT_ROOT / "cormorantgaramond/CormorantGaramond[wght].ttf",
    ),
    "condensed": (
        "Barlow Condensed",
        FONT_ROOT / "barlowcondensed/BarlowCondensed-Regular.ttf",
    ),
}
# Directions describe pacing and information treatment, not just palettes.
DIRECTIONS = {
    "swiss": dict(
        bg="#F2F0E9",
        ink="#161616",
        accent="#B62B21",
        display="sans",
        body="sans",
        rule="Evidence first; asymmetry, flush-left type, restrained rules.",
    ),
    "luxury": dict(
        bg="#F3ECE1",
        ink="#30271F",
        accent="#775523",
        display="serif",
        body="sans",
        rule="Slow pacing; large serif statements; no simulated gold effects.",
    ),
    "architectural": dict(
        bg="#EDECE6",
        ink="#23352E",
        accent="#3D6651",
        display="sans",
        body="sans",
        rule="Plans, material studies and measured spatial relationships.",
    ),
    "cinematic": dict(
        bg="#101925",
        ink="#F0ECE3",
        accent="#F3BC70",
        display="condensed",
        body="sans",
        rule="Large visual anchors; purposeful reveals; minimal simultaneous claims.",
    ),
    "scientific": dict(
        bg="#F1F5F2",
        ink="#123C39",
        accent="#A12A22",
        display="sans",
        body="sans",
        rule="Label mechanisms precisely; distinguish models from observations.",
    ),
    "corporate": dict(
        bg="#F6F3EA",
        ink="#142F42",
        accent="#9A4025",
        display="sans",
        body="sans",
        rule="Action headlines; evidence, implication, decision.",
    ),
    "brutalist": dict(
        bg="#F4E441",
        ink="#171717",
        accent="#171717",
        display="condensed",
        body="sans",
        rule="Oversized language, deliberate hard edges, no gratuitous containers.",
    ),
    "museum": dict(
        bg="#F0E7D7",
        ink="#332C25",
        accent="#84452D",
        display="serif",
        body="sans",
        rule="Object, context, interpretation; captions with provenance.",
    ),
    "industrial": dict(
        bg="#17252D",
        ink="#EDF2ED",
        accent="#E3C860",
        display="condensed",
        body="sans",
        rule="System views, component names, engineering logic; avoid neon decoration.",
    ),
    "magazine": dict(
        bg="#F3EEE9",
        ink="#302030",
        accent="#9F2741",
        display="serif",
        body="sans",
        rule="Editorial sequencing, long-form notes, strong image/type relationships.",
    ),
}
