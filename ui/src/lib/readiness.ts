import { isFullyApproved, type Approved } from "./approval";
import { CONVERSATION_ID, isResolved, type FakeThread, type Resolution } from "./threads";

export interface Readiness {
  unapprovedFiles: number;
  unresolvedThreads: number;
  /** Every file approved through its latest commit and every discussion resolved. */
  ready: boolean;
}

/** The unsent review, as far as it decides whether discussions end up resolved. */
export interface PendingReview {
  drafts: readonly { text: string; resolve: boolean }[];
  replies: ReadonlyMap<string, string>;
  replyResolves: ReadonlySet<string>;
}

const NOTHING_PENDING: PendingReview = { drafts: [], replies: new Map(), replyResolves: new Set() };

/**
 * Shared by the Review tab's check mark and the confirm box when submitting. Counts what the
 * review will leave open once sent: a thread with a pending reply is resolved only if that reply
 * is marked "resolve" (an unmarked reply reopens even a resolved thread), and a new-thread draft
 * is open unless marked "resolve". The PR conversation is not counted: approving resolves it.
 */
export function readiness(
  files: readonly { path: string; changedBy: readonly number[] }[],
  approvals: ReadonlyMap<string, Approved>,
  threads: readonly FakeThread[],
  resolutions: ReadonlyMap<string, Resolution>,
  pending: PendingReview = NOTHING_PENDING,
): Readiness {
  const unapprovedFiles = files.filter((f) => !isFullyApproved(f.changedBy, approvals.get(f.path))).length;
  const openThreads = threads.filter((t) => {
    if (t.id === CONVERSATION_ID) return false;
    if ((pending.replies.get(t.id) ?? "").trim() !== "") return !pending.replyResolves.has(t.id);
    return !isResolved(t, resolutions.get(t.id));
  }).length;
  const openDrafts = pending.drafts.filter((d) => d.text.trim() !== "" && !d.resolve).length;
  const unresolvedThreads = openThreads + openDrafts;
  return { unapprovedFiles, unresolvedThreads, ready: files.length > 0 && unapprovedFiles === 0 && unresolvedThreads === 0 };
}
