#!/usr/bin/env python3
"""CraftOffice: the hooked egui for apps that ArtCraft moved to a newer egui.

rebase_app.py points such an app's egui crates at `<W>/egui-hook-<rev>/…` (written as
/home/claude/egui-hook-<rev>, like the main hook). This fetches each egui revision named there,
applies CraftOffice's egui patch and the About line (and, for the web, the names), as for egui-hook.

Usage: egui_hooks.py <appsrc> <workspace W> <egui.patch> <VERSION> [--brand]
"""
import glob
import os
import re
import subprocess
import sys

src, w, patch, version = sys.argv[1:5]
brand = "--brand" in sys.argv[5:]
here = os.path.dirname(os.path.abspath(__file__))
revs = set()
for toml in glob.glob(os.path.join(src, "**", "Cargo.toml"), recursive=True):
    if "target" in toml.split(os.sep):
        continue
    with open(toml, encoding="utf-8") as f:
        revs.update(re.findall(r"egui-hook-([0-9a-f]{40})", f.read()))
for rev in sorted(revs):
    d = os.path.join(w, f"egui-hook-{rev}")
    if os.path.isdir(os.path.join(d, ".git")):
        continue
    os.makedirs(d, exist_ok=True)
    git = lambda *a: subprocess.run(["git", "-C", d, *a], check=True)
    git("init", "-q")
    git("fetch", "-q", "--depth", "1", "https://github.com/storytold/egui.git", rev)
    git("-c", "advice.detachedHead=false", "checkout", "-q", "FETCH_HEAD")
    git("apply", "--whitespace=nowarn", os.path.abspath(patch))
    wt = os.path.join(d, "crates", "egui", "src", "widget_text.rs")
    if brand:
        subprocess.run([sys.executable, os.path.join(here, "egui_brand.py"), wt], check=True)
    subprocess.run([sys.executable, os.path.join(here, "egui_about.py"), version, wt], check=True)
    print(f"egui-hook-{rev[:12]}: ready")
