import { cellStatus } from "../lib/approval";
import { rangeFromColumns, type CommitRange } from "../lib/range";
import type { Commit } from "../store";
import { useCommitDrag } from "./useCommitDrag";

interface Props {
  commits: Commit[];
  changedBy: number[];
  approved: ReadonlySet<number> | undefined;
  range: CommitRange;
  onChange: (range: CommitRange) => void;
}

/** A single-row commit matrix for one file; drag across commits to pick the range to diff. */
export function RangeBar({ commits, changedBy, approved, range, onChange }: Props) {
  const { drag, start } = useCommitDrag((a, b) => onChange(rangeFromColumns(a, b)));
  const shown = drag ? rangeFromColumns(drag.a, drag.b) : range;

  return (
    <div className="range-bar" style={{ ["--n" as string]: commits.length }} title="Drag to choose which commits to diff">
      {commits.map((c) => {
        const status = cellStatus(changedBy, approved, c.n);
        const selected = c.n > shown.from && c.n <= shown.to;
        return (
          <span
            key={c.n}
            data-n={c.n}
            className={`rb-cell ${status}${selected ? " selected" : ""}`}
            title={`#${c.n} ${c.short} ${c.subject}${status === "none" ? " (doesn't touch this file)" : ` · ${status}`}`}
            onPointerDown={start(c.n)}
          >
            <span className="rb-n">{c.n}</span>
            <span className="rb-glyph">{status !== "none" && <span className={`status-dot ${status}`} />}</span>
          </span>
        );
      })}
    </div>
  );
}
