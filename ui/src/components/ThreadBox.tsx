import { useState } from "react";
import type { FakeThread } from "../lib/threads";

export function ThreadBox({ thread }: { thread: FakeThread }) {
  const [collapsed, setCollapsed] = useState(thread.resolved);
  const [reply, setReply] = useState<string | null>(null);

  if (collapsed) {
    return (
      <div className="comment thread resolved">
        <button className="link" onClick={() => setCollapsed(false)}>
          ✓ Resolved thread · {thread.comments.length} comments · show
        </button>
      </div>
    );
  }

  return (
    <div className={`comment thread${thread.resolved ? " resolved" : ""}`}>
      {thread.comments.map((c, i) => (
        <div className="thread-comment" key={i}>
          <div className="comment-head">
            <strong>{c.author}</strong>
            {i === 0 && thread.resolved && (
              <button className="link" onClick={() => setCollapsed(true)}>
                hide resolved
              </button>
            )}
          </div>
          <div className="body">{c.body}</div>
        </div>
      ))}
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
