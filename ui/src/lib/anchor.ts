import { diffArrays } from "diff";
import type { FakeThread } from "./threads";
import { threadPlacement, type CommitRange } from "./range";

/**
 * Where a line of the old version ends up in the new one. When the line itself is gone or was
 * rewritten, the first line of its replacement, or the line just before a pure deletion.
 */
export function mapLine(oldLines: string[], newLines: string[], line: number): { line: number; exact: boolean } {
  const clamp = (n: number) => Math.max(1, Math.min(n, newLines.length));
  const parts = diffArrays(oldLines, newLines);
  let oldNo = 1;
  let newNo = 1;
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    if (p.added) {
      newNo += p.count;
    } else if (p.removed) {
      if (line < oldNo + p.count) {
        const next = parts[i + 1];
        if (next?.added) return { line: clamp(newNo + Math.min(line - oldNo, next.count - 1)), exact: false };
        return { line: clamp(newNo - 1), exact: false };
      }
      oldNo += p.count;
    } else {
      if (line < oldNo + p.count) return { line: clamp(newNo + line - oldNo), exact: true };
      oldNo += p.count;
      newNo += p.count;
    }
  }
  return { line: clamp(newLines.length), exact: false };
}

/** An outdated new-side thread that no exact placement shows in this range, so it needs a closest-match line. */
export const needsAnchor = (t: FakeThread, range: CommitRange, changedBy: readonly number[]) =>
  t.file !== null && t.side === "new" && t.line === null && t.commit > 0 && threadPlacement(t, range, changedBy) === null;

/** Thread id -> the line (in the range's new text) it is shown next to. */
export type ThreadAnchors = ReadonlyMap<string, number>;
