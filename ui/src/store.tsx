import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import type { LineRange, Side } from "./lib/diff";
import { ALL_THREADS, lastCommentId, type FakeThread, type Resolution } from "./lib/threads";

export interface FileData {
  name: string;
  label: string;
  path: string;
  oldText: string;
  newText: string;
  diffText: string;
}

export interface Draft {
  key: string;
  file: string;
  range: LineRange;
  text: string;
  initial: string;
}

export type Tab = "summary" | "diff" | "review";

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
  threads: FakeThread[];

  tab: Tab;
  setTab: (t: Tab) => void;
  currentFile: string;
  setCurrentFile: (name: string) => void;

  drafts: ReadonlyMap<string, Draft>;
  focus: { key: string; tick: number } | null;
  createDraft: (file: string, range: LineRange, initial: string) => void;
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

/** Untouched suggestion pre-fills and empty boxes don't count as pending. */
export const isRealDraft = (d: Draft) => d.text.trim() !== "" && d.text !== d.initial;

export function pendingReplies(replies: ReadonlyMap<string, string>): [string, string][] {
  return [...replies].filter(([, text]) => text.trim() !== "");
}

export function StoreProvider({ files, children }: { files: FileData[]; children: ReactNode }) {
  const [tab, setTab] = useState<Tab>(() => {
    const t = new URLSearchParams(location.search).get("tab");
    return t === "summary" || t === "review" ? t : "diff";
  });
  const [currentFile, setCurrentFile] = useState(files[0]?.name ?? "");
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
  const [jump, setJump] = useState<JumpTarget | null>(null);

  const createDraft = useCallback((file: string, range: LineRange, initial: string) => {
    const key = draftKey(file, range);
    setDrafts((prev) => (prev.has(key) ? prev : new Map(prev).set(key, { key, file, range, text: initial, initial })));
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

  const jumpTo = useCallback((file: string, side: Side, line: number, threadId?: string) => {
    setCurrentFile(file);
    setTab("diff");
    setJump((j) => ({ file, side, line, threadId, tick: (j?.tick ?? 0) + 1 }));
  }, []);

  const clearJump = useCallback(() => setJump(null), []);

  const fileByName = useMemo(() => new Map(files.map((f) => [f.name, f])), [files]);

  const value = useMemo<Store>(
    () => ({
      files,
      fileByName,
      threads: ALL_THREADS,
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
      tab,
      currentFile,
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
