import type { ThemedToken } from "shiki";
import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  buildRows,
  clampDrag,
  isChanged,
  parseUnifiedDiff,
  rangeFromRows,
  rangeKey,
  segmentsFor,
  suggestionFor,
  textToLines,
  type Cell,
  type LineRange,
  type Row,
  type Side,
} from "../lib/diff";
import { tokenize } from "../lib/highlight";
import { FAKE_THREADS, lastCommentId, type FakeThread, type Resolution } from "../lib/threads";
import { CommentBox, type Draft } from "./CommentBox";
import { ThreadBox } from "./ThreadBox";

interface Props {
  fixtureName: string;
  path: string;
  oldText: string;
  newText: string;
  diffText: string;
  wholeFile: boolean;
}

type Tokens = ThemedToken[][] | null;

interface Selection {
  side: Side;
  lo: number;
  hi: number;
}

export function DiffView({ fixtureName, path, oldText, newText, diffText, wholeFile }: Props) {
  const oldLines = useMemo(() => textToLines(oldText), [oldText]);
  const newLines = useMemo(() => textToLines(newText), [newText]);
  const hunks = useMemo(() => parseUnifiedDiff(diffText), [diffText]);
  const [expandedGaps, setExpandedGaps] = useState<ReadonlySet<string>>(new Set());
  const rows = useMemo(
    () => buildRows(oldLines, newLines, hunks, { wholeFile, expandedGaps }),
    [oldLines, newLines, hunks, wholeFile, expandedGaps],
  );

  const [oldTokens, setOldTokens] = useState<Tokens>(null);
  const [newTokens, setNewTokens] = useState<Tokens>(null);
  useEffect(() => {
    let cancelled = false;
    tokenize(oldText, path).then((t) => !cancelled && setOldTokens(t));
    tokenize(newText, path).then((t) => !cancelled && setNewTokens(t));
    return () => {
      cancelled = true;
    };
  }, [oldText, newText, path]);

  const [resolutions, setResolutions] = useState<Map<string, Resolution>>(
    () =>
      new Map(
        (FAKE_THREADS[fixtureName] ?? []).flatMap((t) =>
          t.resolvedThrough ? [[t.id, { through: t.resolvedThrough, reopened: false }] as const] : [],
        ),
      ),
  );
  const resolveThread = useCallback((t: FakeThread) => {
    setResolutions((prev) => new Map(prev).set(t.id, { through: lastCommentId(t), reopened: false }));
  }, []);
  const reopenThread = useCallback((t: FakeThread) => {
    setResolutions((prev) => new Map(prev).set(t.id, { through: prev.get(t.id)?.through ?? null, reopened: true }));
  }, []);

  const [drafts, setDrafts] = useState<Map<string, Draft>>(new Map());
  const [focus, setFocus] = useState<{ key: string; tick: number } | null>(null);
  const [sel, setSel] = useState<Selection | null>(null);
  const dragRef = useRef<{ side: Side; anchor: number } | null>(null);
  const selRef = useRef<Selection | null>(null);
  const rowsRef = useRef<Row[]>(rows);
  rowsRef.current = rows;
  const containerRef = useRef<HTMLDivElement>(null);

  const rowIndexByLine = useMemo(() => {
    const m = new Map<string, number>();
    rows.forEach((r, i) => {
      if (r.type !== "line") return;
      if (r.left) m.set(`old:${r.left.lineNo}`, i);
      if (r.right) m.set(`new:${r.right.lineNo}`, i);
    });
    return m;
  }, [rows]);

  const createDraft = useCallback(
    (range: LineRange) => {
      const key = rangeKey(range);
      setDrafts((prev) => {
        if (prev.has(key)) return prev;
        const initial = suggestionFor(range, newLines);
        return new Map(prev).set(key, { key, range, text: initial, initial });
      });
      setFocus((f) => ({ key, tick: (f?.tick ?? 0) + 1 }));
    },
    [newLines],
  );

  useEffect(() => {
    const updateSel = (next: Selection | null) => {
      selRef.current = next;
      setSel(next);
    };
    const onMove = (e: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-ri]");
      if (!el) return;
      const [lo, hi] = clampDrag(rowsRef.current, drag.side, drag.anchor, Number(el.dataset.ri));
      const cur = selRef.current;
      if (!cur || cur.lo !== lo || cur.hi !== hi) updateSel({ side: drag.side, lo, hi });
    };
    const onUp = () => {
      const drag = dragRef.current;
      const cur = selRef.current;
      dragRef.current = null;
      if (!drag || !cur) return;
      updateSel(null);
      const range = rangeFromRows(rowsRef.current, drag.side, cur.lo, cur.hi);
      if (range) createDraft(range);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [createDraft]);

  const onGutterDown = useCallback((e: React.PointerEvent, side: Side, ri: number) => {
    if (e.button !== 0) return;
    e.preventDefault();
    dragRef.current = { side, anchor: ri };
    const next = { side, lo: ri, hi: ri };
    selRef.current = next;
    setSel(next);
  }, []);

  const expandGap = useCallback((id: string) => {
    setExpandedGaps((prev) => new Set(prev).add(id));
  }, []);

  const onCodeDown = useCallback((e: React.PointerEvent) => {
    const code = (e.target as HTMLElement).closest<HTMLElement>("[data-code-side]");
    if (code && containerRef.current) containerRef.current.dataset.selectSide = code.dataset.codeSide;
  }, []);

  const threadsByRow = useMemo(() => {
    const m = new Map<number, FakeThread[]>();
    for (const t of FAKE_THREADS[fixtureName] ?? []) {
      const ri = rowIndexByLine.get(`${t.side}:${t.line}`);
      if (ri !== undefined) m.set(ri, [...(m.get(ri) ?? []), t]);
    }
    return m;
  }, [fixtureName, rowIndexByLine]);

  const draftsByRow = useMemo(() => {
    const m = new Map<number, Draft[]>();
    for (const d of drafts.values()) {
      const ri = rowIndexByLine.get(`${d.range.side}:${d.range.end}`);
      if (ri !== undefined) m.set(ri, [...(m.get(ri) ?? []), d]);
    }
    return m;
  }, [drafts, rowIndexByLine]);

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

  return (
    <div className="diff" ref={containerRef} onPointerDown={onCodeDown}>
      {rows.map((row, i) => {
        const inSel = sel && i >= sel.lo && i <= sel.hi ? sel.side : null;
        return (
          <Fragment key={row.id}>
            {row.type === "gap" ? (
              <GapRow row={row} ri={i} onExpand={expandGap} />
            ) : (
              <LineRow
                row={row}
                ri={i}
                oldTokens={oldTokens}
                newTokens={newTokens}
                selectedSide={inSel}
                onGutterDown={onGutterDown}
              />
            )}
            {threadsByRow.get(i)?.map((t) => (
              <div className="attached" data-ri={i} key={t.id}>
                <ThreadBox thread={t} resolution={resolutions.get(t.id)} onResolve={resolveThread} onReopen={reopenThread} />
              </div>
            ))}
            {draftsByRow.get(i)?.map((d) => (
              <div className="attached" data-ri={i} key={d.key}>
                <CommentBox
                  draft={d}
                  focusTick={focus?.key === d.key ? focus.tick : 0}
                  onChange={patchDraft}
                  onDiscard={removeDraft}
                />
              </div>
            ))}
          </Fragment>
        );
      })}
    </div>
  );
}

const GapRow = memo(function GapRow({ row, ri, onExpand }: { row: Extract<Row, { type: "gap" }>; ri: number; onExpand: (id: string) => void }) {
  return (
    <div className="gap" data-ri={ri}>
      <button onClick={() => onExpand(row.id)}>
        ↕ {row.count} unchanged {row.count === 1 ? "line" : "lines"} (lines {row.newStart}–{row.newStart + row.count - 1})
      </button>
    </div>
  );
});

interface LineRowProps {
  row: Extract<Row, { type: "line" }>;
  ri: number;
  oldTokens: Tokens;
  newTokens: Tokens;
  selectedSide: Side | null;
  onGutterDown: (e: React.PointerEvent, side: Side, ri: number) => void;
}

const LineRow = memo(function LineRow({ row, ri, oldTokens, newTokens, selectedSide, onGutterDown }: LineRowProps) {
  return (
    <div className="row" data-ri={ri}>
      <Pane cell={row.left} side="old" ri={ri} tokens={oldTokens} selected={selectedSide === "old"} onGutterDown={onGutterDown} />
      <Pane cell={row.right} side="new" ri={ri} tokens={newTokens} selected={selectedSide === "new"} onGutterDown={onGutterDown} />
    </div>
  );
});

interface PaneProps {
  cell: Cell | null;
  side: Side;
  ri: number;
  tokens: Tokens;
  selected: boolean;
  onGutterDown: (e: React.PointerEvent, side: Side, ri: number) => void;
}

function Pane({ cell, side, ri, tokens, selected, onGutterDown }: PaneProps) {
  const changed = isChanged(cell);
  const kind = cell ? cell.kind : "empty";
  const cls = `${side} ${kind}${selected && cell ? " selected" : ""}`;
  return (
    <>
      <div
        className={`gutter ${cls}${changed ? " clickable" : ""}`}
        onPointerDown={changed ? (e) => onGutterDown(e, side, ri) : undefined}
        onMouseDown={changed ? (e) => e.preventDefault() : undefined}
      >
        {cell?.lineNo}
      </div>
      <div className={`code ${cls}`} data-code-side={side}>
        {cell && <Code cell={cell} tokens={tokens} />}
      </div>
    </>
  );
}

function Code({ cell, tokens }: { cell: Cell; tokens: Tokens }) {
  const segs = segmentsFor(tokens?.[cell.lineNo - 1], cell.text, cell.ranges);
  return (
    <>
      {segs.map((s, i) =>
        s.changed ? (
          <mark key={i} style={{ color: s.color }}>
            {s.text}
          </mark>
        ) : (
          <span key={i} style={{ color: s.color }}>
            {s.text}
          </span>
        ),
      )}
      {cell.text === "" && "​"}
      {cell.noEol && <span className="noeol" title="No newline at end of file"> ⏎̸</span>}
    </>
  );
}
