import { placeable } from "./range";
import type { FakeThread } from "./threads";

const lineOf = (t: FakeThread) => t.line ?? t.originalLine;

/** Whether a discussion can be navigated to: on a file of the PR, and placeable in some diff. */
export function navigable(t: FakeThread, fileOrder: readonly string[]): t is FakeThread & { file: string } {
  return t.file !== null && fileOrder.includes(t.file) && placeable(t);
}

/** The navigable discussions in reading order: by file (in `fileOrder`), then line, then as given. */
export function inReadingOrder(threads: readonly FakeThread[], fileOrder: readonly string[]): FakeThread[] {
  return threads
    .filter((t): t is FakeThread & { file: string } => navigable(t, fileOrder))
    .map((t, order) => ({ t, order, file: fileOrder.indexOf(t.file) }))
    .sort((a, b) => a.file - b.file || lineOf(a.t) - lineOf(b.t) || a.order - b.order)
    .map((x) => x.t);
}

/**
 * The next (dir 1) or previous (dir -1) discussion passing `keep`, stepping from the one last
 * jumped to. If there is none, or it is on another file than the one being viewed, it steps from
 * the start of the viewed file. Does not wrap: null at either end.
 */
export function pickNeighbor(
  threads: readonly FakeThread[],
  fileOrder: readonly string[],
  currentFile: string,
  lastId: string | null,
  dir: 1 | -1,
  keep: (t: FakeThread) => boolean,
): FakeThread | null {
  const ordered = inReadingOrder(threads, fileOrder);
  const last = ordered.findIndex((t) => t.id === lastId && t.file === currentFile);
  if (last >= 0) {
    const rest = dir === 1 ? ordered.slice(last + 1) : ordered.slice(0, last).reverse();
    return rest.find(keep) ?? null;
  }

  const here = fileOrder.indexOf(currentFile);
  const fileIndex = (t: FakeThread) => fileOrder.indexOf(t.file!);
  if (dir === 1) return ordered.find((t) => fileIndex(t) >= here && keep(t)) ?? null;
  return [...ordered].reverse().find((t) => fileIndex(t) < here && keep(t)) ?? null;
}
