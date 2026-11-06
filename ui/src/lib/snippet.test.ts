import { expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { snippetFor } from "./snippet";

// src/main.rs diffed from commit 17 to its latest commit (18).
const read = (f: string) => readFileSync(`public/pr/files/src__main.rs/${f}`, "utf8");
const file = { oldText: read("old-17"), newText: read("new"), diffText: read("diff-17") };

test("new-side snippet returns the covered lines with their kind", () => {
  const s = snippetFor(file, { side: "new", start: 41, end: 43 });
  expect(s.map((l) => l.lineNo)).toEqual([41, 42, 43]);
  expect(s.map((l) => l.kind)).toEqual(["ctx", "add", "add"]);
  expect(s[1].text).toContain("fn traverse_up_to_git_repo");
});

test("old-side snippet shows deleted lines", () => {
  const s = snippetFor(file, { side: "old", start: 12, end: 12 });
  expect(s).toEqual([{ lineNo: 12, text: "use std::path::Path;", kind: "del" }]);
});
