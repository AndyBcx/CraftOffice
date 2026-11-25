#!/usr/bin/env python3
"""CraftOffice: the hooked egui for apps that ArtCraft moved to a newer egui.

rebase_app.py points such an app's egui crates at `<W>/egui-hook-<rev>/…` (written as
/home/claude/egui-hook-<rev>, like the main hook). This fetches each egui revision named there,
applies CraftOffice's egui patch and the About line (and, for the web, the names), as for egui-hook.

An app on a crates.io egui release (no storytold fork) names `egui-hook-crate-<version>`: that egui
crate is downloaded from crates.io and gets the same patch (only the `egui` crate is replaced, so
it stays compatible with the rest of that release).

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
crates = set()
for toml in glob.glob(os.path.join(src, "**", "Cargo.toml"), recursive=True):
    if "target" in toml.split(os.sep):
        continue
    with open(toml, encoding="utf-8") as f:
        text = f.read()
        revs.update(re.findall(r"egui-hook-([0-9a-f]{40})", text))
        crates.update(re.findall(r"egui-hook-crate-(\d+\.\d+\.\d+)", text))
for ver in sorted(crates):
    d = os.path.join(w, f"egui-hook-crate-{ver}")
    if os.path.isfile(os.path.join(d, "Cargo.toml")):
        continue
    import io
    import tarfile
    import urllib.request

    data = urllib.request.urlopen(f"https://static.crates.io/crates/egui/egui-{ver}.crate", timeout=120).read()
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        for m in tar.getmembers():
            name = m.name.split("/", 1)[1] if "/" in m.name else ""
            if not name or ".." in name.split("/"):
                continue
            m.name = name
            if hasattr(tarfile, "data_filter"):
                tar.extract(m, d, filter="data")
            else:
                tar.extract(m, d)
    subprocess.run(["git", "apply", "-p3", "--whitespace=nowarn", os.path.abspath(patch)], cwd=d, check=True)
    wt = os.path.join(d, "src", "widget_text.rs")
    if brand:
        subprocess.run([sys.executable, os.path.join(here, "egui_brand.py"), wt], check=True)
    subprocess.run([sys.executable, os.path.join(here, "egui_about.py"), version, wt], check=True)
    print(f"egui-hook-crate-{ver}: ready")
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
