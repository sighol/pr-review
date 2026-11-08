import { useEffect, useMemo, useState, type PointerEvent as ReactPointerEvent } from "react";
import { DiffView } from "../components/DiffView";
import { DiscussionNav } from "../components/DiscussionNav";
import { FileTree, type FileStatus } from "../components/FileTree";
import { RangeBar } from "../components/RangeBar";
import { isFullyApproved, latestCommit, nextUnapproved } from "../lib/approval";
import { needsAnchor } from "../lib/anchor";
import { rangeKey, useRangeData, useThreadAnchors } from "../lib/data";
import { commentable, placeable, rangeLabel, sameRange, stateOf, threadPlacement } from "../lib/range";
import { isResolved } from "../lib/threads";
import { threadLine, useStore } from "../store";

const MIN_SIDEBAR_WIDTH = 160;
const MAX_SIDEBAR_WIDTH = 560;

export function DiffTab() {
  const {
    files,
    fileByName,
    commits,
    threads,
    resolutions,
    currentFile,
    setCurrentFile,
    approvals,
    approveFile,
    unapproveFile,
    rangeFor,
    setRange,
    globalRange,
    jumpTo,
    tab,
  } = useStore();
  const [wholeFile, setWholeFile] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const file = fileByName.get(currentFile);
  const range = useMemo(() => (file ? rangeFor(file) : undefined), [file, rangeFor]);
  const wholeRange = { from: 0, to: commits.length };
  const data = useRangeData(file, range);
  const anchors = useThreadAnchors(file, range, data, threads);

  useEffect(() => {
    if (tab !== "diff" || !file) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
      if (e.key !== "a") return;
      e.preventDefault();
      if (isFullyApproved(file.changedBy, approvals.get(file.path))) {
        unapproveFile(file);
      } else {
        approveFile(file);
        const next = nextUnapproved(files, approvals, file.path);
        if (next) setCurrentFile(next);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab, file, approvals, files, approveFile, unapproveFile, setCurrentFile]);

  if (!file || !range) return null;

  const approved = approvals.get(file.path);
  const fully = isFullyApproved(file.changedBy, approved);
  const latest = latestCommit(file.changedBy);
  const next = nextUnapproved(files, approvals, file.path);
  const ok = commentable(range, file.changedBy);
  const ready = !!data && data.key === rangeKey(file, range);
  const unchanged = stateOf(file.changedBy, range.from) === stateOf(file.changedBy, range.to);
  const hidden = threads.filter(
    (t) =>
      t.file === file.path &&
      placeable(t) &&
      threadPlacement(t, range, file.changedBy) === null &&
      !(needsAnchor(t, range, file.changedBy) && (anchors === null || anchors.has(t.id))),
  );

  const startSidebarResize = (e: ReactPointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidth;
    const onMove = (ev: PointerEvent) => {
      setSidebarWidth(Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, startWidth + (ev.clientX - startX))));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const statusFor = (path: string): FileStatus => {
    const f = fileByName.get(path)!;
    const fileThreads = threads.filter((t) => t.file === path);
    return {
      approved: isFullyApproved(f.changedBy, approvals.get(path)),
      resolved: fileThreads.filter((t) => isResolved(t, resolutions.get(t.id))).length,
      total: fileThreads.length,
    };
  };

  const commentHint = ok.left && ok.right
    ? "Drag a line number on a changed line to comment"
    : !ok.left && !ok.right
      ? "Read-only: comments need a range that starts at the PR base (left) or ends at the latest version (right)"
      : ok.right
        ? "Right-side comments only: the left side needs a range that starts at the PR base"
        : "Left-side comments only: the right side needs a range that ends at the latest version";

  return (
    <div className="diff-layout">
      <FileTree
        paths={files.map((f) => f.path)}
        currentFile={currentFile}
        onSelect={setCurrentFile}
        statusFor={statusFor}
        renamedFrom={new Map(files.flatMap((f) => (f.oldPath ? [[f.path, f.oldPath] as const] : [])))}
        width={sidebarWidth}
      />
      <div className="file-tree-resize" onPointerDown={startSidebarResize} />
      <div className="diff-main">
        <div className="toolbar">
          <div className="toolbar-row">
            <DiscussionNav />
            <span className="spacer" />
            <button onClick={() => setWholeFile((w) => !w)}>
              {wholeFile ? "Show changed hunks" : "Show whole file"}
            </button>
          </div>
          {file.oldPath && (
            <div className="toolbar-row">
              <span className="hint">Renamed from {file.oldPath}</span>
            </div>
          )}
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
            {!sameRange(range, wholeRange) && (
              <a
                className="link"
                href="#"
                onClick={(e) => {
                  e.preventDefault();
                  setRange(file.path, wholeRange);
                }}
              >
                Clear
              </a>
            )}
          </div>
          <div className="toolbar-row">
            <span className={`hint${ok.left && ok.right ? "" : " warn"}`}>{commentHint}</span>
            <span className="spacer" />
            {fully ? (
              <button className="action approved" title="Unapprove (a)" onClick={() => unapproveFile(file)}>
                <span className="status-dot approved" /> Approved · Unapprove
              </button>
            ) : (
              <button
                className="action primary"
                title={(next ? `Then open ${next}` : "This is the last unapproved file") + " (a)"}
                onClick={() => {
                  approveFile(file);
                  if (next) setCurrentFile(next);
                }}
              >
                Approve through #{latest}
              </button>
            )}
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
        {ready && unchanged && <div className="no-changes">No changes to this file in {rangeLabel(range)}.</div>}
        {ready ? (
          <DiffView key={data.key} file={file} data={data} range={range} wholeFile={wholeFile} anchors={anchors} />
        ) : (
          <div className="loading">Loading…</div>
        )}
      </div>
    </div>
  );
}
