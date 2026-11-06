import type { ThemedToken } from "shiki";
import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  buildRows,
  clampDrag,
  isChanged,
  parseUnifiedDiff,
  rangeFromRows,
  segmentsFor,
  textToLines,
  type Cell,
  type LineRange,
  type Row,
  type Side,
} from "../lib/diff";
import { tokenize } from "../lib/highlight";
import type { FakeThread } from "../lib/threads";
import type { RangeData } from "../lib/data";
import { commentable, threadPlacement, type CommitRange } from "../lib/range";
import { useStore, type Draft, type FileData } from "../store";
import { CommentBox } from "./CommentBox";
import { ThreadBox } from "./ThreadBox";

interface Props {
  file: FileData;
  data: RangeData;
  range: CommitRange;
  wholeFile: boolean;
}

type Tokens = ThemedToken[][] | null;

interface Selection {
  side: Side;
  lo: number;
  hi: number;
}

export function DiffView({ file, data, range, wholeFile }: Props) {
  const { threads, drafts, jump, clearJump, createDraft: storeCreateDraft } = useStore();
  const { path } = file;
  const { oldText, newText, diffText } = data;
  const canComment = commentable(range, file.changedBy);
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
    (range: LineRange) => storeCreateDraft(file.name, range),
    [storeCreateDraft, file.name],
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

  const placedLine = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of threads) {
      if (t.file !== file.name) continue;
      const line = threadPlacement(t, range, file.changedBy);
      if (line !== null) m.set(t.id, line);
    }
    return m;
  }, [threads, file.name, file.changedBy, range]);

  useEffect(() => {
    if (!jump || jump.file !== file.name) return;
    const placed = jump.threadId ? placedLine.get(jump.threadId) : jump.line;
    if (placed === undefined) {
      clearJump();
      return;
    }
    const ri = rowIndexByLine.get(`${jump.side}:${placed}`);
    if (ri === undefined) {
      const hidingGap = rows.find(
        (r) =>
          r.type === "gap" &&
          (jump.side === "old"
            ? placed >= r.oldStart && placed < r.oldStart + r.count
            : placed >= r.newStart && placed < r.newStart + r.count),
      );
      if (hidingGap) {
        expandGap(hidingGap.id);
        return;
      }
      clearJump();
      return;
    }
    clearJump();
    const threadId = jump.threadId;
    const scroll = () => {
      const q = threadId ? `[data-thread-id="${threadId}"]` : `.row[data-ri="${ri}"]`;
      const el = containerRef.current?.querySelector<HTMLElement>(q);
      if (!el) return;
      el.scrollIntoView({ block: "center" });
      return threadId ? (el.querySelector<HTMLElement>(".comment") ?? el) : el;
    };
    requestAnimationFrame(() => {
      const el = scroll();
      const cls = threadId ? "flash-thread" : "flash";
      el?.classList.add(cls);
      setTimeout(scroll, 150);
      setTimeout(() => el?.classList.remove(cls), threadId ? 1100 : 1800);
    });
  }, [jump, rows, rowIndexByLine, placedLine, file.name, expandGap, clearJump]);

  const onCodeDown = useCallback((e: React.PointerEvent) => {
    const code = (e.target as HTMLElement).closest<HTMLElement>("[data-code-side]");
    if (code && containerRef.current) containerRef.current.dataset.selectSide = code.dataset.codeSide;
  }, []);

  const threadsByRow = useMemo(() => {
    const m = new Map<number, FakeThread[]>();
    for (const t of threads) {
      const line = placedLine.get(t.id);
      if (t.file !== file.name || line === undefined) continue;
      const ri = rowIndexByLine.get(`${t.side}:${line}`);
      if (ri !== undefined) m.set(ri, [...(m.get(ri) ?? []), t]);
    }
    return m;
  }, [threads, file.name, placedLine, rowIndexByLine]);

  const draftsByRow = useMemo(() => {
    const m = new Map<number, Draft[]>();
    for (const d of drafts.values()) {
      if (d.file !== file.name || !(d.range.side === "old" ? canComment.left : canComment.right)) continue;
      const ri = rowIndexByLine.get(`${d.range.side}:${d.range.end}`);
      if (ri !== undefined) m.set(ri, [...(m.get(ri) ?? []), d]);
    }
    return m;
  }, [drafts, file.name, canComment.left, canComment.right, rowIndexByLine]);

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
                leftEnabled={canComment.left}
                rightEnabled={canComment.right}
                onGutterDown={onGutterDown}
              />
            )}
            {threadsByRow.get(i)?.map((t) => (
              <div className="attached" data-ri={i} data-thread-id={t.id} key={t.id}>
                <ThreadBox thread={t} />
              </div>
            ))}
            {draftsByRow.get(i)?.map((d) => (
              <div className="attached" data-ri={i} key={d.key}>
                <CommentBox draft={d} />
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
  leftEnabled: boolean;
  rightEnabled: boolean;
  onGutterDown: (e: React.PointerEvent, side: Side, ri: number) => void;
}

const LineRow = memo(function LineRow({ row, ri, oldTokens, newTokens, selectedSide, leftEnabled, rightEnabled, onGutterDown }: LineRowProps) {
  return (
    <div className="row" data-ri={ri}>
      <Pane cell={row.left} side="old" ri={ri} tokens={oldTokens} selected={selectedSide === "old"} enabled={leftEnabled} onGutterDown={onGutterDown} />
      <Pane cell={row.right} side="new" ri={ri} tokens={newTokens} selected={selectedSide === "new"} enabled={rightEnabled} onGutterDown={onGutterDown} />
    </div>
  );
});

interface PaneProps {
  cell: Cell | null;
  side: Side;
  ri: number;
  tokens: Tokens;
  selected: boolean;
  enabled: boolean;
  onGutterDown: (e: React.PointerEvent, side: Side, ri: number) => void;
}

function Pane({ cell, side, ri, tokens, selected, enabled, onGutterDown }: PaneProps) {
  const changed = isChanged(cell) && enabled;
  const kind = cell ? cell.kind : "empty";
  const cls = `${side} ${kind}${selected && cell ? " selected" : ""}`;
  return (
    <>
      <div
        className={`gutter ${cls}${changed ? " clickable" : ""}`}
        title={!enabled && isChanged(cell) ? (side === "old" ? "Left-side comments need a range that starts at the PR base" : "Right-side comments need a range that ends at the latest version") : undefined}
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
