#!/usr/bin/env python3
"""CraftOffice: an app at a newer ArtCraft revision with CraftOffice's changes carried over.

The CraftOffice patch (patches/<app>.patch) was made against an older revision (OLD). This fetches
both revisions, applies the patch to OLD, and replays that change on NEW (a cherry-pick). Where
ArtCraft changed the same lines, ArtCraft's version is kept (CraftOffice's names and translations
still come through the text hook), and in Cargo.toml ArtCraft's dependencies are kept while the egui
crates still point at CraftOffice's hooked egui. The same inputs always give the same tree, so the
extra/<app>-N.patch files made on it apply.

Usage: rebase_app.py <app> <OLD> <NEW> <dir> <patch made on OLD>...
"""
import os
import re
import subprocess
import sys

app, old, new, d = sys.argv[1:5]
patches = [os.path.abspath(p) for p in sys.argv[5:] if os.path.isfile(p)]


def git(*args, check=True, capture=False):
    r = subprocess.run(["git", "-C", d, *args], check=check, text=True, capture_output=capture)
    return r.stdout if capture else r.returncode


def resolve_text(path):
    with open(path, encoding="utf-8", newline="") as f:
        s = f.read()
    if path.replace("\\", "/").endswith("Cargo.toml"):
        def cargo(m):
            head, ours = m.group(1), m.group(2)
            hooked = {}
            for line in ours.splitlines(True):
                if "egui-hook" in line:
                    hooked[line.split("=")[0].strip()] = line
            return "".join(hooked.get(line.split("=")[0].strip(), line) for line in head.splitlines(True))
        s = re.sub(r"<<<<<<< [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n", cargo, s, flags=re.S)
    else:
        s = re.sub(r"<<<<<<< [^\n]*\n(.*?)=======\n.*?>>>>>>> [^\n]*\n", lambda m: m.group(1), s, flags=re.S)
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(s)


os.makedirs(d, exist_ok=True)
git("init", "-q")
url = f"https://github.com/storytold/{app}.git"
# the history between the two (no file contents until needed), for the replay
git("fetch", "-q", "--filter=blob:none", url, new)
git("fetch", "-q", "--filter=blob:none", url, old)
git("-c", "advice.detachedHead=false", "checkout", "-q", "-f", old)
for p in patches:
    git("apply", "--whitespace=nowarn", p)
git("add", "-A")
git("-c", "user.name=CraftOffice", "-c", "user.email=craftoffice@localhost", "-c", "commit.gpgsign=false",
    "commit", "-q", "--date=2000-01-01T00:00:00Z", "-m", "CraftOffice")
change = git("rev-parse", "HEAD", capture=True).strip()
git("-c", "advice.detachedHead=false", "checkout", "-q", "-f", new)
rc = git("-c", "user.name=CraftOffice", "-c", "user.email=craftoffice@localhost", "cherry-pick", "--no-commit", change, check=False)
if rc != 0:
    status = git("status", "--porcelain", capture=True)
    for line in status.splitlines():
        code, path = line[:2], line[3:]
        if path.startswith('"'):
            path = path.strip('"')
        full = os.path.join(d, path)
        if code in ("UU", "AA"):
            resolve_text(full)
            git("add", "--", path)
            print(f"{app}: kept ArtCraft's lines where both changed: {path}")
        elif code == "DU":  # gone in ArtCraft: stays gone
            git("rm", "-q", "--", path)
            print(f"{app}: removed by ArtCraft: {path}")
        elif code == "UD":  # CraftOffice deleted it, ArtCraft changed it: ArtCraft's
            git("add", "--", path)
        elif code in ("AU", "UA", "DD"):
            git("add", "-A", "--", path)
    left = [l for l in git("diff", "--name-only", "--diff-filter=U", capture=True).splitlines() if l]
    if left:
        sys.exit(f"{app}: unresolved: {left}")
print(f"{app}: CraftOffice changes on {new[:12]}")
