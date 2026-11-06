import type { Side } from "./diff";
import { lastApproved, isFullyApproved } from "./approval";

/**
 * A commit range. `from` is the state after commit `from` (0 is the PR base),
 * `to` the state after commit `to`. Selecting commits a..b in the UI means
 * `{ from: a - 1, to: b }`: "changes introduced by commits a through b".
 */
export interface CommitRange {
  from: number;
  to: number;
}

export const sameRange = (a: CommitRange, b: CommitRange) => a.from === b.from && a.to === b.to;

export const rangeFromColumns = (a: number, b: number): CommitRange => ({
  from: Math.min(a, b) - 1,
  to: Math.max(a, b),
});

export function rangeLabel(r: CommitRange): string {
  return `${r.from === 0 ? "base" : `#${r.from}`}..#${r.to}`;
}

/** The file's content state after commit k: 0 (base) or the latest touching commit at or before k. */
export function stateOf(changedBy: readonly number[], k: number): number {
  const touching = changedBy.filter((c) => c <= k);
  return touching.length ? Math.max(...touching) : 0;
}

/** Since-approval by default; the full PR diff when nothing (or everything) is approved. */
export function defaultRange(
  changedBy: readonly number[],
  approved: ReadonlySet<number> | undefined,
  headN: number,
): CommitRange {
  if (isFullyApproved(changedBy, approved)) return { from: 0, to: headN };
  return { from: lastApproved(changedBy, approved) ?? 0, to: headN };
}

/** Which gutters can take comments: GitHub anchors them to the PR base (left) and head (right). */
export function commentable(range: CommitRange, changedBy: readonly number[]): { left: boolean; right: boolean } {
  return { left: range.from === 0, right: range.to >= Math.max(...changedBy) };
}

export interface ThreadAnchor {
  side: Side;
  /** Current line, or null when the thread is outdated. */
  line: number | null;
  /** The line in the version the reviewer saw (at `commit`). */
  originalLine: number;
  /** The commit the thread was made on. */
  commit: number;
}

/** The line to show a thread at in this range, or null when it has no valid place. */
export function threadPlacement(t: ThreadAnchor, range: CommitRange, changedBy: readonly number[]): number | null {
  if (t.side === "old") return range.from === 0 ? (t.line ?? t.originalLine) : null;
  if (t.line !== null && range.to >= Math.max(...changedBy)) return t.line;
  if (range.to === t.commit) return t.originalLine;
  return null;
}

/**
 * False for a thread that no commit range can show: outdated on the new side, and made on a
 * commit that is not in the PR (commit 0), so there is no version of the file to anchor it to.
 */
export const placeable = (t: ThreadAnchor) => t.side === "old" || t.line !== null || t.commit > 0;

/** A range that shows the thread, changing as little of the current range as possible. */
export function rangeForThread(t: ThreadAnchor, current: CommitRange, headN: number): CommitRange {
  if (t.side === "old") return { from: 0, to: current.to };
  if (t.line !== null) return { from: current.from, to: headN };
  return { from: 0, to: t.commit };
}

/** The range a reviewer saw when making the comment: PR base to the comment's commit. */
export const asOfRange = (t: ThreadAnchor): CommitRange => ({ from: 0, to: t.commit });

/** Adjusts a range so the given side can take comments (used when jumping to a draft). */
export function rangeForSide(side: Side, current: CommitRange, headN: number): CommitRange {
  return side === "old" ? { from: 0, to: current.to } : { from: current.from, to: headN };
}
