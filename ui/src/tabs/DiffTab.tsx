import { useEffect, useState } from "react";
import { DiffView } from "../components/DiffView";
import {
  diffFrom,
  isFullyApproved,
  lastApproved,
  latestCommit,
  nextUnapproved,
} from "../lib/approval";
import { useStore } from "../store";

export function DiffTab() {
  const { files, fileByName, currentFile, setCurrentFile, approvals, approveFile, unapproveFile, jump } = useStore();
  const [wholeFile, setWholeFile] = useState(false);
  const [fullDiffFile, setFullDiffFile] = useState<string | null>(null);
  const file = fileByName.get(currentFile);

  // Old-side comments only exist against the PR base, so jumping to one needs the full diff.
  const wantsFull = !!jump && jump.file === currentFile && jump.side === "old";
  useEffect(() => {
    if (wantsFull) setFullDiffFile(currentFile);
  }, [wantsFull, currentFile]);

  if (!file) return null;

  const fullDiff = fullDiffFile === file.path || wantsFull;
  const approved = approvals.get(file.path);
  const fully = isFullyApproved(file.changedBy, approved);
  const last = lastApproved(file.changedBy, approved);
  const latest = latestCommit(file.changedBy);
  const wanted = diffFrom(file.changedBy, approved, fullDiff);
  const from = file.versions.has(wanted) ? wanted : 0;
  const version = file.versions.get(from)!;
  const partial = from !== 0;
  const next = nextUnapproved(files, approvals, file.path);
  const canToggle = last !== null && !fully;

  const status = (path: string) => {
    const f = fileByName.get(path)!;
    return isFullyApproved(f.changedBy, approvals.get(path)) ? "✓" : "●";
  };

  const banner = partial
    ? `Changes since you approved through #${from} (commits #${file.changedBy.find((c) => c > from)}–#${latest})`
    : fully
      ? `Approved through #${latest} · showing the full PR diff`
      : "Full PR diff";

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
          <span className={`range-banner${partial ? " partial" : ""}`}>{banner}</span>
          <span className="spacer" />
          {canToggle && (
            <button onClick={() => setFullDiffFile((f) => (f === file.path ? null : file.path))}>
              {partial ? "Show full PR diff" : `Show since #${last}`}
            </button>
          )}
          <button onClick={() => setWholeFile((w) => !w)}>{wholeFile ? "Show changed hunks" : "Show whole file"}</button>
        </div>
        <div className="toolbar-row">
          <span className="hint">
            {partial
              ? "Drag a right-side line number to comment (left side needs the full PR diff)"
              : "Drag a line number on a changed line to comment"}
          </span>
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
      <DiffView key={`${file.name}:${from}`} file={file} version={version} partial={partial} wholeFile={wholeFile} />
    </>
  );
}
