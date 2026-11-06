import { cellStatus, isFullyApproved, lastApproved } from "../lib/approval";
import { useStore } from "../store";

function PathLabel({ path }: { path: string }) {
  const i = path.lastIndexOf("/") + 1;
  return (
    <span className="mx-path" title={path}>
      <span className="mx-dir">{path.slice(0, i)}</span>
      <strong>{path.slice(i)}</strong>
    </span>
  );
}

export function FileMatrix() {
  const { files, commits, approvals, setCurrentFile } = useStore();
  const fullyApproved = files.filter((f) => isFullyApproved(f.changedBy, approvals.get(f.path))).length;

  return (
    <>
      <p className="muted">
        {fullyApproved} of {files.length} files approved · {commits.length} commits. Click a file to review it.
      </p>
      <div className="matrix" style={{ ["--n" as string]: commits.length }}>
        <div className="mx-row mx-head">
          <span />
          <span className="mx-file">FILE</span>
          <span className="mx-stat" />
          {commits.map((c) => (
            <span className="mx-cell" key={c.n} title={`#${c.n} ${c.short} ${c.subject}`}>
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
              <span className={`mx-state ${fully ? "done" : ""}`}>{fully ? "✓" : ""}</span>
              <span className="mx-file">
                <PathLabel path={f.path} />
              </span>
              <span className="mx-stat">
                <span className="add">{f.added}</span>/<span className="del">{f.removed}</span>
              </span>
              {commits.map((c) => {
                const status = cellStatus(f.changedBy, approved, c.n);
                const inSpan = fully || (last !== null && c.n <= last);
                return (
                  <span
                    className={`mx-cell ${status}${inSpan ? " span" : ""}`}
                    key={c.n}
                    title={`#${c.n} ${c.short} ${c.subject}${status === "none" ? "" : ` · ${status}`}`}
                  >
                    {status === "approved" ? "✓" : status === "unapproved" ? "●" : ""}
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
