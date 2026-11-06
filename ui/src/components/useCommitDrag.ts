import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

export interface CommitDrag {
  a: number;
  b: number;
}

/**
 * Drag across elements carrying `data-n="<commit number>"`. Calls `onDone`
 * with the two commit numbers where the drag started and ended.
 */
export function useCommitDrag(onDone: (a: number, b: number) => void) {
  const [drag, setDrag] = useState<CommitDrag | null>(null);
  const dragRef = useRef<CommitDrag | null>(null);
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const cur = dragRef.current;
      if (!cur) return;
      const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-n]");
      const n = el ? Number(el.dataset.n) : NaN;
      if (Number.isNaN(n) || n === cur.b) return;
      dragRef.current = { a: cur.a, b: n };
      setDrag(dragRef.current);
    };
    const onUp = () => {
      const cur = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (cur) doneRef.current(cur.a, cur.b);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, []);

  const start = useCallback(
    (n: number) => (e: ReactPointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      dragRef.current = { a: n, b: n };
      setDrag(dragRef.current);
    },
    [],
  );

  return { drag, start };
}
