#!/usr/bin/env python3
"""Paint the X root window with a solid colour. xsetroot is not part of the
local toolchain, so this uses python-xlib (already a hard dependency)."""
import os
from Xlib import display, X

# deep slate blue, easy on the eyes behind st/dillo
BG = (0x1c, 0x25, 0x33)

d = display.Display(os.environ.get("DISPLAY", ":99"))
root = d.screen().root
# allocate a private colourmap pixel on the default colormap
cmap = d.screen().default_colormap
pixel = cmap.alloc_color(BG[0] << 8, BG[1] << 8, BG[2] << 8).pixel
root.change_attributes(background_pixel=pixel)
root.clear_area(0, 0, 0, 0)
d.sync()
d.close()
print("root background set")
