import { diffWordsWithSpace } from "diff";

export type Side = "old" | "new";
export type CellKind = "ctx" | "add" | "del";
export type Range = [start: number, end: number];

export interface DiffLine {
  kind: CellKind;
  text: string;
  noEol?: boolean;
}

export interface Hunk {
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  header: string;
  lines: DiffLine[];
}

export interface Cell {
  lineNo: number;
  text: string;
  kind: CellKind;
  noEol?: boolean;
  /** Changed-word ranges, only set on paired del/add cells. */
  ranges?: Range[];
}

export type Row =
  | { type: "line"; id: string; left: Cell | null; right: Cell | null; hunk: number }
  | { type: "gap"; id: string; oldStart: number; newStart: number; count: number };

export function textToLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

const HUNK_RE = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$/;

export function parseUnifiedDiff(diff: string): Hunk[] {
  const input = diff.split("\n");
  const hunks: Hunk[] = [];
  let i = 0;
  while (i < input.length) {
    const m = HUNK_RE.exec(input[i]);
    if (!m) {
      i++;
      continue;
    }
    const hunk: Hunk = {
      oldStart: Number(m[1]),
      oldCount: m[2] === undefined ? 1 : Number(m[2]),
      newStart: Number(m[3]),
      newCount: m[4] === undefined ? 1 : Number(m[4]),
      header: m[5].trim(),
      lines: [],
    };
    i++;
    let oldLeft = hunk.oldCount;
    let newLeft = hunk.newCount;
    while (i < input.length) {
      const line = input[i];
      if (line.startsWith("\\")) {
        const prev = hunk.lines[hunk.lines.length - 1];
        if (prev) prev.noEol = true;
        i++;
        continue;
      }
      if (oldLeft === 0 && newLeft === 0) break;
      const tag = line[0] ?? " ";
      const text = line.slice(1);
      if (tag === "+") {
        hunk.lines.push({ kind: "add", text });
        newLeft--;
      } else if (tag === "-") {
        hunk.lines.push({ kind: "del", text });
        oldLeft--;
      } else {
        hunk.lines.push({ kind: "ctx", text });
        oldLeft--;
        newLeft--;
      }
      i++;
    }
    hunks.push(hunk);
  }
  return hunks;
}

export function mergeRanges(ranges: Range[]): Range[] {
  const out: Range[] = [];
  for (const r of ranges) {
    const last = out[out.length - 1];
    if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else out.push([r[0], r[1]]);
  }
  return out;
}

export function wordRanges(oldText: string, newText: string): { old: Range[]; new: Range[] } {
  const old: Range[] = [];
  const nu: Range[] = [];
  let o = 0;
  let n = 0;
  for (const part of diffWordsWithSpace(oldText, newText)) {
    const len = part.value.length;
    if (part.removed) {
      old.push([o, o + len]);
      o += len;
    } else if (part.added) {
      nu.push([n, n + len]);
      n += len;
    } else {
      o += len;
      n += len;
    }
  }
  return { old: mergeRanges(old), new: mergeRanges(nu) };
}

export interface BuildOptions {
  wholeFile: boolean;
  expandedGaps: ReadonlySet<string>;
}

export function buildRows(
  oldLines: string[],
  newLines: string[],
  hunks: Hunk[],
  { wholeFile, expandedGaps }: BuildOptions,
): Row[] {
  const rows: Row[] = [];
  let oldPos = 1;
  let newPos = 1;

  const emitGap = (count: number) => {
    if (count <= 0) return;
    const id = `gap-${oldPos}`;
    if (wholeFile || expandedGaps.has(id)) {
      for (let k = 0; k < count; k++) {
        const o = oldPos + k;
        const n = newPos + k;
        rows.push({
          type: "line",
          id: `l${o}:${n}`,
          left: { lineNo: o, text: oldLines[o - 1] ?? "", kind: "ctx" },
          right: { lineNo: n, text: newLines[n - 1] ?? "", kind: "ctx" },
          hunk: -1,
        });
      }
    } else {
      rows.push({ type: "gap", id, oldStart: oldPos, newStart: newPos, count });
    }
  };

  hunks.forEach((h, hi) => {
    const oldBefore = h.oldCount === 0 ? h.oldStart : h.oldStart - 1;
    emitGap(oldBefore - (oldPos - 1));
    oldPos = oldBefore + 1;
    newPos = (h.newCount === 0 ? h.newStart : h.newStart - 1) + 1;

    let dels: DiffLine[] = [];
    let adds: DiffLine[] = [];
    let delNo = oldPos;
    let addNo = newPos;

    const flush = () => {
      const n = Math.max(dels.length, adds.length);
      for (let k = 0; k < n; k++) {
        const d = dels[k];
        const a = adds[k];
        const left: Cell | null = d ? { lineNo: delNo + k, text: d.text, kind: "del", noEol: d.noEol } : null;
        const right: Cell | null = a ? { lineNo: addNo + k, text: a.text, kind: "add", noEol: a.noEol } : null;
        if (left && right) {
          const w = wordRanges(left.text, right.text);
          left.ranges = w.old;
          right.ranges = w.new;
        }
        rows.push({
          type: "line",
          id: `l${left?.lineNo ?? "-"}:${right?.lineNo ?? "-"}`,
          left,
          right,
          hunk: hi,
        });
      }
      delNo += dels.length;
      addNo += adds.length;
      dels = [];
      adds = [];
    };

    for (const line of h.lines) {
      if (line.kind === "ctx") {
        flush();
        rows.push({
          type: "line",
          id: `l${delNo}:${addNo}`,
          left: { lineNo: delNo, text: line.text, kind: "ctx", noEol: line.noEol },
          right: { lineNo: addNo, text: line.text, kind: "ctx", noEol: line.noEol },
          hunk: hi,
        });
        delNo++;
        addNo++;
      } else if (line.kind === "del") {
        if (adds.length > 0) flush();
        dels.push(line);
      } else {
        adds.push(line);
      }
    }
    flush();
    oldPos = delNo;
    newPos = addNo;
  });

  emitGap(oldLines.length - (oldPos - 1));
  return rows;
}

export function cellOn(row: Row, side: Side): Cell | null {
  if (row.type !== "line") return null;
  return side === "old" ? row.left : row.right;
}

export function isChanged(cell: Cell | null): boolean {
  return cell !== null && cell.kind !== "ctx";
}

export interface DragSelection {
  side: Side;
  anchor: number;
  end: number;
}

/**
 * Clamps a drag from `anchor` toward `pointer` so the range stays inside the
 * anchor's hunk (no gaps, no rows outside any hunk) and ends on a changed
 * line on the dragged side. Returns the inclusive row-index range.
 */
export function clampDrag(rows: Row[], side: Side, anchor: number, pointer: number): [number, number] {
  const anchorRow = rows[anchor];
  if (!anchorRow || anchorRow.type !== "line") return [anchor, anchor];
  const dir = pointer >= anchor ? 1 : -1;
  let reach = anchor;
  for (let i = anchor + dir; dir > 0 ? i <= pointer : i >= pointer; i += dir) {
    const r = rows[i];
    if (!r || r.type !== "line" || r.hunk !== anchorRow.hunk) break;
    reach = i;
  }
  while (reach !== anchor && !isChanged(cellOn(rows[reach], side))) reach -= dir;
  return dir > 0 ? [anchor, reach] : [reach, anchor];
}

export interface LineRange {
  side: Side;
  start: number;
  end: number;
}

export function rangeFromRows(rows: Row[], side: Side, lo: number, hi: number): LineRange | null {
  let start = Infinity;
  let end = -Infinity;
  for (let i = lo; i <= hi; i++) {
    const c = cellOn(rows[i], side);
    if (c) {
      start = Math.min(start, c.lineNo);
      end = Math.max(end, c.lineNo);
    }
  }
  return end < 0 ? null : { side, start, end };
}

export const rangeKey = (r: LineRange) => `${r.side}:${r.start}-${r.end}`;

export const SUGGESTION_OPEN = "```suggestion\n";

/** A suggestion block holding the range's current new-side code; "" on the old side. */
export function suggestionBlock(range: LineRange, newLines: string[]): string {
  if (range.side !== "new") return "";
  const body = newLines.slice(range.start - 1, range.end).join("\n");
  return `${SUGGESTION_OPEN}${body}\n\`\`\``;
}

export interface Segment {
  text: string;
  color?: string;
  changed: boolean;
}

export interface TokenLike {
  content: string;
  color?: string;
}

/** Splits syntax tokens at changed-word boundaries. */
export function segmentsFor(tokens: TokenLike[] | undefined, text: string, ranges: Range[] | undefined): Segment[] {
  const toks: TokenLike[] = tokens && tokens.length > 0 ? tokens : [{ content: text }];
  const rs = ranges ?? [];
  const out: Segment[] = [];
  let pos = 0;
  for (const tok of toks) {
    const end = pos + tok.content.length;
    let cur = pos;
    for (const [rs0, rs1] of rs) {
      if (rs1 <= cur || rs0 >= end) continue;
      const a = Math.max(rs0, cur);
      const b = Math.min(rs1, end);
      if (a > cur) out.push({ text: tok.content.slice(cur - pos, a - pos), color: tok.color, changed: false });
      out.push({ text: tok.content.slice(a - pos, b - pos), color: tok.color, changed: true });
      cur = b;
    }
    if (cur < end) out.push({ text: tok.content.slice(cur - pos), color: tok.color, changed: false });
    pos = end;
  }
  return out;
}
