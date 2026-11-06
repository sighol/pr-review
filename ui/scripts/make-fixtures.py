#!/usr/bin/env python3
"""Generates the prototype's fake PR from this repo's own git history.

The PR is BASE..HEAD. A file has one *state* per version of its content: the
PR base (state 0) and each commit that touched it. For every changed file we
write, under public/pr/files/<slug>/:

  text-<s>      the file at state s
  diff-<i>-<j>  `git diff` from state i to state j, for every i < j

Any commit range from..to maps to a pair of states (a commit that didn't touch
the file is the same state as the previous one), so every range has a real
git diff available.
"""
import json
import os
import shutil
import subprocess
import sys

BASE = "76578ea"
HEAD = "a91e901"

repo = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
out_root = os.path.join(repo, "ui", "public", "pr")


def git(*args, check=True):
    r = subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed: {r.stderr}")
    return r.stdout


def show(rev, path):
    r = subprocess.run(["git", "-C", repo, "show", f"{rev}:{path}"], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""


def slug(path):
    return path.replace("/", "__")


base_oid = git("rev-parse", BASE).strip()
head_oid = git("rev-parse", HEAD).strip()
commit_oids = git("rev-list", "--reverse", f"{base_oid}..{head_oid}").split()
commits = []
for n, oid in enumerate(commit_oids, start=1):
    commits.append({"n": n, "oid": oid, "short": oid[:7], "subject": git("log", "-1", "--format=%s", oid).strip()})

numstat = {}
for line in git("diff", "--numstat", "--no-renames", base_oid, head_oid).splitlines():
    added, removed, path = line.split("\t")
    numstat[path] = (int(added), int(removed))

# Empty the directory instead of replacing it: deleting `public/pr` itself makes
# a running Vite dev server lose track of it and serve index.html for its files.
os.makedirs(out_root, exist_ok=True)
for entry in os.listdir(out_root):
    path = os.path.join(out_root, entry)
    shutil.rmtree(path) if os.path.isdir(path) else os.remove(path)

files = []
n_diffs = 0
for path in sorted(numstat):
    touching = set(git("rev-list", f"{base_oid}..{head_oid}", "--", path).split())
    changed_by = [c["n"] for c in commits if c["oid"] in touching]
    states = [0] + changed_by

    def oid_of(state):
        return base_oid if state == 0 else commits[state - 1]["oid"]

    d = os.path.join(out_root, "files", slug(path))
    os.makedirs(d)
    for s in states:
        with open(os.path.join(d, f"text-{s}"), "w") as fh:
            fh.write(show(oid_of(s), path))
    for i, si in enumerate(states):
        for sj in states[i + 1 :]:
            diff = git("diff", "--no-color", "--no-renames", "-U3", oid_of(si), oid_of(sj), "--", path)
            with open(os.path.join(d, f"diff-{si}-{sj}"), "w") as fh:
                fh.write(diff)
            n_diffs += 1

    files.append(
        {
            "path": path,
            "dir": slug(path),
            "added": numstat[path][0],
            "removed": numstat[path][1],
            "changedBy": changed_by,
        }
    )

with open(os.path.join(out_root, "manifest.json"), "w") as fh:
    json.dump({"base": base_oid, "head": head_oid, "commits": commits, "files": files}, fh, indent=2)

print(f"wrote {len(files)} files, {len(commits)} commits, {n_diffs} diffs to {out_root}")
for f in files:
    print(f"  {f['path']:36} +{f['added']}/-{f['removed']}  commits {f['changedBy']}")
