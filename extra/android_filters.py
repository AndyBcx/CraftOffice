#!/usr/bin/env python3
"""CraftOffice on Android: the file types each app opens from other apps ("Open with…" in a file
manager, a download, an e-mail attachment) and takes when something is shared with it.

Usage: android_filters.py <app id> <AndroidManifest.xml>
Replaces the `<!-- CRAFTOFFICE_FILTERS -->` mark in the manifest with the app's intent filters.
"""
import sys

DOCX = "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
XLSX = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
PPTX = "application/vnd.openxmlformats-officedocument.presentationml.presentation"
IMAGES = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp", "image/tiff"]

TYPES = {
    "wordcraft": [DOCX, "application/msword", "application/vnd.oasis.opendocument.text", "application/rtf",
                  "text/rtf", "text/plain", "text/markdown", "text/html"],
    "gridcraft": [XLSX, "application/vnd.ms-excel", "application/vnd.oasis.opendocument.spreadsheet",
                  "text/csv", "text/comma-separated-values", "text/tab-separated-values"],
    "deckcraft": [PPTX, "application/vnd.ms-powerpoint", "application/vnd.oasis.opendocument.presentation"],
    "designcraft": ["application/vnd.adobe.indesign-idml-package", "application/x-indesign"],
    "pdfcraft": ["application/pdf"],
    "photocraft": IMAGES + ["image/vnd.adobe.photoshop", "image/x-photoshop", "application/x-photoshop"],
    "vectorcraft": ["image/svg+xml", "application/illustrator", "application/postscript", "application/eps"],
    "lightcraft": IMAGES + ["image/x-adobe-dng", "image/x-canon-cr2", "image/x-nikon-nef", "image/x-sony-arw",
                            "image/heic", "image/heif"],
    "filmcraft": ["video/*"],
    "effectcraft": ["video/*"],
    "soundcraft": ["audio/*"],
    "figcraft": ["image/svg+xml"],
    "cadcraft": ["image/vnd.dxf", "application/dxf", "image/vnd.dwg", "application/acad", "model/step",
                 "model/stl", "model/obj"],
    "materialcraft": [],
    "texturecraft": [],
    "craft-anything": ["*/*"],
    "sketch": IMAGES,
}


def filters(types):
    if not types:
        return ""
    data = "\n".join(f'                <data android:mimeType="{t}" />' for t in types)
    out = []
    for action in ("android.intent.action.VIEW", "android.intent.action.SEND"):
        cats = '                <category android:name="android.intent.category.DEFAULT" />'
        if action.endswith("VIEW"):
            cats += '\n                <category android:name="android.intent.category.BROWSABLE" />'
        out.append(
            "            <intent-filter>\n"
            f'                <action android:name="{action}" />\n'
            f"{cats}\n{data}\n"
            "            </intent-filter>"
        )
    return "\n".join(out)


app, path = sys.argv[1], sys.argv[2]
src = open(path, encoding="utf-8").read()
mark = "<!-- CRAFTOFFICE_FILTERS -->"
if mark not in src:
    sys.exit(f"{path}: no {mark}")
src = src.replace(mark, filters(TYPES.get(app, [])))
open(path, "w", encoding="utf-8").write(src)
print(f"{app}: {len(TYPES.get(app, []))} file types")
