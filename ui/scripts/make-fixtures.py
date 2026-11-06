#!/usr/bin/env python3
"""Generates prototype fixtures from this repo's git history.

Each fixture is public/fixtures/<name>/{old,new,diff}; the same three inputs
(old text, new text, `git diff` output) the real backend would supply.
"""
import json
import os
import subprocess
import sys

FIXTURES = [
    {"name": "small-main", "from": "a91e901^", "to": "a91e901", "path": "src/main.rs", "label": "small: main.rs (a91e901)"},
    {"name": "medium-grid", "from": "7dacc07^", "to": "7dacc07", "path": "src/grid.rs", "label": "medium: grid.rs (7dacc07)"},
    {"name": "medium-interactive", "from": "7dacc07^", "to": "7dacc07", "path": "src/interactive.rs", "label": "medium: interactive.rs (7dacc07)"},
    {"name": "large-github", "from": "76578ea^", "to": "76578ea", "path": "src/github.rs", "label": "large: github.rs (76578ea)"},
    {"name": "toml-cargo", "from": "03d10ae", "to": "6db66ac", "path": "Cargo.toml", "label": "toml: Cargo.toml (03d10ae..6db66ac)"},
]

repo = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
out_root = os.path.join(repo, "ui", "public", "fixtures")


def git(*args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)


def show(rev, path):
    r = git("show", f"{rev}:{path}")
    return r.stdout if r.returncode == 0 else ""


manifest = []
for f in FIXTURES:
    d = os.path.join(out_root, f["name"])
    os.makedirs(d, exist_ok=True)
    diff = git("diff", "--no-color", "-U3", f["from"], f["to"], "--", f["path"])
    if diff.returncode != 0:
        sys.exit(f"git diff failed for {f['name']}: {diff.stderr}")
    for fname, content in (("old", show(f["from"], f["path"])), ("new", show(f["to"], f["path"])), ("diff", diff.stdout)):
        with open(os.path.join(d, fname), "w") as fh:
            fh.write(content)
    manifest.append({"name": f["name"], "label": f["label"], "path": f["path"]})

with open(os.path.join(out_root, "manifest.json"), "w") as fh:
    json.dump(manifest, fh, indent=2)
print(f"wrote {len(manifest)} fixtures to {out_root}")
