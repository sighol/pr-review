import { buildRows, cellOn, parseUnifiedDiff, textToLines, type CellKind, type LineRange } from "./diff";

export interface SnippetLine {
  lineNo: number;
  text: string;
  kind: CellKind;
}

/** The lines a comment range covers on its side, with their diff kind. */
export function snippetFor(
  file: { oldText: string; newText: string; diffText: string },
  range: LineRange,
): SnippetLine[] {
  const rows = buildRows(textToLines(file.oldText), textToLines(file.newText), parseUnifiedDiff(file.diffText), {
    wholeFile: true,
    expandedGaps: new Set(),
  });
  const out: SnippetLine[] = [];
  for (const row of rows) {
    const cell = cellOn(row, range.side);
    if (cell && cell.lineNo >= range.start && cell.lineNo <= range.end) {
      out.push({ lineNo: cell.lineNo, text: cell.text, kind: cell.kind });
    }
  }
  return out;
}
