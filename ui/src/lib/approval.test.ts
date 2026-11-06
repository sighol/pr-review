import { describe, expect, test } from "vitest";
import {
  approveThrough,
  cellStatus,
  diffFrom,
  isFullyApproved,
  lastApproved,
  nextUnapproved,
} from "./approval";

const changedBy = [2, 5, 9];

describe("approval status", () => {
  test("cells: no change, unapproved, approved", () => {
    const approved = new Set([2]);
    expect(cellStatus(changedBy, approved, 1)).toBe("none");
    expect(cellStatus(changedBy, approved, 2)).toBe("approved");
    expect(cellStatus(changedBy, approved, 5)).toBe("unapproved");
    expect(cellStatus(changedBy, undefined, 5)).toBe("unapproved");
  });

  test("last approved ignores commits that did not touch the file", () => {
    expect(lastApproved(changedBy, undefined)).toBeNull();
    expect(lastApproved(changedBy, new Set([2, 5]))).toBe(5);
    expect(lastApproved(changedBy, new Set([3, 4]))).toBeNull();
  });

  test("fully approved needs every touching commit", () => {
    expect(isFullyApproved(changedBy, new Set([2, 5]))).toBe(false);
    expect(isFullyApproved(changedBy, new Set([2, 5, 9]))).toBe(true);
  });
});

describe("diff start point", () => {
  test("nothing approved diffs from the PR base", () => {
    expect(diffFrom(changedBy, undefined, false)).toBe(0);
  });

  test("partially approved diffs from the last approved commit", () => {
    expect(diffFrom(changedBy, new Set([2, 5]), false)).toBe(5);
  });

  test("fully approved, or explicitly full, diffs from the base", () => {
    expect(diffFrom(changedBy, new Set([2, 5, 9]), false)).toBe(0);
    expect(diffFrom(changedBy, new Set([2, 5]), true)).toBe(0);
  });
});

describe("approving", () => {
  test("approves every touching commit up to the latest by default", () => {
    expect([...approveThrough(changedBy)]).toEqual([2, 5, 9]);
  });

  test("approving through a commit stops there", () => {
    expect([...approveThrough(changedBy, 5)]).toEqual([2, 5]);
  });
});

describe("next unapproved file", () => {
  const files = [
    { path: "a", changedBy: [1] },
    { path: "b", changedBy: [2] },
    { path: "c", changedBy: [3] },
  ];

  test("skips approved files and wraps around", () => {
    const approvals = new Map([["b", new Set([2])]]);
    expect(nextUnapproved(files, approvals, "a")).toBe("c");
    expect(nextUnapproved(files, approvals, "c")).toBe("a");
  });

  test("null when everything else is approved", () => {
    const approvals = new Map([
      ["a", new Set([1])],
      ["b", new Set([2])],
    ]);
    expect(nextUnapproved(files, approvals, "c")).toBeNull();
  });
});
