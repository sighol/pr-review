import { useCallback, useEffect, useMemo, useState } from "react";
import { navigable, pickInOtherFile, pickRendered, type Box } from "../lib/nav";
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
  const { threads, resolutions, files, currentFile, tab, jumpTo } = useStore();
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
      const byId = new Map(threads.map((t) => [t.id, t]));
      // Discussions on screen are measured from the middle of the viewport, where jumping puts one.
      const boxes: Box[] = [...document.querySelectorAll<HTMLElement>(".diff [data-thread-id]")].map((el) => {
        const r = el.getBoundingClientRect();
        return { id: el.dataset.threadId!, top: r.top, bottom: r.bottom };
      });
      const onScreen = pickRendered(boxes, window.innerHeight / 2, dir, (id) => {
        const t = byId.get(id);
        return !!t && keep(t);
      });
      const target = onScreen ? byId.get(onScreen) : pickInOtherFile(threads, fileOrder, currentFile, dir, keep);
      if (!target?.file) {
        const what = kind === "unresolved" ? "unresolved discussions" : "discussions";
        setNotice(`No more ${what} ${dir === 1 ? "after" : "before"} this`);
        return;
      }
      setNotice(null);
      jumpTo(target.file, target.side, threadLine(target), target.id);
    },
    [threads, fileOrder, currentFile, isOpen, jumpTo],
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
        ◀ Discussion
      </button>
      <button disabled={!anyNavigable} title="Next discussion (n)" onClick={() => go(1, "any")}>
        Discussion ▶
      </button>
      <button disabled={!anyOpen} title="Previous unresolved discussion (Shift+P)" onClick={() => go(-1, "unresolved")}>
        ◀ Unresolved
      </button>
      <button disabled={!anyOpen} title="Next unresolved discussion (Shift+N)" onClick={() => go(1, "unresolved")}>
        Unresolved ▶
      </button>
      {notice && <span className="nav-notice">{notice}</span>}
    </div>
  );
}
