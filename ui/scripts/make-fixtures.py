#!/usr/bin/env python3
"""Generates the prototype's fake PR from this repo's own git history.

The PR is BASE..HEAD. For every changed file we write, under
public/pr/files/<slug>/:

  new        the file at the latest commit that changed it
  old-<k>    the file as of commit k (k=0 is the PR base)
  diff-<k>   `git diff` from commit k to the file's latest commit

k ranges over the PR base and every commit that touched the file except its
latest one. That is exactly the set of start points the TUI can diff from
("last approved commit, or the PR base").
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

if os.path.isdir(out_root):
    shutil.rmtree(out_root)
os.makedirs(out_root)

files = []
for path in sorted(numstat):
    touching = set(git("rev-list", f"{base_oid}..{head_oid}", "--", path).split())
    changed_by = [c["n"] for c in commits if c["oid"] in touching]
    latest = changed_by[-1]
    latest_oid = commits[latest - 1]["oid"]
    starts = [0] + changed_by[:-1]

    d = os.path.join(out_root, "files", slug(path))
    os.makedirs(d)
    with open(os.path.join(d, "new"), "w") as fh:
        fh.write(show(latest_oid, path))
    for k in starts:
        from_oid = base_oid if k == 0 else commits[k - 1]["oid"]
        with open(os.path.join(d, f"old-{k}"), "w") as fh:
            fh.write(show(from_oid, path))
        diff = git("diff", "--no-color", "--no-renames", "-U3", from_oid, latest_oid, "--", path)
        with open(os.path.join(d, f"diff-{k}"), "w") as fh:
            fh.write(diff)

    files.append(
        {
            "path": path,
            "dir": slug(path),
            "added": numstat[path][0],
            "removed": numstat[path][1],
            "changedBy": changed_by,
            "from": starts,
        }
    )

with open(os.path.join(out_root, "manifest.json"), "w") as fh:
    json.dump({"base": base_oid, "head": head_oid, "commits": commits, "files": files}, fh, indent=2)

print(f"wrote {len(files)} files, {len(commits)} commits to {out_root}")
for f in files:
    print(f"  {f['path']:36} +{f['added']}/-{f['removed']}  commits {f['changedBy']}")
