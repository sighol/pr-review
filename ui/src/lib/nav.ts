import { placeable } from "./range";
import type { FakeThread } from "./threads";

/** A discussion box on screen, in viewport coordinates. */
export interface Box {
  id: string;
  top: number;
  bottom: number;
}

const lineOf = (t: FakeThread) => t.line ?? t.originalLine;

/**
 * The discussion to go to among those rendered on screen, measured from `centerY` (the middle
 * of the viewport, where jumping puts a discussion): the first one that starts below it (next)
 * or the last one that ends above it (previous). The one you are reading, which spans
 * `centerY`, is neither. `boxes` are in document order.
 */
export function pickRendered(
  boxes: readonly Box[],
  centerY: number,
  dir: 1 | -1,
  keep: (id: string) => boolean,
): string | null {
  const candidates = boxes.filter((b) => keep(b.id));
  if (dir === 1) return candidates.find((b) => b.top > centerY)?.id ?? null;
  return [...candidates].reverse().find((b) => b.bottom < centerY)?.id ?? null;
}

/** Whether a discussion can be navigated to: on a file of the PR, and placeable in some diff. */
export function navigable(t: FakeThread, fileOrder: readonly string[]): t is FakeThread & { file: string } {
  return t.file !== null && fileOrder.includes(t.file) && placeable(t);
}

/**
 * The first (next) or last (previous) discussion of the nearest file after or before `current`,
 * in `fileOrder`, that has one passing `keep`.
 */
export function pickInOtherFile(
  threads: readonly FakeThread[],
  fileOrder: readonly string[],
  current: string,
  dir: 1 | -1,
  keep: (t: FakeThread) => boolean,
): FakeThread | null {
  const index = (t: FakeThread & { file: string }) => fileOrder.indexOf(t.file);
  const here = fileOrder.indexOf(current);
  const candidates = threads
    .filter((t): t is FakeThread & { file: string } => navigable(t, fileOrder) && keep(t))
    .filter((t) => (dir === 1 ? index(t) > here : index(t) < here));
  if (candidates.length === 0) return null;

  const nearest = dir === 1 ? Math.min(...candidates.map(index)) : Math.max(...candidates.map(index));
  const inFile = candidates
    .filter((t) => index(t) === nearest)
    .map((t, order) => ({ t, order }))
    .sort((a, b) => lineOf(a.t) - lineOf(b.t) || a.order - b.order);
  return (dir === 1 ? inFile[0] : inFile[inFile.length - 1]).t;
}
