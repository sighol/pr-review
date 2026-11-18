import { isFullyApproved, type Approved } from "./approval";
import { isResolved, type FakeThread, type Resolution } from "./threads";

export interface Readiness {
  unapprovedFiles: number;
  unresolvedThreads: number;
  /** Every file approved through its latest commit and every discussion resolved. */
  ready: boolean;
}

/** Shared by the Review tab's check mark and the confirm box when submitting. Counts the PR conversation too. */
export function readiness(
  files: readonly { path: string; changedBy: readonly number[] }[],
  approvals: ReadonlyMap<string, Approved>,
  threads: readonly FakeThread[],
  resolutions: ReadonlyMap<string, Resolution>,
): Readiness {
  const unapprovedFiles = files.filter((f) => !isFullyApproved(f.changedBy, approvals.get(f.path))).length;
  const unresolvedThreads = threads.filter((t) => !isResolved(t, resolutions.get(t.id))).length;
  return { unapprovedFiles, unresolvedThreads, ready: files.length > 0 && unapprovedFiles === 0 && unresolvedThreads === 0 };
}
