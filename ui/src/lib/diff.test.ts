import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import {
  buildRows,
  clampDrag,
  parseUnifiedDiff,
  rangeFromRows,
  segmentsFor,
  suggestionBlock,
  textToLines,
  wordRanges,
  type Row,
} from "./diff";

interface ManifestFile {
  path: string;
  dir: string;
  changedBy: number[];
}
const manifest: { files: ManifestFile[] } = JSON.parse(readFileSync("public/pr/manifest.json", "utf8"));

/** Old text at state `from`, new text at state `to`, and the git diff between them. */
function load(dir: string, from: number, to: number) {
  const read = (f: string) => readFileSync(`public/pr/files/${dir}/${f}`, "utf8");
  return {
    oldLines: textToLines(read(`text-${from}`)),
    newLines: textToLines(read(`text-${to}`)),
    hunks: parseUnifiedDiff(read(`diff-${from}-${to}`)),
  };
}

/** src/main.rs from state 17 to state 18: two small hunks. */
const loadSmall = () => load("src__main.rs", 17, 18);

function checkCoverage({ oldLines, newLines, hunks }: ReturnType<typeof load>, wholeFile: boolean) {
  const rows = buildRows(oldLines, newLines, hunks, { wholeFile, expandedGaps: new Set() });
  const oldSeen: number[] = [];
  const newSeen: number[] = [];
  for (const r of rows) {
    if (r.type === "gap") {
      for (let k = 0; k < r.count; k++) {
        oldSeen.push(r.oldStart + k);
        newSeen.push(r.newStart + k);
      }
    } else {
      if (r.left) {
        oldSeen.push(r.left.lineNo);
        expect(r.left.text).toBe(oldLines[r.left.lineNo - 1]);
      }
      if (r.right) {
        newSeen.push(r.right.lineNo);
        expect(r.right.text).toBe(newLines[r.right.lineNo - 1]);
      }
    }
  }
  expect(oldSeen).toEqual(oldLines.map((_, i) => i + 1));
  expect(newSeen).toEqual(newLines.map((_, i) => i + 1));
}

describe("buildRows covers every line exactly once, in order", () => {
  for (const f of manifest.files) {
    const states = [0, ...f.changedBy];
    for (let i = 0; i < states.length; i++) {
      for (let j = i + 1; j < states.length; j++) {
        for (const wholeFile of [false, true]) {
          test(`${f.path} state ${states[i]}..${states[j]} wholeFile=${wholeFile}`, () => {
            checkCoverage(load(f.dir, states[i], states[j]), wholeFile);
          });
        }
      }
    }
  }
});

describe("parseUnifiedDiff", () => {
  test("main.rs state 17..18 has two hunks with expected shape", () => {
    const { hunks } = loadSmall();
    expect(hunks).toHaveLength(2);
    expect(hunks[0]).toMatchObject({ oldStart: 9, oldCount: 7, newStart: 9, newCount: 7 });
    expect(hunks[0].lines.filter((l) => l.kind === "del")).toHaveLength(1);
  });

  test("handles new-file hunks (-0,0) and missing-newline markers", () => {
    const hunks = parseUnifiedDiff("@@ -0,0 +1,2 @@\n+a\n+b\n\\ No newline at end of file\n");
    expect(hunks[0].lines).toEqual([
      { kind: "add", text: "a" },
      { kind: "add", text: "b", noEol: true },
    ]);
    const rows = buildRows([], ["a", "b"], hunks, { wholeFile: false, expandedGaps: new Set() });
    expect(rows).toHaveLength(2);
    expect(rows.every((r) => r.type === "line" && r.left === null)).toBe(true);
  });
});

describe("word ranges", () => {
  test("a single changed word is the only highlighted range", () => {
    const w = wordRanges("use std::path::Path;", "use std::path::PathBuf;");
    const slice = (s: string, r: [number, number][]) => r.map(([a, b]) => s.slice(a, b));
    expect(slice("use std::path::Path;", w.old).join("")).toContain("Path");
    expect(slice("use std::path::PathBuf;", w.new).join("")).toContain("PathBuf");
    expect(w.old.length).toBe(1);
    expect(w.new.length).toBe(1);
  });

  test("pairs are positional: unequal del/add blocks leave filler cells", () => {
    const hunks = parseUnifiedDiff("@@ -1,2 +1,3 @@\n-a\n-b\n+A\n+B\n+C\n");
    const rows = buildRows(["a", "b"], ["A", "B", "C"], hunks, { wholeFile: false, expandedGaps: new Set() });
    expect(rows).toHaveLength(3);
    const last = rows[2];
    expect(last.type === "line" && last.left === null && last.right?.text === "C").toBe(true);
  });
});

describe("segmentsFor", () => {
  test("splits tokens at range boundaries and preserves text", () => {
    const segs = segmentsFor(
      [
        { content: "let ", color: "#f00" },
        { content: "value", color: "#0f0" },
      ],
      "let value",
      [[2, 7]],
    );
    expect(segs.map((s) => s.text).join("")).toBe("let value");
    expect(segs.filter((s) => s.changed).map((s) => s.text).join("")).toBe("t val");
  });

  test("falls back to plain text without tokens", () => {
    expect(segmentsFor(undefined, "abc", undefined)).toEqual([{ text: "abc", changed: false }]);
  });
});

describe("drag clamping", () => {
  const { oldLines, newLines, hunks } = loadSmall();
  const rows: Row[] = buildRows(oldLines, newLines, hunks, { wholeFile: false, expandedGaps: new Set() });

  test("cannot cross a gap row", () => {
    const firstChanged = rows.findIndex((r) => r.type === "line" && (r.right?.kind === "add" || r.left?.kind === "del"));
    const [lo, hi] = clampDrag(rows, "new", firstChanged, rows.length - 1);
    for (let i = lo; i <= hi; i++) expect(rows[i].type).toBe("line");
    expect(hi).toBeLessThan(rows.findIndex((r, i) => i > firstChanged && r.type === "gap"));
  });

  test("range ends on a changed line on the dragged side", () => {
    const addIdx = rows.findIndex((r) => r.type === "line" && r.right?.kind === "add");
    const [lo, hi] = clampDrag(rows, "new", addIdx, addIdx + 2);
    const r = rows[hi];
    expect(r.type === "line" && r.right?.kind === "add").toBe(true);
    expect(rangeFromRows(rows, "new", lo, hi)?.start).toBeLessThanOrEqual(rangeFromRows(rows, "new", lo, hi)?.end ?? 0);
  });

  test("suggestion blocks exist on the new side only, for single and multi-line ranges", () => {
    expect(suggestionBlock({ side: "old", start: 1, end: 3 }, ["a", "b", "c"])).toBe("");
    expect(suggestionBlock({ side: "new", start: 2, end: 2 }, ["a", "b", "c"])).toBe("```suggestion\nb\n```");
    expect(suggestionBlock({ side: "new", start: 1, end: 2 }, ["a", "b", "c"])).toBe("```suggestion\na\nb\n```");
  });
});

describe("buildRows reveal", () => {
  test("keeps revealed lines and their context visible inside a collapsed gap", () => {
    const text = Array.from({ length: 30 }, (_, i) => `l${i + 1}`);
    const rows = buildRows(text, text, [], { wholeFile: false, expandedGaps: new Set(), reveal: new Set([15]) });
    const shown = rows.filter((r) => r.type === "line").map((r) => (r.type === "line" ? r.right!.lineNo : 0));
    expect(shown).toEqual([12, 13, 14, 15, 16, 17, 18]);
    const gaps = rows.filter((r) => r.type === "gap");
    expect(gaps.map((g) => (g.type === "gap" ? [g.newStart, g.count] : []))).toEqual([[1, 11], [19, 12]]);
  });

  test("without reveal the gap is one row", () => {
    const text = ["a", "b", "c"];
    const rows = buildRows(text, text, [], { wholeFile: false, expandedGaps: new Set() });
    expect(rows).toHaveLength(1);
  });
});
