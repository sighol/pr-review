import { describe, expect, test } from "vitest";
import {
  asOfRange,
  commentable,
  defaultRange,
  rangeForSide,
  rangeForThread,
  rangeFromColumns,
  rangeLabel,
  stateOf,
  threadPlacement,
} from "./range";

const changedBy = [3, 7, 12];

describe("ranges", () => {
  test("selecting columns a..b means changes introduced by those commits", () => {
    expect(rangeFromColumns(4, 9)).toEqual({ from: 3, to: 9 });
    expect(rangeFromColumns(9, 4)).toEqual({ from: 3, to: 9 });
    expect(rangeFromColumns(1, 5)).toEqual({ from: 0, to: 5 });
    expect(rangeFromColumns(6, 6)).toEqual({ from: 5, to: 6 });
  });

  test("labels", () => {
    expect(rangeLabel({ from: 0, to: 9 })).toBe("base..#9");
    expect(rangeLabel({ from: 3, to: 9 })).toBe("#3..#9");
  });

  test("a commit that didn't touch the file is the same state as the previous touching one", () => {
    expect(stateOf(changedBy, 2)).toBe(0);
    expect(stateOf(changedBy, 3)).toBe(3);
    expect(stateOf(changedBy, 6)).toBe(3);
    expect(stateOf(changedBy, 18)).toBe(12);
  });

  test("default: since last approval, or the whole PR when nothing or all approved", () => {
    expect(defaultRange(changedBy, undefined, 18)).toEqual({ from: 0, to: 18 });
    expect(defaultRange(changedBy, new Set([3, 7]), 18)).toEqual({ from: 7, to: 18 });
    expect(defaultRange(changedBy, new Set([3, 7, 12]), 18)).toEqual({ from: 0, to: 18 });
  });
});

describe("which gutters take comments", () => {
  test("left needs the PR base, right needs the file's latest version", () => {
    expect(commentable({ from: 0, to: 18 }, changedBy)).toEqual({ left: true, right: true });
    expect(commentable({ from: 7, to: 18 }, changedBy)).toEqual({ left: false, right: true });
    expect(commentable({ from: 0, to: 9 }, changedBy)).toEqual({ left: true, right: false });
    expect(commentable({ from: 0, to: 12 }, changedBy)).toEqual({ left: true, right: true });
  });
});

describe("thread placement", () => {
  const current = { side: "new" as const, line: 73, originalLine: 63, commit: 7 };
  const outdated = { side: "new" as const, line: null, originalLine: 96, commit: 7 };
  const oldSide = { side: "old" as const, line: 13, originalLine: 13, commit: 1 };

  test("new-side thread: current line when the range ends at the latest version", () => {
    expect(threadPlacement(current, { from: 0, to: 18 }, changedBy)).toBe(73);
    expect(threadPlacement(current, { from: 7, to: 18 }, changedBy)).toBe(73);
  });

  test("new-side thread: original line only when the range ends exactly at its commit", () => {
    expect(threadPlacement(current, { from: 0, to: 7 }, changedBy)).toBe(63);
    expect(threadPlacement(current, { from: 0, to: 8 }, changedBy)).toBeNull();
    expect(threadPlacement(current, { from: 0, to: 6 }, changedBy)).toBeNull();
  });

  test("outdated thread only shows at its own commit", () => {
    expect(threadPlacement(outdated, { from: 0, to: 18 }, changedBy)).toBeNull();
    expect(threadPlacement(outdated, { from: 2, to: 7 }, changedBy)).toBe(96);
  });

  test("old-side thread needs a range starting at the base", () => {
    expect(threadPlacement(oldSide, { from: 0, to: 5 }, changedBy)).toBe(13);
    expect(threadPlacement(oldSide, { from: 1, to: 18 }, changedBy)).toBeNull();
  });
});

describe("ranges that reveal a thread", () => {
  test("new-side current thread: keep from, extend to the head", () => {
    const t = { side: "new" as const, line: 73, originalLine: 63, commit: 7 };
    expect(rangeForThread(t, { from: 4, to: 9 }, 18)).toEqual({ from: 4, to: 18 });
  });

  test("outdated thread: base to its commit", () => {
    const t = { side: "new" as const, line: null, originalLine: 96, commit: 7 };
    expect(rangeForThread(t, { from: 4, to: 18 }, 18)).toEqual({ from: 0, to: 7 });
  });

  test("old-side thread: start at the base, keep to", () => {
    const t = { side: "old" as const, line: 13, originalLine: 13, commit: 1 };
    expect(rangeForThread(t, { from: 4, to: 9 }, 18)).toEqual({ from: 0, to: 9 });
  });

  test("as-of range is base..comment commit", () => {
    expect(asOfRange({ side: "new", line: 73, originalLine: 63, commit: 7 })).toEqual({ from: 0, to: 7 });
  });

  test("range for a side", () => {
    expect(rangeForSide("old", { from: 4, to: 9 }, 18)).toEqual({ from: 0, to: 9 });
    expect(rangeForSide("new", { from: 4, to: 9 }, 18)).toEqual({ from: 4, to: 18 });
  });
});
