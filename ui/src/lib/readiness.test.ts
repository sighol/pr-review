import { describe, expect, test } from "vitest";
import { readiness } from "./readiness";
import type { FakeThread, Resolution } from "./threads";

const files = [
  { path: "a.rs", changedBy: [1, 2] },
  { path: "b.rs", changedBy: [2] },
];
const allApproved = new Map([
  ["a.rs", new Set([1, 2])],
  ["b.rs", new Set([2])],
]);
const thread = (id: string): FakeThread => ({ id, file: "a.rs", comments: [{ id: `${id}-c`, author: "x", body: "hi" }] }) as unknown as FakeThread;
const resolvedThrough = (id: string): Resolution => ({ through: `${id}-c`, reopened: false });

describe("readiness", () => {
  test("ready when everything is approved and resolved", () => {
    const r = readiness(files, allApproved, [thread("t1")], new Map([["t1", resolvedThrough("t1")]]));
    expect(r).toEqual({ unapprovedFiles: 0, unresolvedThreads: 0, ready: true });
  });

  test("an unapproved commit blocks", () => {
    const r = readiness(files, new Map([["a.rs", new Set([1])], ["b.rs", new Set([2])]]), [], new Map());
    expect(r.unapprovedFiles).toBe(1);
    expect(r.ready).toBe(false);
  });

  test("an open discussion blocks, including the PR conversation", () => {
    expect(readiness(files, allApproved, [thread("t1")], new Map()).ready).toBe(false);
    const open = readiness(files, allApproved, [thread("conversation")], new Map());
    expect(open.unresolvedThreads).toBe(1);
    expect(open.ready).toBe(false);
    expect(readiness(files, allApproved, [thread("conversation")], new Map([["conversation", resolvedThrough("conversation")]])).ready).toBe(true);
  });

  test("a PR without files is never ready", () => {
    expect(readiness([], new Map(), [], new Map()).ready).toBe(false);
  });
});
