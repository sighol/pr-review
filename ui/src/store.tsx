import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { approveThrough } from "./lib/approval";
import type { LineRange, Side } from "./lib/diff";
import { parseLine, parseRoute, routeUrl, type Route, type Tab } from "./lib/route";
import { ALL_THREADS, lastCommentId, type FakeThread, type Resolution } from "./lib/threads";

export type { Tab } from "./lib/route";

export interface Commit {
  n: number;
  oid: string;
  short: string;
  subject: string;
}

/** The old text and diff when diffing from commit `k` (0 is the PR base) to the file's latest commit. */
export interface FileVersion {
  oldText: string;
  diffText: string;
}

export interface FileData {
  /** Same as `path`; the key used in URLs and drafts. */
  name: string;
  path: string;
  added: number;
  removed: number;
  /** Commit numbers (1-based) that touched this file. */
  changedBy: number[];
  /** The file at its latest commit. */
  newText: string;
  /** Keyed by start commit k. Key 0 is the full PR diff. */
  versions: ReadonlyMap<number, FileVersion>;
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

interface Store {
  files: FileData[];
  fileByName: ReadonlyMap<string, FileData>;
  commits: Commit[];
  threads: FakeThread[];

  /** File path -> approved commit numbers. */
  approvals: ReadonlyMap<string, ReadonlySet<number>>;
  approveFile: (file: FileData) => void;
  unapproveFile: (file: FileData) => void;

  tab: Tab;
  setTab: (t: Tab) => void;
  currentFile: string;
  setCurrentFile: (name: string) => void;

  drafts: ReadonlyMap<string, Draft>;
  focus: { key: string; tick: number } | null;
  createDraft: (file: string, range: LineRange) => void;
  patchDraft: (key: string, text: string) => void;
  removeDraft: (key: string) => void;

  /** Reply drafts by thread id. "" means an open but empty box. */
  replies: ReadonlyMap<string, string>;
  setReply: (threadId: string, text: string | null) => void;

  resolutions: ReadonlyMap<string, Resolution>;
  resolveThread: (t: FakeThread) => void;
  reopenThread: (t: FakeThread) => void;

  reviewBody: string;
  setReviewBody: (s: string) => void;

  jump: JumpTarget | null;
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

const threadOf = (r: Route) => (r.thread ? ALL_THREADS.find((t) => t.id === r.thread && t.file) : undefined);

export function StoreProvider({
  files,
  commits,
  children,
}: {
  files: FileData[];
  commits: Commit[];
  children: ReactNode;
}) {
  const fileNames = useMemo(() => new Set(files.map((f) => f.name)), [files]);
  const validFile = useCallback(
    (name: string | undefined, fallback: string) => (name && fileNames.has(name) ? name : fallback),
    [fileNames],
  );

  const resolveJump = useCallback(
    (r: Route, file: string, tick: number): JumpTarget | null => {
      if (r.tab !== "diff") return null;
      const thread = threadOf(r);
      if (thread) return { file: thread.file!, side: thread.side, line: thread.line, threadId: thread.id, tick };
      const l = parseLine(r.line);
      return l ? { file, side: l.side, line: l.line, tick } : null;
    },
    [],
  );

  const initial = useMemo(() => {
    const r = currentRoute();
    const file = validFile(threadOf(r)?.file ?? r.file, files[0]?.name ?? "");
    return { tab: r.tab, file, jump: resolveJump(r, file, 1) };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [tab, setTabState] = useState<Tab>(initial.tab);
  const [currentFile, setCurrentFileState] = useState(initial.file);
  const [jump, setJump] = useState<JumpTarget | null>(initial.jump);
  const currentFileRef = useRef(currentFile);
  currentFileRef.current = currentFile;
  const tickRef = useRef(1);

  const apply = useCallback(
    (r: Route) => {
      setTabState(r.tab);
      if (r.tab !== "diff") return;
      const file = validFile(threadOf(r)?.file ?? r.file, currentFileRef.current);
      setCurrentFileState(file);
      const target = resolveJump(r, file, ++tickRef.current);
      if (target) setJump(target);
    },
    [validFile, resolveJump],
  );

  const navigate = useCallback(
    (r: Route) => {
      const url = routeUrl(r);
      if (url !== location.pathname + location.search) history.pushState(null, "", url);
      apply(r);
    },
    [apply],
  );

  useEffect(() => {
    const onPop = () => apply(currentRoute());
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [apply]);

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
  const clearJump = useCallback(() => setJump(null), []);

  const [drafts, setDrafts] = useState<ReadonlyMap<string, Draft>>(new Map());
  const [focus, setFocus] = useState<{ key: string; tick: number } | null>(null);
  const [replies, setReplies] = useState<ReadonlyMap<string, string>>(new Map());
  const [resolutions, setResolutions] = useState<ReadonlyMap<string, Resolution>>(
    () =>
      new Map(
        ALL_THREADS.flatMap((t) =>
          t.resolvedThrough ? [[t.id, { through: t.resolvedThrough, reopened: false }] as const] : [],
        ),
      ),
  );
  const [reviewBody, setReviewBody] = useState("");
  const [approvals, setApprovals] = useState<ReadonlyMap<string, ReadonlySet<number>>>(
    () =>
      new Map(
        files.flatMap((f) => {
          const through = SEED_APPROVED_THROUGH[f.path];
          return through ? [[f.path, approveThrough(f.changedBy, through)] as const] : [];
        }),
      ),
  );

  const approveFile = useCallback((file: FileData) => {
    setApprovals((prev) => new Map(prev).set(file.path, approveThrough(file.changedBy)));
  }, []);

  const unapproveFile = useCallback((file: FileData) => {
    setApprovals((prev) => {
      const next = new Map(prev);
      next.delete(file.path);
      return next;
    });
  }, []);

  const createDraft = useCallback((file: string, range: LineRange) => {
    const key = draftKey(file, range);
    setDrafts((prev) => (prev.has(key) ? prev : new Map(prev).set(key, { key, file, range, text: "" })));
    setFocus((f) => ({ key, tick: (f?.tick ?? 0) + 1 }));
  }, []);

  const patchDraft = useCallback((key: string, text: string) => {
    setDrafts((prev) => {
      const d = prev.get(key);
      return d ? new Map(prev).set(key, { ...d, text }) : prev;
    });
  }, []);

  const removeDraft = useCallback((key: string) => {
    setDrafts((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Map(prev);
      next.delete(key);
      return next;
    });
  }, []);

  const setReply = useCallback((threadId: string, text: string | null) => {
    setReplies((prev) => {
      const next = new Map(prev);
      if (text === null) next.delete(threadId);
      else next.set(threadId, text);
      return next;
    });
  }, []);

  const resolveThread = useCallback((t: FakeThread) => {
    setResolutions((prev) => new Map(prev).set(t.id, { through: lastCommentId(t), reopened: false }));
  }, []);

  const reopenThread = useCallback((t: FakeThread) => {
    setResolutions((prev) => new Map(prev).set(t.id, { through: prev.get(t.id)?.through ?? null, reopened: true }));
  }, []);

  const fileByName = useMemo(() => new Map(files.map((f) => [f.name, f])), [files]);

  const value = useMemo<Store>(
    () => ({
      files,
      fileByName,
      commits,
      threads: ALL_THREADS,
      approvals,
      approveFile,
      unapproveFile,
      tab,
      setTab,
      currentFile,
      setCurrentFile,
      drafts,
      focus,
      createDraft,
      patchDraft,
      removeDraft,
      replies,
      setReply,
      resolutions,
      resolveThread,
      reopenThread,
      reviewBody,
      setReviewBody,
      jump,
      jumpTo,
      clearJump,
    }),
    [
      files,
      fileByName,
      commits,
      approvals,
      approveFile,
      unapproveFile,
      tab,
      setTab,
      currentFile,
      setCurrentFile,
      drafts,
      focus,
      createDraft,
      patchDraft,
      removeDraft,
      replies,
      setReply,
      resolutions,
      resolveThread,
      reopenThread,
      reviewBody,
      jump,
      jumpTo,
      clearJump,
    ],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
