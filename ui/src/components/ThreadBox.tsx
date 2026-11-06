import { useEffect, useState } from "react";
import { langFor } from "../lib/highlight";
import { firstNewIndex, isResolved, newCount, type FakeThread } from "../lib/threads";
import { useStore } from "../store";
import { Markdown } from "./Markdown";

interface Props {
  thread: FakeThread;
  /** Never collapse, even when resolved (used on the Review tab). */
  expanded?: boolean;
  /** Show a rendered preview under reply drafts. */
  preview?: boolean;
}

export function ThreadBox({ thread, expanded = false, preview = false }: Props) {
  const { resolutions, resolveThread, reopenThread, fileByName } = useStore();
  const resolution = resolutions.get(thread.id);
  const resolved = isResolved(thread, resolution);
  const newFrom = firstNewIndex(thread, resolution);
  const nNew = newCount(thread, resolution);
  const [collapsed, setCollapsed] = useState(resolved && !expanded);
  const fileLang = thread.file ? langFor(fileByName.get(thread.file)?.path ?? "") : null;

  useEffect(() => {
    setCollapsed(resolved && !expanded);
  }, [resolved, expanded]);

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
        {nNew > 0 && <span className="badge-new">{nNew} new</span>}
        <span className="gh-status" title="GitHub's own resolved flag">
          GitHub: {thread.githubResolved ? "resolved" : "unresolved"}
        </span>
        <span className="spacer" />
        {resolved ? (
          <button className="action" onClick={() => reopenThread(thread)}>
            Reopen
          </button>
        ) : (
          <button className="action primary" onClick={() => resolveThread(thread)}>
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
              <Markdown text={c.body} fileLang={fileLang} />
            </div>
          </div>
        );
      })}
      <ReplyBox threadId={thread.id} fileLang={fileLang} preview={preview} />
    </div>
  );
}

export function ReplyBox({ threadId, fileLang, preview }: { threadId: string; fileLang: string | null; preview: boolean }) {
  const { replies, setReply } = useStore();
  const text = replies.get(threadId);

  if (text === undefined) {
    return (
      <button className="link reply" onClick={() => setReply(threadId, "")}>
        Reply…
      </button>
    );
  }
  return (
    <div className="reply-draft">
      <div className="draft-tag" aria-hidden>
        DRAFT
      </div>
      <div className="reply-main">
        <button className="link reply-delete" onClick={() => setReply(threadId, null)}>
          delete
        </button>
        <textarea
          autoFocus={text === ""}
          className="reply-box"
          value={text}
          placeholder="Reply"
          onChange={(e) => setReply(threadId, e.target.value)}
          onBlur={() => text.trim() === "" && setReply(threadId, null)}
        />
        {preview && text.trim() !== "" && (
          <div className="preview">
            <div className="preview-label">Preview</div>
            <Markdown text={text} fileLang={fileLang} />
          </div>
        )}
      </div>
    </div>
  );
}
