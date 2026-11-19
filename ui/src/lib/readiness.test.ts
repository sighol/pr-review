import { describe, expect, test } from "vitest";
import { readiness, type PendingReview } from "./readiness";
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

  test("an open discussion blocks", () => {
    const open = readiness(files, allApproved, [thread("t1")], new Map());
    expect(open.unresolvedThreads).toBe(1);
    expect(open.ready).toBe(false);
  });

  test("the PR conversation never blocks, since approving resolves it", () => {
    const r = readiness(files, allApproved, [thread("conversation")], new Map());
    expect(r).toEqual({ unapprovedFiles: 0, unresolvedThreads: 0, ready: true });
  });

  const pending = (p: Partial<PendingReview>): PendingReview => ({
    drafts: [],
    replies: new Map(),
    replyResolves: new Set(),
    ...p,
  });

  test("a reply marked resolve counts as resolving its thread; an unmarked one does not", () => {
    const replies = new Map([["t1", "done"]]);
    const marked = pending({ replies, replyResolves: new Set(["t1"]) });
    expect(readiness(files, allApproved, [thread("t1")], new Map(), marked).ready).toBe(true);
    expect(readiness(files, allApproved, [thread("t1")], new Map(), pending({ replies })).ready).toBe(false);
  });

  test("an unmarked reply reopens a thread that is resolved now", () => {
    const resolved = new Map([["t1", resolvedThrough("t1")]]);
    const replies = new Map([["t1", "one more thing"]]);
    expect(readiness(files, allApproved, [thread("t1")], resolved, pending({ replies })).unresolvedThreads).toBe(1);
  });

  test("a blank reply is ignored", () => {
    const resolved = new Map([["t1", resolvedThrough("t1")]]);
    const blank = pending({ replies: new Map([["t1", "  "]]) });
    expect(readiness(files, allApproved, [thread("t1")], resolved, blank).ready).toBe(true);
  });

  test("new-thread drafts block unless marked resolve", () => {
    const open = pending({ drafts: [{ text: "nit", resolve: false }] });
    const done = pending({ drafts: [{ text: "nit", resolve: true }] });
    const empty = pending({ drafts: [{ text: " ", resolve: false }] });
    expect(readiness(files, allApproved, [], new Map(), open).unresolvedThreads).toBe(1);
    expect(readiness(files, allApproved, [], new Map(), done).ready).toBe(true);
    expect(readiness(files, allApproved, [], new Map(), empty).ready).toBe(true);
  });

  test("a PR without files is never ready", () => {
    expect(readiness([], new Map(), [], new Map()).ready).toBe(false);
  });
});
