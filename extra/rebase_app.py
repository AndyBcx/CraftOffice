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

# the egui revision CraftOffice's hooked egui (egui-hook) is made from (EGUI_REF in the workflows)
HOOK_REV = "6d518fc799681571963842f95b55a6b68431b15a"

app, old, new, d = sys.argv[1:5]
patches = [os.path.abspath(p) for p in sys.argv[5:] if os.path.isfile(p)]


def git(*args, check=True, capture=False):
    r = subprocess.run(["git", "-C", d, *args], check=check, text=True, capture_output=capture)
    return r.stdout if capture else r.returncode


def merge_block(head, base, ours):
    """ArtCraft's lines, plus the lines CraftOffice only added (each after the line it followed, when
    ArtCraft still has that line). Where CraftOffice changed or removed lines, ArtCraft's stay."""
    import difflib
    h, b, o = head.splitlines(True), base.splitlines(True), ours.splitlines(True)
    adds = []  # (base line before the insertion or None, lines)
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, b, o, autojunk=False).get_opcodes():
        if tag == "insert":
            adds.append((b[i1 - 1] if i1 > 0 else None, o[j1:j2]))
        elif tag in ("replace", "delete"):
            pass  # a CraftOffice edit of a line ArtCraft also changed: ArtCraft's wins
    out = list(h)
    for anchor, lines in reversed(adds):
        if anchor is None:
            at = 0
        elif anchor in out:
            at = len(out) - out[::-1].index(anchor)
        else:
            at = len(out)
        if "".join(lines) not in "".join(out):
            out[at:at] = lines
    return "".join(out)


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
            out = []
            for line in head.splitlines(True):
                mine = hooked.get(line.split("=")[0].strip())
                rev = re.search(r'storytold/egui(?:\.git)?", rev = "([0-9a-f]{40})"', line)
                if mine and rev and rev.group(1) != HOOK_REV:
                    # ArtCraft moved this app to a newer egui: the same hook on that revision
                    # (extra/egui_hooks.py fetches and patches egui-hook-<rev>)
                    mine = mine.replace("/egui-hook/", f"/egui-hook-{rev.group(1)}/")
                out.append(mine or line)
            return "".join(out)
        s = re.sub(r"<<<<<<< [^\n]*\n(.*?)\|\|\|\|\|\|\| [^\n]*\n.*?=======\n(.*?)>>>>>>> [^\n]*\n", cargo, s, flags=re.S)
    else:
        s = re.sub(r"<<<<<<< [^\n]*\n(.*?)\|\|\|\|\|\|\| [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n",
                   lambda m: merge_block(m.group(1), m.group(2), m.group(3)), s, flags=re.S)
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
rc = git("-c", "user.name=CraftOffice", "-c", "user.email=craftoffice@localhost", "-c", "merge.conflictStyle=diff3",
         "cherry-pick", "--no-commit", change, check=False)
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
            print(f"{app}: ArtCraft's lines kept where both changed (CraftOffice's added lines carried over): {path}")
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
