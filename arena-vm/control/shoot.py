#!/usr/bin/env python3
"""Capture the virtual display to an image file.
Usage: shoot.py [output.png] [display]
Works headless through Xlib (mss)."""
import os
import sys

import mss
from PIL import Image

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    os.path.dirname(__file__) or ".", "..", "evidence", "shot.png")
out = os.path.abspath(out)
os.makedirs(os.path.dirname(out), exist_ok=True)

with mss.mss(display=os.environ.get("DISPLAY", ":99")) as sct:
    mon = sct.monitors[0]  # virtual screen = all monitors combined
    shot = sct.grab(mon)
    img = Image.frombytes("RGB", shot.size, shot.bgra, "raw", "BGRX")
    img.save(out)
print(out)
