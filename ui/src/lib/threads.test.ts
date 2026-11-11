import { describe, expect, test } from "vitest";
import { CONVERSATION_ID, firstNewIndex, isResolved, mergeForcePushes, replacedLines, type FakeThread } from "./threads";

const comment = (id: string) => ({ id, author: "a", when: "now", body: "" });
const thread = (ids: string[]): FakeThread => ({
  id: "t",
  file: null,
  side: "new",
  line: 1,
  githubResolved: false,
  comments: ids.map(comment),
});

describe("local thread resolution", () => {
  test("never-resolved thread is open with nothing marked new", () => {
    const t = thread(["c1", "c2"]);
    expect(isResolved(t, undefined)).toBe(false);
    expect(firstNewIndex(t, undefined)).toBeNull();
  });

  test("resolved through the last comment is resolved, nothing new", () => {
    const t = thread(["c1", "c2"]);
    const r = { through: "c2", reopened: false };
    expect(isResolved(t, r)).toBe(true);
    expect(firstNewIndex(t, r)).toBeNull();
  });

  test("a new comment after resolving reopens the thread and marks it new", () => {
    const t = thread(["c1", "c2", "c3", "c4"]);
    const r = { through: "c2", reopened: false };
    expect(isResolved(t, r)).toBe(false);
    expect(firstNewIndex(t, r)).toBe(2);
  });

  test("manual reopen keeps the marker for the divider", () => {
    const t = thread(["c1", "c2"]);
    const r = { through: "c2", reopened: true };
    expect(isResolved(t, r)).toBe(false);
    expect(firstNewIndex(t, r)).toBeNull();
  });
});

describe("lines a suggestion replaces", () => {
  const text = "l1\nl2\nl3\nl4\nl5\n";

  test("a multi-line thread covers its whole range", () => {
    expect(replacedLines({ side: "new", line: 4, startLine: 2 }, text)).toEqual(["l2", "l3", "l4"]);
  });

  test("a single-line thread covers its line, with or without a start", () => {
    expect(replacedLines({ side: "new", line: 3, startLine: null }, text)).toEqual(["l3"]);
    expect(replacedLines({ side: "new", line: 3 }, text)).toEqual(["l3"]);
    expect(replacedLines({ side: "new", line: 3, startLine: 3 }, text)).toEqual(["l3"]);
  });

  test("old-side and outdated threads replace nothing", () => {
    expect(replacedLines({ side: "old", line: 3, startLine: 2 }, text)).toBeNull();
    expect(replacedLines({ side: "new", line: null, startLine: null }, text)).toBeNull();
  });

  test("lines past the end of the file give no lines", () => {
    expect(replacedLines({ side: "new", line: 40, startLine: 38 }, text)).toEqual([]);
  });
});

describe("merging detected force-pushes into the conversation", () => {
  const conversation = (ids: string[]): FakeThread => ({ ...thread(ids), id: CONVERSATION_ID, file: null });
  const fp = (oldOid: string, newOid: string, detectedAt: string) => ({ oldOid, newOid, detectedAt });

  test("no force-pushes leaves the threads untouched", () => {
    const threads = [conversation(["c1"])];
    expect(mergeForcePushes(threads, [], 0)).toBe(threads);
  });

  test("a force-push is appended as a no-author system comment on the conversation", () => {
    const threads = [conversation(["c1"])];
    const merged = mergeForcePushes(threads, [fp("aaaaaaa1", "bbbbbbb2", "2026-01-01T00:00:00.000Z")], 0);
    const convo = merged.find((t) => t.id === CONVERSATION_ID)!;
    expect(convo.comments.map((c) => c.id)).toEqual(["c1", "force-push-bbbbbbb2"]);
    const synthetic = convo.comments[1]!;
    expect(synthetic.system).toBe(true);
    expect(synthetic.author).toBe("");
    expect(synthetic.body).toContain("aaaaaaa");
    expect(synthetic.body).toContain("bbbbbbb");
  });

  test("force-pushes interleave with real comments by time", () => {
    const early = { ...conversation(["c1"]).comments[0]!, createdAt: "2026-01-01T00:00:00.000Z" };
    const late = { id: "c2", author: "a", when: "now", createdAt: "2026-03-01T00:00:00.000Z", body: "" };
    const threads = [{ ...conversation([]), comments: [early, late] }];
    const merged = mergeForcePushes(threads, [fp("a", "b", "2026-02-01T00:00:00.000Z")], 0);
    expect(merged[0]!.comments.map((c) => c.id)).toEqual(["c1", "force-push-b", "c2"]);
  });

  test("synthesizes the conversation thread when the PR has no real comments", () => {
    const threads: FakeThread[] = [thread(["c1"])];
    const merged = mergeForcePushes(threads, [fp("a", "b", "2026-01-01T00:00:00.000Z")], 0);
    expect(merged).toHaveLength(2);
    const convo = merged.find((t) => t.id === CONVERSATION_ID)!;
    expect(convo.file).toBeNull();
    expect(convo.comments.map((c) => c.id)).toEqual(["force-push-b"]);
  });
});
