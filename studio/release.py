"""Explicit release gate. It never infers aesthetic approval from a passing build."""

import argparse, hashlib, json, sys
from pathlib import Path


def digest(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def gate(folder, review):
    folder = Path(folder)
    qa = json.loads((folder / "qa.json").read_text())
    r = json.loads(Path(review).read_text())
    errors = []
    if qa.get("automated_checks") != "pass":
        errors.append("Automated preflight did not pass")
    if r.get("pptx_sha256") != digest(folder / "deck.pptx"):
        errors.append("Review is missing or stale for this PPTX")
    if not r.get("reviewer"):
        errors.append("Named reviewer required")
    slides = r.get("slides", [])
    if len(slides) != qa["slide_count"]:
        errors.append("Review every slide")
    seen = set()
    for s in slides:
        n = s.get("index")
        seen.add(n)
        if n not in range(1, qa["slide_count"] + 1):
            errors.append("Invalid slide index")
            continue
        path = folder / "slides" / f"slide-{n:02}.png"
        if s.get("render_sha256") != digest(path):
            errors.append(f"Slide {n} review is stale")
        if s.get("status") != "pass" or not s.get("observations"):
            errors.append(f"Slide {n} needs a substantive visual review")
    if seen != set(range(1, qa["slide_count"] + 1)):
        errors.append("Slide review coverage incomplete")
    if r.get("blockers"):
        errors.append("Unresolved review blockers")
    for app in ["powerpoint", "libreoffice"]:
        test = r.get("office", {}).get(app, {})
        if not test.get("version") or test.get("status") != "pass":
            errors.append(f"{app} compatibility not verified")
    return errors


def main():
    p = argparse.ArgumentParser()
    p.add_argument("folder")
    p.add_argument("review")
    a = p.parse_args()
    try:
        errors = gate(a.folder, a.review)
    except (OSError, ValueError, KeyError, TypeError) as e:
        errors = ["Review unavailable/invalid: " + str(e)]
    print(
        json.dumps(
            dict(release="blocked" if errors else "approved", errors=errors), indent=2
        )
    )
    return bool(errors)


if __name__ == "__main__":
    sys.exit(main())
