import { expect, test } from "vitest";
import { navigable, pickInOtherFile, pickRendered, type Box } from "./nav";
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

const boxes: Box[] = [
  { id: "a", top: 0, bottom: 80 },
  { id: "b", top: 200, bottom: 320 },
  { id: "c", top: 600, bottom: 700 },
];

test("next is the first box starting below the center, previous the last one ending above it", () => {
  expect(pickRendered(boxes, 150, 1, () => true)).toBe("b");
  expect(pickRendered(boxes, 150, -1, () => true)).toBe("a");
});

test("the box spanning the center is neither next nor previous", () => {
  // Jumping centers a box, so it must not be picked again.
  expect(pickRendered(boxes, 260, 1, () => true)).toBe("c");
  expect(pickRendered(boxes, 260, -1, () => true)).toBe("a");
});

test("nothing left in that direction", () => {
  expect(pickRendered(boxes, 650, 1, () => true)).toBeNull();
  expect(pickRendered(boxes, 40, -1, () => true)).toBeNull();
  expect(pickRendered([], 100, 1, () => true)).toBeNull();
});

test("the filter skips boxes", () => {
  expect(pickRendered(boxes, 150, 1, (id) => id !== "b")).toBe("c");
  expect(pickRendered(boxes, 650, -1, (id) => id === "a")).toBe("a");
});

const order = ["a.rs", "b.rs", "c.rs", "d.rs"];

test("other file: the first discussion of the next file with one, the last of the previous", () => {
  const threads = [
    thread("b-late", "b.rs", 90),
    thread("b-early", "b.rs", 5),
    thread("d1", "d.rs", 1),
    thread("a1", "a.rs", 10),
    thread("a2", "a.rs", 20),
  ];
  expect(pickInOtherFile(threads, order, "a.rs", 1, () => true)?.id).toBe("b-early");
  expect(pickInOtherFile(threads, order, "b.rs", 1, () => true)?.id).toBe("d1");
  expect(pickInOtherFile(threads, order, "b.rs", -1, () => true)?.id).toBe("a2");
  expect(pickInOtherFile(threads, order, "d.rs", -1, () => true)?.id).toBe("b-late");
});

test("other file: ends of the list, and the filter", () => {
  const threads = [thread("a1", "a.rs", 10), thread("c1", "c.rs", 4, { githubResolved: true })];
  expect(pickInOtherFile(threads, order, "c.rs", 1, () => true)).toBeNull();
  expect(pickInOtherFile(threads, order, "a.rs", -1, () => true)).toBeNull();
  expect(pickInOtherFile(threads, order, "a.rs", 1, (t) => t.id !== "c1")).toBeNull();
});

test("other file: same-line discussions keep their order", () => {
  const threads = [thread("x", "b.rs", 5), thread("y", "b.rs", 5)];
  expect(pickInOtherFile(threads, order, "a.rs", 1, () => true)?.id).toBe("x");
  expect(pickInOtherFile(threads, order, "c.rs", -1, () => true)?.id).toBe("y");
});

test("conversation, unknown files and unplaceable threads are not navigable", () => {
  expect(navigable(thread("conv", null, 0), order)).toBe(false);
  expect(navigable(thread("gone", "z.rs", 3), order)).toBe(false);
  // Outdated on the new side and made on a commit that is not in the PR.
  expect(navigable(thread("orphan", "a.rs", null, { commit: 0 }), order)).toBe(false);
  expect(navigable(thread("ok", "a.rs", 3), order)).toBe(true);

  const threads = [thread("orphan", "b.rs", null, { commit: 0 }), thread("conv", null, 0)];
  expect(pickInOtherFile(threads, order, "a.rs", 1, () => true)).toBeNull();
});
