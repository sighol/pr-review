import { expect, test } from "vitest";
import { inReadingOrder, navigable, pickNeighbor } from "./nav";
import type { FakeThread } from "./threads";

const thread = (id: string, file: string | null, line: number | null, over: Partial<FakeThread> = {}): FakeThread => ({
  id,
  file,
  side: "new",
  line,
  originalLine: line ?? 7,
  commit: 3,
  githubResolved: false,
  comments: [{ id: `${id}-c`, author: "a", when: "now", body: "x" }],
  ...over,
});

const order = ["a.rs", "b.rs", "c.rs"];
const all = () => true;
const id = (t: FakeThread | null) => t?.id ?? null;

// Reading order: a1 a2 | b1 b2 | c1 (given out of order on purpose).
const threads = [
  thread("c1", "c.rs", 5),
  thread("b2", "b.rs", 90),
  thread("a2", "a.rs", 20),
  thread("b1", "b.rs", 5),
  thread("a1", "a.rs", 10),
];

test("reading order is by file, then line", () => {
  expect(inReadingOrder(threads, order).map((t) => t.id)).toEqual(["a1", "a2", "b1", "b2", "c1"]);
});

test("same-line discussions keep their given order", () => {
  const same = [thread("x", "a.rs", 5), thread("y", "a.rs", 5)];
  expect(inReadingOrder(same, order).map((t) => t.id)).toEqual(["x", "y"]);
});

test("next and previous step from the last discussion", () => {
  expect(id(pickNeighbor(threads, order, "a.rs", "a1", 1, all))).toBe("a2");
  expect(id(pickNeighbor(threads, order, "a.rs", "a2", 1, all))).toBe("b1");
  expect(id(pickNeighbor(threads, order, "b.rs", "b1", -1, all))).toBe("a2");
  expect(id(pickNeighbor(threads, order, "b.rs", "b2", -1, all))).toBe("b1");
});

test("no wrap-around at either end", () => {
  expect(pickNeighbor(threads, order, "c.rs", "c1", 1, all)).toBeNull();
  expect(pickNeighbor(threads, order, "a.rs", "a1", -1, all)).toBeNull();
});

test("the filter skips discussions but stepping still starts at the last one", () => {
  const skipB = (t: FakeThread) => t.file !== "b.rs";
  expect(id(pickNeighbor(threads, order, "a.rs", "a2", 1, skipB))).toBe("c1");
  expect(id(pickNeighbor(threads, order, "c.rs", "c1", -1, skipB))).toBe("a2");
  expect(pickNeighbor(threads, order, "a.rs", "a2", 1, (t) => t.id === "a1")).toBeNull();
});

test("the last one may itself be filtered out (e.g. resolved)", () => {
  const notA2 = (t: FakeThread) => t.id !== "a2";
  expect(id(pickNeighbor(threads, order, "a.rs", "a2", 1, notA2))).toBe("b1");
  expect(id(pickNeighbor(threads, order, "a.rs", "a2", -1, notA2))).toBe("a1");
});

test("with nothing jumped to yet, it steps from the start of the viewed file", () => {
  expect(id(pickNeighbor(threads, order, "b.rs", null, 1, all))).toBe("b1");
  expect(id(pickNeighbor(threads, order, "b.rs", null, -1, all))).toBe("a2");
  expect(id(pickNeighbor(threads, order, "a.rs", null, 1, all))).toBe("a1");
  expect(pickNeighbor(threads, order, "a.rs", null, -1, all)).toBeNull();
});

test("a last discussion on another file than the viewed one is ignored", () => {
  // Jumped to c1, then picked b.rs in the file selector.
  expect(id(pickNeighbor(threads, order, "b.rs", "c1", 1, all))).toBe("b1");
  expect(id(pickNeighbor(threads, order, "b.rs", "c1", -1, all))).toBe("a2");
});

test("a last discussion that no longer exists is ignored", () => {
  expect(id(pickNeighbor(threads, order, "b.rs", "gone", 1, all))).toBe("b1");
});

test("conversation, unknown files and unplaceable threads are not navigable", () => {
  expect(navigable(thread("conv", null, 0), order)).toBe(false);
  expect(navigable(thread("gone", "z.rs", 3), order)).toBe(false);
  // Outdated on the new side and made on a commit that is not in the PR.
  expect(navigable(thread("orphan", "a.rs", null, { commit: 0 }), order)).toBe(false);
  expect(navigable(thread("ok", "a.rs", 3), order)).toBe(true);

  const odd = [thread("orphan", "b.rs", null, { commit: 0 }), thread("conv", null, 0)];
  expect(pickNeighbor(odd, order, "a.rs", null, 1, all)).toBeNull();
});
