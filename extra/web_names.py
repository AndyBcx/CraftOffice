#!/usr/bin/env python3
"""CraftOffice: the apps' own names in a web build's page (the tab title, "Loading GridCraft…",
the start-up error) replaced with their CraftOffice names. Only the page's text: the program's
files keep their names.

Usage: web_names.py <site folder>   (its .html pages and web app manifest)
"""
import os
import re
import sys

NAMES = {
    "WordCraft": "CraftOffice Writer",
    "GridCraft": "CraftOffice Sheets",
    "DeckCraft": "CraftOffice Slides",
    "DesignCraft": "CraftOffice Layout",
    "PdfCraft": "CraftOffice PDF",
    "PDFCraft": "CraftOffice PDF",
    "PhotoCraft": "CraftOffice Photo",
    "VectorCraft": "CraftOffice Vector",
    "LightCraft": "CraftOffice Darkroom",
    "FilmCraft": "CraftOffice Video",
    "EffectCraft": "CraftOffice Motion",
    "SoundCraft": "CraftOffice Audio",
    "AniCraft": "CraftOffice Animation",
    "FigCraft": "CraftOffice Prototype",
    "CADCraft": "CraftOffice CAD",
    "CadCraft": "CraftOffice CAD",
    "MaterialCraft": "CraftOffice Material",
    "TextureCraft": "CraftOffice Texture",
    "Craft Anything": "CraftOffice Workshop",
}
WORD = re.compile(r"\b(" + "|".join(re.escape(k) for k in sorted(NAMES, key=len, reverse=True)) + r")\b")

paths = []
for d in sys.argv[1:]:
    for f in sorted(os.listdir(d)):
        if f.endswith((".html", ".webmanifest")):
            paths.append(os.path.join(d, f))
for path in paths:
    src = open(path, encoding="utf-8").read()
    out = WORD.sub(lambda m: NAMES[m.group(1)], src)
    if out != src:
        open(path, "w", encoding="utf-8").write(out)
        print(f"{path}: CraftOffice names")
