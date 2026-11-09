# Copyright 2026 The justRead Authors
# SPDX-License-Identifier: Apache-2.0

# Builds justRead icon masters from the user's artwork: clean flat green,
# cream letters (anti-aliased alpha recovered from the colour blend), centred.
import sys
from PIL import Image, ImageDraw
import numpy as np

src, out = sys.argv[1], sys.argv[2]
G = np.array([32, 73, 70]); C = np.array([232, 224, 212])
im = np.array(Image.open(src).convert("RGB")).astype(float)
# STEP 1: alpha of the letters = how far each pixel is from green towards cream.
x0, x1, y0, y1 = 330, 1530, 540, 1610
reg = im[y0:y1, x0:x1]
d = C - G
t = ((reg - G) @ d) / (d @ d)
t = np.clip((t - 0.12) / 0.76, 0, 1)  # drop faint background blotches, keep edges smooth
ys, xs = np.nonzero(t > 0.02)
t = t[ys.min():ys.max() + 1, xs.min():xs.max() + 1]
alpha = Image.fromarray((t * 255).astype(np.uint8))
letters = Image.new("RGBA", alpha.size, tuple(C) + (255,))
letters.putalpha(alpha)
letters.save(f"{out}/letters.png")
lw, lh = letters.size

def place(size, width_frac, bg):
    canvas = Image.new("RGBA", (size, size), bg)
    w = round(size * width_frac); h = round(lh * w / lw)
    l = letters.resize((w, h), Image.LANCZOS)
    canvas.alpha_composite(l, ((size - w) // 2, (size - h) // 2))
    return canvas

S = 2048
GREEN = tuple(G) + (255,)
# Full-bleed square (iOS, Android legacy base). Letters ~56% wide, as drawn.
full = place(S, 0.56, GREEN)
# Rounded square with transparent corners (desktop), like the artwork.
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, S - 1, S - 1), radius=round(S * 0.21), fill=255)
rounded = full.copy(); rounded.putalpha(mask); rounded.save(f"{out}/rounded.png")
# Android adaptive foreground: letters only, small enough for circle masks (safe zone 66/108).
fg = place(S, 0.42, (0, 0, 0, 0)); fg.save(f"{out}/foreground.png")
print("letters", letters.size, "green #%02x%02x%02x" % tuple(G))

# STEP 2 (optional, after `tauri icon`): legacy Android launcher icons (pre-8.0
# launchers). `tauri icon` derives them from the adaptive foreground, which
# makes the letters far too large, so draw them from the full artwork instead.
if len(sys.argv) > 3:
    res = sys.argv[3]
    sizes = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
    circle = Image.new("L", (S, S), 0)
    ImageDraw.Draw(circle).ellipse((0, 0, S - 1, S - 1), fill=255)
    round_icon = full.copy(); round_icon.putalpha(circle)
    for dpi, px in sizes.items():
        rounded.resize((px, px), Image.LANCZOS).save(f"{res}/mipmap-{dpi}/ic_launcher.png")
        round_icon.resize((px, px), Image.LANCZOS).save(f"{res}/mipmap-{dpi}/ic_launcher_round.png")
    with open(f"{res}/values/ic_launcher_background.xml", "w") as f:
        f.write('<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
                '  <color name="ic_launcher_background">#204946</color>\n</resources>\n')
