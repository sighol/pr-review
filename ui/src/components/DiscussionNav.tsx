import { useCallback, useEffect, useMemo, useState } from "react";
import { navigable, pickNeighbor } from "../lib/nav";
import { isResolved, type FakeThread } from "../lib/threads";
import { threadLine, useStore } from "../store";

type Kind = "any" | "unresolved";

const KEYS: Record<string, [1 | -1, Kind]> = {
  n: [1, "any"],
  p: [-1, "any"],
  N: [1, "unresolved"],
  P: [-1, "unresolved"],
};

/** Previous/next discussion buttons for the Diff toolbar, with the n/p and Shift+N/P keys. */
export function DiscussionNav() {
  const { threads, resolutions, files, currentFile, lastThreadId, tab, jumpTo } = useStore();
  const [notice, setNotice] = useState<string | null>(null);
  const fileOrder = useMemo(() => files.map((f) => f.name), [files]);

  const isOpen = useCallback((t: FakeThread) => !isResolved(t, resolutions.get(t.id)), [resolutions]);
  const anyNavigable = threads.some((t) => navigable(t, fileOrder));
  const anyOpen = threads.some((t) => navigable(t, fileOrder) && isOpen(t));

  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 2500);
    return () => clearTimeout(timer);
  }, [notice]);

  const go = useCallback(
    (dir: 1 | -1, kind: Kind) => {
      const keep = (t: FakeThread) => kind === "any" || isOpen(t);
      const target = pickNeighbor(threads, fileOrder, currentFile, lastThreadId, dir, keep);
      if (!target?.file) {
        const what = kind === "unresolved" ? "unresolved discussions" : "discussions";
        setNotice(`No more ${what} ${dir === 1 ? "after" : "before"} this`);
        return;
      }
      setNotice(null);
      jumpTo(target.file, target.side, threadLine(target), target.id);
    },
    [threads, fileOrder, currentFile, lastThreadId, isOpen, jumpTo],
  );

  useEffect(() => {
    if (tab !== "diff") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
      const action = KEYS[e.key];
      if (!action) return;
      e.preventDefault();
      go(...action);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab, go]);

  return (
    <div className="nav-group" role="group" aria-label="Discussion navigation">
      <button disabled={!anyNavigable} title="Previous discussion (p)" onClick={() => go(-1, "any")}>
        ◀&nbsp;Discussion
      </button>
      <button disabled={!anyNavigable} title="Next discussion (n)" onClick={() => go(1, "any")}>
        Discussion&nbsp;▶
      </button>
      <button disabled={!anyOpen} title="Previous unresolved discussion (Shift+P)" onClick={() => go(-1, "unresolved")}>
        ◀&nbsp;Unresolved
      </button>
      <button disabled={!anyOpen} title="Next unresolved discussion (Shift+N)" onClick={() => go(1, "unresolved")}>
        Unresolved&nbsp;▶
      </button>
      {notice && <span className="nav-notice">{notice}</span>}
    </div>
  );
}
