import { describe, expect, test } from "vitest";
import { mapLine, needsAnchor } from "./anchor";
import type { FakeThread } from "./threads";

const lines = (s: string) => s.split("");

describe("mapLine", () => {
  test("an unchanged line follows insertions and deletions before it", () => {
    expect(mapLine(lines("abcd"), lines("xyabcd"), 3)).toEqual({ line: 5, exact: true });
    expect(mapLine(lines("abcd"), lines("bcd"), 4)).toEqual({ line: 3, exact: true });
  });

  test("a rewritten line maps to its replacement", () => {
    expect(mapLine(lines("abcd"), lines("aXd"), 2)).toEqual({ line: 2, exact: false });
    expect(mapLine(lines("abcd"), lines("aXYd"), 3)).toEqual({ line: 3, exact: false });
  });

  test("a deleted line maps to the line before the deletion", () => {
    expect(mapLine(lines("abcd"), lines("ad"), 3)).toEqual({ line: 1, exact: false });
    expect(mapLine(lines("abcd"), lines("cd"), 1)).toEqual({ line: 1, exact: false });
  });

  test("stays inside the new text", () => {
    expect(mapLine(lines("abcd"), lines("ab"), 4)).toEqual({ line: 2, exact: false });
  });
});

describe("needsAnchor", () => {
  const t = (over: Partial<FakeThread>): FakeThread => ({
    id: "t",
    file: "f",
    side: "new",
    line: null,
    originalLine: 5,
    commit: 2,
    githubResolved: false,
    comments: [],
    ...over,
  });
  const changedBy = [1, 2, 3];

  test("outdated new-side threads whose commit differs from the range end", () => {
    expect(needsAnchor(t({}), { from: 0, to: 3 }, changedBy)).toBe(true);
    expect(needsAnchor(t({}), { from: 0, to: 2 }, changedBy)).toBe(false);
  });

  test("not for current, old-side or force-pushed-away threads", () => {
    expect(needsAnchor(t({ line: 5 }), { from: 0, to: 1 }, changedBy)).toBe(false);
    expect(needsAnchor(t({ side: "old" }), { from: 0, to: 3 }, changedBy)).toBe(false);
    expect(needsAnchor(t({ commit: 0 }), { from: 0, to: 3 }, changedBy)).toBe(false);
  });
});
