import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { approveThrough } from "./lib/approval";
import type { LineRange, Side } from "./lib/diff";
import { needsAnchor } from "./lib/anchor";
import {
  asOfRange,
  commentable,
  defaultRange,
  placeable,
  rangeForSide,
  rangeForThread,
  threadPlacement,
  type CommitRange,
} from "./lib/range";
import { parseLine, parseRoute, routeUrl, type Route, type Tab } from "./lib/route";
import {
  persist,
  submitReview as sendReview,
  type ReviewEvent,
  type SavedState,
  type SubmitResult,
} from "./lib/backend";
import { formatDirectives, type Directives } from "./lib/directives";
import {
  CONVERSATION_ID,
  lastCommentId,
  mergeForcePushes,
  threadFromJson,
  type FakeThread,
  type PrInfo,
  type Resolution,
} from "./lib/threads";

export type { Tab } from "./lib/route";

export interface Commit {
  n: number;
  oid: string;
  short: string;
  subject: string;
  /** What "state 0" (the PR base) means for a range ending at this commit; see `backend.ts`. */
  floor: string;
}

export interface FileData {
  /** Same as `path`; the key used in URLs and drafts. */
  name: string;
  path: string;
  /** The file's path at the PR base, when it was renamed in the PR. */
  oldPath?: string;
  /** Directory the file's states and diffs are stored under. */
  dir: string;
  added: number;
  removed: number;
  /** Commit numbers (1-based) that touched this file. */
  changedBy: number[];
  /** The file at its latest commit (the PR head). */
  newText: string;
}

/** Seed approvals for the prototype: file path -> approved through this commit. */
const SEED_APPROVED_THROUGH: Record<string, number> = {
  "src/cli.rs": 17,
  "src/main.rs": 11,
  "src/threads.rs": 7,
  "src/github.rs": 7,
  "src/interactive.rs": 9,
};

export interface Draft {
  key: string;
  file: string;
  range: LineRange;
  text: string;
  /** The file changed since this was written, so its line may be wrong; blocks submitting until acknowledged. */
  stale: boolean;
}

export interface JumpTarget {
  file: string;
  side: Side;
  line: number;
  /** When set, scroll to and flash this thread instead of the code line. */
  threadId?: string;
  tick: number;
}

export const draftKey = (file: string, r: LineRange) => `${file}|${r.side}:${r.start}-${r.end}`;

/** The line a thread is shown at when its placement doesn't matter (indexes, labels). */
export const threadLine = (t: FakeThread) => t.line ?? t.originalLine;

interface Store {
  files: FileData[];
  fileByName: ReadonlyMap<string, FileData>;
  commits: Commit[];
  threads: FakeThread[];
  pr: PrInfo;
  /** Re-fetches the whole PR, the same as closing and reopening the app. No-op in a browser. */
  refresh: () => void;
  refreshing: boolean;
  refreshError: string | null;

  /** File path -> approved commit numbers. */
  approvals: ReadonlyMap<string, ReadonlySet<number>>;
  /** Approves the commits touching the file up to the commit `throughOid` (default: all of them). */
  approveFile: (file: FileData, throughOid?: string) => void;
  unapproveFile: (file: FileData) => void;

  /** The commit range a file is currently viewed at. */
  rangeFor: (file: FileData) => CommitRange;
  setRange: (path: string, range: CommitRange) => void;
  /** Drops a file's own range so it follows the all-files range, or since-approval. */
  resetRange: (path: string) => void;
  globalRange: CommitRange | null;
  /** Sets one range for every file (null clears it). */
  setAllRanges: (range: CommitRange | null) => void;
  /** Shows the file as the reviewer saw it when commenting: PR base to the comment's commit. */
  viewAsOf: (thread: FakeThread) => void;

  tab: Tab;
  setTab: (t: Tab) => void;
  currentFile: string;
  setCurrentFile: (name: string) => void;

  drafts: ReadonlyMap<string, Draft>;
  focus: { key: string; tick: number } | null;
  createDraft: (file: string, range: LineRange) => void;
  patchDraft: (key: string, text: string) => void;
  removeDraft: (key: string) => void;
  /** "The line is still right": clears a draft's stale flag. */
  acknowledgeDraft: (key: string) => void;
  /**
   * Sends the saved review to GitHub and, on success, clears drafts, replies and the review body
   * and swaps in the refreshed threads. `text` is the review body without its directives.
   * If a label or assignee change failed, the body becomes the directives still to apply.
   * Resolves to null in a browser, where nothing is sent.
   */
  submitReview: (
    event: ReviewEvent,
    text: string,
    directives: Directives,
    onProgress?: (message: string) => void,
  ) => Promise<SubmitResult | null>;

  /** Reply drafts by thread id. "" means an open but empty box. */
  replies: ReadonlyMap<string, string>;
  setReply: (threadId: string, text: string | null) => void;

  resolutions: ReadonlyMap<string, Resolution>;
  resolveThread: (t: FakeThread) => void;
  reopenThread: (t: FakeThread) => void;

  reviewBody: string;
  setReviewBody: (s: string) => void;

  jump: JumpTarget | null;
  /** The discussion last jumped to (by next/previous, a Summary click or a thread link), if any. */
  lastThreadId: string | null;
  jumpTo: (file: string, side: Side, line: number, threadId?: string) => void;
  clearJump: () => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("useStore outside StoreProvider");
  return s;
}

/** Empty boxes don't count as pending. */
export const isRealDraft = (d: Draft) => d.text.trim() !== "";

export function pendingReplies(replies: ReadonlyMap<string, string>): [string, string][] {
  return [...replies].filter(([, text]) => text.trim() !== "");
}

const currentRoute = () => parseRoute(location.pathname, location.search);

const threadOf = (threads: FakeThread[], r: Route) =>
  r.thread ? threads.find((t) => t.id === r.thread && t.file) : undefined;

function computeRange(
  file: FileData,
  overrides: ReadonlyMap<string, CommitRange>,
  global: CommitRange | null,
  approvals: ReadonlyMap<string, ReadonlySet<number>>,
  headN: number,
): CommitRange {
  return overrides.get(file.path) ?? global ?? defaultRange(file.changedBy, approvals.get(file.path), headN);
}

export function StoreProvider({
  files,
  commits,
  threads: initialThreads,
  pr: initialPr,
  saved,
  refresh = () => {},
  refreshing = false,
  refreshError = null,
  children,
}: {
  files: FileData[];
  commits: Commit[];
  threads: FakeThread[];
  pr: PrInfo;
  /** Review state from the database, or null (browser fixtures) to start from the seeded prototype state. */
  saved: SavedState | null;
  /** Re-fetches the whole PR; owned by the caller, since it reloads `files`/`commits` too. */
  refresh?: () => void;
  refreshing?: boolean;
  refreshError?: string | null;
  children: ReactNode;
}) {
  const headN = commits.length;
  // Threads are replaced after submitting a review; callbacks that never re-run read the ref.
  const [threads, setThreads] = useState(() =>
    saved ? mergeForcePushes(initialThreads, saved.forcePushes, Date.now()) : initialThreads,
  );
  const pr = initialPr;
  const threadsRef = useRef(threads);
  threadsRef.current = threads;
  const fileNames = useMemo(() => new Set(files.map((f) => f.name)), [files]);
  const fileByName = useMemo(() => new Map(files.map((f) => [f.name, f])), [files]);
  const validFile = useCallback(
    (name: string | undefined, fallback: string) => (name && fileNames.has(name) ? name : fallback),
    [fileNames],
  );

  // Ranges and approvals are mirrored in refs so navigation can read the latest values synchronously.
  const [rangeOverrides, setRangeOverrides] = useState<ReadonlyMap<string, CommitRange>>(new Map());
  const [globalRange, setGlobalRange] = useState<CommitRange | null>(null);
  const [approvals, setApprovals] = useState<ReadonlyMap<string, ReadonlySet<number>>>(() => {
    if (saved) {
      const numberOf = new Map(commits.map((c) => [c.oid, c.n]));
      const byFile = new Map<string, Set<number>>();
      for (const a of saved.approvals) {
        const n = numberOf.get(a.commitOid);
        if (n === undefined || !fileNames.has(a.filePath)) continue;
        const set = byFile.get(a.filePath) ?? new Set<number>();
        byFile.set(a.filePath, set.add(n));
      }
      return byFile;
    }
    return new Map(
      files.flatMap((f) => {
        const through = SEED_APPROVED_THROUGH[f.path];
        return through ? [[f.path, approveThrough(f.changedBy, through)] as const] : [];
      }),
    );
  });
  const overridesRef = useRef(rangeOverrides);
  const globalRef = useRef(globalRange);
  const approvalsRef = useRef(approvals);
  approvalsRef.current = approvals;

  const rangeFor = useCallback(
    (file: FileData) => computeRange(file, rangeOverrides, globalRange, approvals, headN),
    [rangeOverrides, globalRange, approvals, headN],
  );
  const currentRangeRef = (file: FileData) =>
    computeRange(file, overridesRef.current, globalRef.current, approvalsRef.current, headN);

  const setRange = useCallback((path: string, range: CommitRange) => {
    const next = new Map(overridesRef.current).set(path, range);
    overridesRef.current = next;
    setRangeOverrides(next);
  }, []);

  const resetRange = useCallback((path: string) => {
    const next = new Map(overridesRef.current);
    next.delete(path);
    overridesRef.current = next;
    setRangeOverrides(next);
  }, []);

  const setAllRanges = useCallback((range: CommitRange | null) => {
    globalRef.current = range;
    setGlobalRange(range);
    overridesRef.current = new Map();
    setRangeOverrides(overridesRef.current);
  }, []);

  /** Changes the file's range if the target isn't visible or commentable in it. */
  const ensureVisible = (r: Route, fileName: string) => {
    const file = fileByName.get(fileName);
    if (!file) return;
    const current = currentRangeRef(file);
    const thread = threadOf(threads, r);
    if (thread) {
      if (placeable(thread) && threadPlacement(thread, current, file.changedBy) === null && !needsAnchor(thread, current, file.changedBy)) {
        setRange(file.path, rangeForThread(thread, current, headN));
      }
      return;
    }
    const l = parseLine(r.line);
    if (l) {
      const ok = commentable(current, file.changedBy);
      if (!(l.side === "old" ? ok.left : ok.right)) setRange(file.path, rangeForSide(l.side, current, headN));
    }
  };

  const resolveJump = useCallback(
    (r: Route, file: string, tick: number): JumpTarget | null => {
      if (r.tab !== "diff") return null;
      const thread = threadOf(threadsRef.current, r);
      if (thread) {
        return { file: thread.file!, side: thread.side, line: thread.line ?? thread.originalLine, threadId: thread.id, tick };
      }
      const l = parseLine(r.line);
      return l ? { file, side: l.side, line: l.line, tick } : null;
    },
    [],
  );

  const initial = useMemo(() => {
    const r = currentRoute();
    const thread = threadOf(threads, r);
    const file = validFile(thread?.file ?? r.file, files[0]?.name ?? "");
    return { tab: r.tab, file, jump: resolveJump(r, file, 1), thread: thread?.id ?? null };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [tab, setTabState] = useState<Tab>(initial.tab);
  /** The discussion last jumped to; next/previous discussion steps from it. */
  const [lastThreadId, setLastThreadId] = useState<string | null>(initial.thread);
  const [currentFile, setCurrentFileState] = useState(initial.file);
  const [jump, setJump] = useState<JumpTarget | null>(initial.jump);
  const currentFileRef = useRef(currentFile);
  currentFileRef.current = currentFile;
  const tickRef = useRef(1);

  useEffect(() => {
    // A deep link to a thread or line must open at a range that shows it.
    const r = currentRoute();
    if (r.tab === "diff") ensureVisible(r, initial.file);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const apply = (r: Route) => {
    setTabState(r.tab);
    if (r.tab !== "diff") return;
    const thread = threadOf(threads, r);
    if (thread) setLastThreadId(thread.id);
    const file = validFile(thread?.file ?? r.file, currentFileRef.current);
    setCurrentFileState(file);
    ensureVisible(r, file);
    const target = resolveJump(r, file, ++tickRef.current);
    if (target) setJump(target);
  };
  const applyRef = useRef(apply);
  applyRef.current = apply;

  const navigate = useCallback((r: Route) => {
    const url = routeUrl(r);
    if (url !== location.pathname + location.search) history.pushState(null, "", url);
    applyRef.current(r);
  }, []);

  useEffect(() => {
    const onPop = () => applyRef.current(currentRoute());
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  const setTab = useCallback(
    (t: Tab) => navigate(t === "diff" ? { tab: t, file: currentFileRef.current } : { tab: t }),
    [navigate],
  );
  const setCurrentFile = useCallback((name: string) => navigate({ tab: "diff", file: name }), [navigate]);
  const jumpTo = useCallback(
    (file: string, side: Side, line: number, threadId?: string) =>
      navigate(threadId ? { tab: "diff", file, thread: threadId } : { tab: "diff", file, line: `${side}:${line}` }),
    [navigate],
  );
  const viewAsOf = useCallback(
    (thread: FakeThread) => {
      if (!thread.file || thread.commit === 0) return;
      setRange(thread.file, asOfRange(thread));
      navigate({ tab: "diff", file: thread.file, thread: thread.id });
    },
    [navigate, setRange],
  );
  const clearJump = useCallback(() => setJump(null), []);

  const [drafts, setDrafts] = useState<ReadonlyMap<string, Draft>>(
    () =>
      new Map(
        (saved?.drafts ?? [])
          .filter((d) => fileNames.has(d.filePath))
          .map((d): [string, Draft] => [
            d.draftKey,
            {
              key: d.draftKey,
              file: d.filePath,
              range: { side: d.side, start: d.startLine, end: d.endLine },
              text: d.body,
              stale: d.stale,
            },
          ]),
      ),
  );
  const draftsRef = useRef(drafts);
  draftsRef.current = drafts;
  const [focus, setFocus] = useState<{ key: string; tick: number } | null>(null);
  const [replies, setReplies] = useState<ReadonlyMap<string, string>>(() => {
    const threadIds = new Set(threads.map((t) => t.id));
    return new Map(
      (saved?.replies ?? []).filter((r) => threadIds.has(r.threadId)).map((r): [string, string] => [r.threadId, r.body]),
    );
  });
  const [resolutions, setResolutions] = useState<ReadonlyMap<string, Resolution>>(() =>
    saved
      ? new Map(
          saved.resolutions.map((r): [string, Resolution] => [
            r.threadId,
            { through: r.resolvedThrough, reopened: r.reopened },
          ]),
        )
      : new Map(
          threads.flatMap((t) =>
            t.resolvedThrough ? [[t.id, { through: t.resolvedThrough, reopened: false }] as const] : [],
          ),
        ),
  );
  const resolutionsRef = useRef(resolutions);
  resolutionsRef.current = resolutions;
  const [reviewBody, setReviewBodyState] = useState(saved?.reviewBody ?? "");
  const setReviewBody = useCallback((s: string) => {
    setReviewBodyState(s);
    persist.reviewBody(s);
  }, []);

  const approveFile = useCallback(
    (file: FileData, throughOid?: string) => {
      const through = throughOid === undefined ? undefined : commits.find((c) => c.oid === throughOid)?.n;
      // Never lowers an existing approval: approving through an earlier commit keeps later ones.
      const approved = new Set([...(approvalsRef.current.get(file.path) ?? []), ...approveThrough(file.changedBy, through)]);
      setApprovals((prev) => new Map(prev).set(file.path, approved));
      persist.fileApprovals(file.path, [...approved].map((n) => commits[n - 1].oid));
    },
    [commits],
  );

  const unapproveFile = useCallback((file: FileData) => {
    setApprovals((prev) => {
      const next = new Map(prev);
      next.delete(file.path);
      return next;
    });
    persist.fileApprovals(file.path, []);
  }, []);

  const createDraft = useCallback((file: string, range: LineRange) => {
    const key = draftKey(file, range);
    setDrafts((prev) => (prev.has(key) ? prev : new Map(prev).set(key, { key, file, range, text: "", stale: false })));
    setFocus((f) => ({ key, tick: (f?.tick ?? 0) + 1 }));
  }, []);

  const patchDraft = useCallback((key: string, text: string) => {
    setDrafts((prev) => {
      const d = prev.get(key);
      return d ? new Map(prev).set(key, { ...d, text }) : prev;
    });
    const d = draftsRef.current.get(key);
    if (d) {
      persist.draft({
        draftKey: key,
        filePath: d.file,
        side: d.range.side,
        startLine: d.range.start,
        endLine: d.range.end,
        body: text,
      });
    }
  }, []);

  const removeDraft = useCallback((key: string) => {
    setDrafts((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Map(prev);
      next.delete(key);
      return next;
    });
    persist.deleteDraft(key);
  }, []);

  const acknowledgeDraft = useCallback((key: string) => {
    setDrafts((prev) => {
      const d = prev.get(key);
      return d ? new Map(prev).set(key, { ...d, stale: false }) : prev;
    });
    persist.acknowledgeDraft(key);
  }, []);

  const submitReview = useCallback(async (
    event: ReviewEvent,
    text: string,
    directives: Directives,
    onProgress?: (message: string) => void,
  ): Promise<SubmitResult | null> => {
    const result = await sendReview(event, text, directives, onProgress);
    if (!result) return null;
    // The backend already cleared its copy; mirror that here without saving anything.
    setDrafts(new Map());
    if (result.directives.error) {
      // Keep what still has to be applied, so submitting again retries only that.
      setReviewBody(formatDirectives(result.directives.unapplied));
    } else {
      setReviewBodyState("");
    }
    setReplies((prev) =>
      result.conversationError ? new Map([...prev].filter(([id]) => id === CONVERSATION_ID)) : new Map(),
    );
    if (result.threads) {
      const now = Date.now();
      setThreads(result.threads.map((t) => threadFromJson(t, now)));
    }
    return result;
  }, []);

  const setReply = useCallback((threadId: string, text: string | null) => {
    setReplies((prev) => {
      const next = new Map(prev);
      if (text === null) next.delete(threadId);
      else next.set(threadId, text);
      return next;
    });
    persist.reply(threadId, text ?? "");
  }, []);

  const resolveThread = useCallback((t: FakeThread) => {
    const through = lastCommentId(t);
    setResolutions((prev) => new Map(prev).set(t.id, { through, reopened: false }));
    persist.resolution(t.id, through, false);
  }, []);

  const reopenThread = useCallback((t: FakeThread) => {
    const through = resolutionsRef.current.get(t.id)?.through ?? null;
    setResolutions((prev) => new Map(prev).set(t.id, { through: prev.get(t.id)?.through ?? null, reopened: true }));
    persist.resolution(t.id, through, true);
  }, []);

  const value = useMemo<Store>(
    () => ({
      files,
      fileByName,
      commits,
      threads,
      pr,
      refresh,
      refreshing,
      refreshError,
      approvals,
      approveFile,
      unapproveFile,
      rangeFor,
      setRange,
      resetRange,
      globalRange,
      setAllRanges,
      viewAsOf,
      tab,
      setTab,
      currentFile,
      setCurrentFile,
      drafts,
      focus,
      createDraft,
      patchDraft,
      removeDraft,
      acknowledgeDraft,
      submitReview,
      replies,
      setReply,
      resolutions,
      resolveThread,
      reopenThread,
      reviewBody,
      setReviewBody,
      jump,
      lastThreadId,
      jumpTo,
      clearJump,
    }),
    [
      files,
      fileByName,
      commits,
      threads,
      pr,
      refresh,
      refreshing,
      refreshError,
      approvals,
      approveFile,
      unapproveFile,
      rangeFor,
      setRange,
      resetRange,
      globalRange,
      setAllRanges,
      viewAsOf,
      tab,
      setTab,
      currentFile,
      setCurrentFile,
      drafts,
      focus,
      createDraft,
      patchDraft,
      removeDraft,
      acknowledgeDraft,
      submitReview,
      replies,
      setReply,
      resolutions,
      resolveThread,
      reopenThread,
      reviewBody,
      setReviewBody,
      jump,
      lastThreadId,
      jumpTo,
      clearJump,
    ],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
