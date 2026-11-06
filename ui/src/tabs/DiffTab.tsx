import { useMemo, useState } from "react";
import { DiffView } from "../components/DiffView";
import { DiscussionNav } from "../components/DiscussionNav";
import { RangeBar } from "../components/RangeBar";
import { isFullyApproved, latestCommit, nextUnapproved } from "../lib/approval";
import { rangeKey, useRangeData } from "../lib/data";
import { commentable, defaultRange, placeable, rangeLabel, sameRange, stateOf, threadPlacement } from "../lib/range";
import { threadLine, useStore } from "../store";

export function DiffTab() {
  const {
    files,
    fileByName,
    commits,
    threads,
    currentFile,
    setCurrentFile,
    approvals,
    approveFile,
    unapproveFile,
    rangeFor,
    setRange,
    globalRange,
    jumpTo,
  } = useStore();
  const [wholeFile, setWholeFile] = useState(false);
  const file = fileByName.get(currentFile);
  const range = useMemo(() => (file ? rangeFor(file) : undefined), [file, rangeFor]);
  const data = useRangeData(file, range);

  if (!file || !range) return null;

  const headN = commits.length;
  const approved = approvals.get(file.path);
  const fully = isFullyApproved(file.changedBy, approved);
  const latest = latestCommit(file.changedBy);
  const next = nextUnapproved(files, approvals, file.path);
  const since = defaultRange(file.changedBy, approved, headN);
  const full = { from: 0, to: headN };
  const ok = commentable(range, file.changedBy);
  const ready = !!data && data.key === rangeKey(file, range);
  const unchanged = stateOf(file.changedBy, range.from) === stateOf(file.changedBy, range.to);
  const hidden = threads.filter(
    (t) => t.file === file.path && placeable(t) && threadPlacement(t, range, file.changedBy) === null,
  );

  const status = (path: string) => {
    const f = fileByName.get(path)!;
    return isFullyApproved(f.changedBy, approvals.get(path)) ? "✓" : "●";
  };

  const commentHint = ok.left && ok.right
    ? "Drag a line number on a changed line to comment"
    : !ok.left && !ok.right
      ? "Read-only: comments need a range that starts at the PR base (left) or ends at the latest version (right)"
      : ok.right
        ? "Right-side comments only: the left side needs a range that starts at the PR base"
        : "Left-side comments only: the right side needs a range that ends at the latest version";

  return (
    <>
      <div className="toolbar">
        <div className="toolbar-row">
          <select value={currentFile} onChange={(e) => setCurrentFile(e.target.value)}>
            {files.map((f) => (
              <option key={f.path} value={f.path}>
                {status(f.path)} {f.path}
              </option>
            ))}
          </select>
          <DiscussionNav />
          <span className="spacer" />
          <button onClick={() => setWholeFile((w) => !w)}>{wholeFile ? "Show changed hunks" : "Show whole file"}</button>
        </div>
        <div className="toolbar-row">
          <RangeBar
            commits={commits}
            changedBy={file.changedBy}
            approved={approved}
            range={range}
            onChange={(r) => setRange(file.path, r)}
          />
          <span className="range-label">
            {rangeLabel(range)}
            {globalRange && sameRange(globalRange, range) ? " · all files" : ""}
          </span>
          <button disabled={sameRange(range, since)} onClick={() => setRange(file.path, since)}>
            Since approval
          </button>
          <button disabled={sameRange(range, full)} onClick={() => setRange(file.path, full)}>
            Full PR diff
          </button>
        </div>
        <div className="toolbar-row">
          <span className={`hint${ok.left && ok.right ? "" : " warn"}`}>{commentHint}</span>
          <span className="spacer" />
          {fully ? (
            <button className="action approved" onClick={() => unapproveFile(file)}>
              ✓ Approved · Unapprove
            </button>
          ) : (
            <button className="action primary" onClick={() => approveFile(file)}>
              Approve through #{latest}
            </button>
          )}
          <button className="action" disabled={!next} onClick={() => next && setCurrentFile(next)}>
            Next unapproved file →
          </button>
        </div>
      </div>
      {hidden.length > 0 && (
        <div className="hidden-threads">
          {hidden.length} {hidden.length === 1 ? "thread" : "threads"} not shown in this range:
          {hidden.map((t) => (
            <button key={t.id} className="link" onClick={() => jumpTo(file.path, t.side, threadLine(t), t.id)}>
              {t.side === "old" ? "old " : ""}line {threadLine(t)}
              {t.line === null ? " (outdated)" : ""}
            </button>
          ))}
        </div>
      )}
      {ready && unchanged && (
        <div className="no-changes">No changes to this file in {rangeLabel(range)}.</div>
      )}
      {ready ? (
        <DiffView key={data.key} file={file} data={data} range={range} wholeFile={wholeFile} />
      ) : (
        <div className="loading">Loading…</div>
      )}
    </>
  );
}
