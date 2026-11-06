import { describe, expect, test } from "vitest";
import { firstNewIndex, isResolved, type FakeThread } from "./threads";

const comment = (id: string) => ({ id, author: "a", when: "now", body: "" });
const thread = (ids: string[]): FakeThread => ({
  id: "t",
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
