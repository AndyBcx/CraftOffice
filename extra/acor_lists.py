#!/usr/bin/env python3
"""LibreOffice's Hungarian and Romanian AutoCorrect lists as acor_*.dat files (zips, as an
installed LibreOffice has them) for the package's `szotarak` folder, where Writer finds them.
Usage: acor_lists.py OUTDIR  (pinned by LO_CORE_REF)"""
import io
import os
import sys
import urllib.request
import zipfile

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
CORE = "https://raw.githubusercontent.com/LibreOffice/core/" + os.environ["LO_CORE_REF"]
for lang, tag in [("hu", "hu-HU"), ("ro", "ro-RO")]:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ["DocumentList.xml", "SentenceExceptList.xml", "WordExceptList.xml"]:
            try:
                with urllib.request.urlopen(f"{CORE}/extras/source/autocorr/lang/{lang}/{name}", timeout=120) as r:
                    z.writestr(name, r.read())
            except Exception as e:
                print(f"autocorr {lang}/{name}: {e}", file=sys.stderr)
    path = os.path.join(out, f"acor_{tag}.dat")
    with open(path, "wb") as f:
        f.write(buf.getvalue())
    print(f"{path}: {len(buf.getvalue())} bytes", file=sys.stderr)
