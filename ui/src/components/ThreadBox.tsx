import { useEffect, useState } from "react";
import { firstNewIndex, isResolved, type FakeThread, type Resolution } from "../lib/threads";

interface Props {
  thread: FakeThread;
  resolution: Resolution | undefined;
  onResolve: (thread: FakeThread) => void;
  onReopen: (thread: FakeThread) => void;
}

export function ThreadBox({ thread, resolution, onResolve, onReopen }: Props) {
  const resolved = isResolved(thread, resolution);
  const newFrom = firstNewIndex(thread, resolution);
  const newCount = newFrom === null ? 0 : thread.comments.length - newFrom;
  const [collapsed, setCollapsed] = useState(resolved);
  const [reply, setReply] = useState<string | null>(null);

  useEffect(() => {
    setCollapsed(resolved);
  }, [resolved]);

  if (collapsed) {
    return (
      <div className="comment thread is-resolved">
        <button className="link" onClick={() => setCollapsed(false)}>
          ✓ Resolved · {thread.comments.length} comments · show
        </button>
      </div>
    );
  }

  return (
    <div className={`comment thread ${resolved ? "is-resolved" : "is-open"}`}>
      <div className="thread-head">
        <span className={`status ${resolved ? "resolved" : "open"}`}>{resolved ? "✓ Resolved" : "● Open"}</span>
        {newCount > 0 && <span className="badge-new">{newCount} new</span>}
        <span className="gh-status" title="GitHub's own resolved flag">
          GitHub: {thread.githubResolved ? "resolved" : "unresolved"}
        </span>
        <span className="spacer" />
        {resolved ? (
          <button className="action" onClick={() => onReopen(thread)}>
            Reopen
          </button>
        ) : (
          <button className="action primary" onClick={() => onResolve(thread)}>
            Resolve
          </button>
        )}
      </div>
      {thread.comments.map((c, i) => {
        const isNew = newFrom !== null && i >= newFrom;
        return (
          <div key={c.id}>
            {i === newFrom && <div className="new-divider">New since you resolved</div>}
            <div className={`thread-comment${isNew ? " new" : ""}`}>
              <div className="comment-head">
                <span>
                  <strong>{c.author}</strong> · {c.when}
                  {isNew && <span className="badge-new inline">NEW</span>}
                </span>
              </div>
              <div className="body">{c.body}</div>
            </div>
          </div>
        );
      })}
      {reply === null ? (
        <button className="link reply" onClick={() => setReply("")}>
          Reply…
        </button>
      ) : (
        <textarea
          autoFocus
          className="reply-box"
          value={reply}
          placeholder="Reply"
          onChange={(e) => setReply(e.target.value)}
          onBlur={() => reply.trim() === "" && setReply(null)}
        />
      )}
    </div>
  );
}
