import { cellStatus, isFullyApproved, lastApproved } from "../lib/approval";
import { rangeFromColumns, rangeLabel } from "../lib/range";
import { useStore } from "../store";
import { useCommitDrag } from "./useCommitDrag";

function PathLabel({ path, oldPath }: { path: string; oldPath?: string }) {
  const i = path.lastIndexOf("/") + 1;
  return (
    <span className="mx-path" title={oldPath ? `${oldPath} → ${path}` : path}>
      <span className="mx-dir">{path.slice(0, i)}</span>
      <strong>{path.slice(i)}</strong>
    </span>
  );
}

export function FileMatrix() {
  const { files, commits, approvals, setCurrentFile, globalRange, setAllRanges } = useStore();
  const fullyApproved = files.filter((f) => isFullyApproved(f.changedBy, approvals.get(f.path))).length;
  const { drag, start } = useCommitDrag((a, b) => setAllRanges(rangeFromColumns(a, b)));
  const shown = drag ? rangeFromColumns(drag.a, drag.b) : globalRange;
  const inRange = (n: number) => !!shown && n > shown.from && n <= shown.to;

  return (
    <>
      <p className="muted">
        {fullyApproved} of {files.length} files approved · {commits.length} commits. Click a file to review it.
      </p>
      <p className="muted range-note">
        {globalRange ? (
          <>
            Every file opens at <strong>{rangeLabel(globalRange)}</strong>.{" "}
            <button className="link" onClick={() => setAllRanges(null)}>
              Clear
            </button>
          </>
        ) : (
          "Drag across the commit numbers to open every file at one commit range."
        )}
      </p>
      <div className="matrix" style={{ ["--n" as string]: commits.length }}>
        <div className="mx-row mx-head">
          <span />
          <span className="mx-file">FILE</span>
          <span className="mx-stat" />
          {commits.map((c) => (
            <span
              className={`mx-cell mx-pick${inRange(c.n) ? " in-range" : ""}`}
              key={c.n}
              data-n={c.n}
              title={`#${c.n} ${c.short} ${c.subject}`}
              onPointerDown={start(c.n)}
            >
              {c.n}
            </span>
          ))}
        </div>
        {files.map((f) => {
          const approved = approvals.get(f.path);
          const fully = isFullyApproved(f.changedBy, approved);
          const last = lastApproved(f.changedBy, approved);
          return (
            <button className="mx-row" key={f.path} onClick={() => setCurrentFile(f.path)}>
              <span className="mx-state">{fully && <span className="status-dot approved" />}</span>
              <span className="mx-file">
                <PathLabel path={f.path} oldPath={f.oldPath} />
              </span>
              <span className="mx-stat">
                <span className="add">{f.added}</span>/<span className="del">{f.removed}</span>
              </span>
              {commits.map((c) => {
                const status = cellStatus(f.changedBy, approved, c.n);
                const inSpan = fully || (last !== null && c.n <= last);
                return (
                  <span
                    className={`mx-cell ${status}${inSpan ? " span" : ""}${inRange(c.n) ? " in-range" : ""}`}
                    key={c.n}
                    data-n={c.n}
                    title={`#${c.n} ${c.short} ${c.subject}${status === "none" ? "" : ` · ${status}`}`}
                  >
                    {status !== "none" && <span className={`status-dot ${status}`} />}
                  </span>
                );
              })}
            </button>
          );
        })}
      </div>
    </>
  );
}
