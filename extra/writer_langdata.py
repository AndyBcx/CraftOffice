#!/usr/bin/env python3
"""Hungarian and Romanian language data built into Writer for the browser and Android.

Usage: writer_langdata.py PROOF_DICTS_DIR ENGINE_LANGDATA_DIR

- hyphenation: LibreOffice's hyph_hu_HU.dic / hyph_ro_RO.dic as plain Liang patterns
  (charset and keyword lines, comments and the non-standard `ssz/sz=sz,1,3` patterns dropped)
  -> PROOF_DICTS_DIR/hyph_{hu,ro}.pat.z  (craft-proof, extra/craft-proof.patch)
- thesaurus: th_hu_HU_v2.dat / th_ro_RO_v2.dat (MyThes) -> ENGINE_LANGDATA_DIR/th_{hu,ro}.z
- AutoCorrect: LibreOffice's autocorr lists (DocumentList, SentenceExceptList, WordExceptList)
  zipped like an acor_*.dat -> ENGINE_LANGDATA_DIR/acor_{hu,ro}.z  (extra/wordcraft-4.patch)

Everything is zlib-compressed. Pinned by LO_DICT_REF (LibreOffice/dictionaries) and
LO_CORE_REF (LibreOffice/core)."""
import io
import os
import re
import sys
import urllib.request
import zipfile
import zlib

proof_dir, engine_dir = sys.argv[1], sys.argv[2]
os.makedirs(proof_dir, exist_ok=True)
os.makedirs(engine_dir, exist_ok=True)
DICT = "https://raw.githubusercontent.com/LibreOffice/dictionaries/" + os.environ["LO_DICT_REF"]
CORE = "https://raw.githubusercontent.com/LibreOffice/core/" + os.environ["LO_CORE_REF"]


def get(url):
    with urllib.request.urlopen(url, timeout=120) as r:
        return r.read()


def put(path, data):
    z = zlib.compress(data, 9)
    with open(path, "wb") as f:
        f.write(z)
    print(f"{path}: {len(data)} -> {len(z)} bytes", file=sys.stderr)


def liang(text):
    out = []
    for n, line in enumerate(text.splitlines()):
        line = line.split("%", 1)[0].strip()
        if not line or n == 0:
            continue
        for tok in line.split():
            if "/" in tok or "-" in tok or re.fullmatch(r"[A-Z_0-9]+", tok):
                continue
            out.append(tok)
    return ("\n".join(out) + "\n").encode("utf-8")


for lang, path in [("hu", "hu_HU/hyph_hu_HU.dic"), ("ro", "ro/hyph_ro_RO.dic")]:
    put(os.path.join(proof_dir, f"hyph_{lang}.pat.z"), liang(get(f"{DICT}/{path}").decode("utf-8", "replace")))

for lang, path in [("hu", "hu_HU/th_hu_HU_v2.dat"), ("ro", "ro/th_ro_RO_v2.dat")]:
    put(os.path.join(engine_dir, f"th_{lang}.z"), get(f"{DICT}/{path}"))

for lang in ["hu", "ro"]:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ["DocumentList.xml", "SentenceExceptList.xml", "WordExceptList.xml"]:
            try:
                z.writestr(name, get(f"{CORE}/extras/source/autocorr/lang/{lang}/{name}"))
            except Exception as e:  # a language without one of the lists
                print(f"autocorr {lang}/{name}: {e}", file=sys.stderr)
    put(os.path.join(engine_dir, f"acor_{lang}.z"), buf.getvalue())
