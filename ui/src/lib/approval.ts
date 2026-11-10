export type CellStatus = "none" | "unapproved" | "approved";

/** Commit numbers (1-based) approved for a file; absent means nothing approved. */
export type Approved = ReadonlySet<number> | undefined;

export function cellStatus(changedBy: readonly number[], approved: Approved, commit: number): CellStatus {
  if (!changedBy.includes(commit)) return "none";
  return approved?.has(commit) ? "approved" : "unapproved";
}

/** Highest approved commit that actually touched the file. */
export function lastApproved(changedBy: readonly number[], approved: Approved): number | null {
  const hits = changedBy.filter((c) => approved?.has(c));
  return hits.length ? Math.max(...hits) : null;
}

export function isFullyApproved(changedBy: readonly number[], approved: Approved): boolean {
  return changedBy.every((c) => approved?.has(c));
}

export const latestCommit = (changedBy: readonly number[]) => Math.max(...changedBy);

/**
 * Start point for the diff: the last approved commit, or 0 (the PR base).
 * A fully approved file, or an explicit request, shows the whole PR diff.
 */
export function diffFrom(changedBy: readonly number[], approved: Approved, fullDiff: boolean): number {
  if (fullDiff || isFullyApproved(changedBy, approved)) return 0;
  return lastApproved(changedBy, approved) ?? 0;
}

/** Approving "through" a commit approves every commit that touched the file up to it. */
export function approveThrough(changedBy: readonly number[], through: number = latestCommit(changedBy)): Set<number> {
  return new Set(changedBy.filter((c) => c <= through));
}

/**
 * The next file (wrapping) after `current` that still has unapproved commits, in `order`
 * (the file-tree order the UI displays, not necessarily `files`' own order).
 */
export function nextUnapproved(
  files: readonly { path: string; changedBy: readonly number[] }[],
  approvals: ReadonlyMap<string, ReadonlySet<number>>,
  current: string,
  order: readonly string[],
): string | null {
  const byPath = new Map(files.map((f) => [f.path, f]));
  const start = order.indexOf(current);
  for (let i = 1; i <= order.length; i++) {
    const path = order[(start + i) % order.length];
    const f = byPath.get(path);
    if (f && f.path !== current && !isFullyApproved(f.changedBy, approvals.get(f.path))) return f.path;
  }
  return null;
}
