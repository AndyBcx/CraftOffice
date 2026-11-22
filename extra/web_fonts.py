#!/usr/bin/env python3
"""CraftOffice: the fonts that come with the browser (and Android) Writer. A phone has none of
the usual document fonts, so these are copied beside the page, with `list.txt` naming them; the
app adds them to its font list when it starts. All free (OFL, Apache, GPL with font exception).

- Liberation Sans/Serif/Mono: the metrics of Arial, Times New Roman and Courier New
- Carlito, Caladea: the metrics of Calibri and Cambria
- DejaVu, Noto, Open Sans, Roboto, Lato, Comic Neue, EB Garamond, Linux Libertine

Usage: web_fonts.py <output folder>   (after the Debian font packages are installed)
"""
import os
import re
import shutil
import sys

PATTERNS = [
    r"^(LiberationSans|LiberationSerif|LiberationMono|Carlito|Caladea|NotoSans|NotoSerif|OpenSans|Roboto|Lato)-(Regular|Bold|Italic|BoldItalic)\.ttf$",
    r"^DejaVu(Sans|Serif|SansMono)(-Bold|-Oblique|-BoldOblique|-Italic|-BoldItalic)?\.ttf$",
    r"^ComicNeue-(Regular|Bold|Italic|BoldItalic)\.otf$",
    r"^EBGaramond12-(Regular|Bold|Italic)\.otf$",
    r"^LinLibertine_(R|RB|RI|RBI)\.otf$",
]

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
found = {}
for root, _, files in os.walk("/usr/share/fonts"):
    for f in sorted(files):
        p = os.path.join(root, f)
        if f not in found and os.path.isfile(p) and any(re.match(x, f) for x in PATTERNS):
            found[f] = p
for f, p in sorted(found.items()):
    shutil.copyfile(p, os.path.join(out, f))
with open(os.path.join(out, "list.txt"), "w", encoding="utf-8") as fh:
    fh.write("".join(f + "\n" for f in sorted(found)))
total = sum(os.path.getsize(p) for p in found.values())
print(f"{len(found)} fonts, {total / 1e6:.1f} MB")
