import { useEffect, useMemo, useState } from "react";
import { textToLines } from "../lib/diff";
import { langFor } from "../lib/highlight";
import { asOfRange, rangeLabel, sameRange } from "../lib/range";
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
  const { resolutions, resolveThread, reopenThread, fileByName, rangeFor, viewAsOf } = useStore();
  const resolution = resolutions.get(thread.id);
  const resolved = isResolved(thread, resolution);
  const newFrom = firstNewIndex(thread, resolution);
  const nNew = newCount(thread, resolution);
  const [collapsed, setCollapsed] = useState(resolved && !expanded);
  const file = thread.file ? fileByName.get(thread.file) : undefined;
  const fileLang = file ? langFor(file.path) : null;
  const replaces = useMemo(
    () =>
      file && thread.side === "new" && thread.line !== null
        ? [textToLines(file.newText)[thread.line - 1] ?? ""]
        : null,
    [file, thread.side, thread.line],
  );
  const range = file ? rangeFor(file) : null;
  const asOf = asOfRange(thread);
  // Only offer the older view when the comment was made on an earlier version of the file,
  // and only when clicking would actually change the diff.
  const showAsOf =
    !!file && !!range && thread.commit > 0 && thread.commit < Math.max(...file.changedBy) && !sameRange(range, asOf);

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
        {showAsOf && (
          <button
            className="action small"
            title={`Show the diff as the reviewer saw it: ${rangeLabel(asOf)}`}
            onClick={() => viewAsOf(thread)}
          >
            View {rangeLabel(asOf)}
          </button>
        )}
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
              <Markdown text={c.body} fileLang={fileLang} replaces={replaces} />
            </div>
          </div>
        );
      })}
      <ReplyBox threadId={thread.id} fileLang={fileLang} replaces={replaces} preview={preview} />
    </div>
  );
}

export function ReplyBox({
  threadId,
  fileLang,
  replaces,
  preview,
}: {
  threadId: string;
  fileLang: string | null;
  replaces: string[] | null;
  preview: boolean;
}) {
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
            <Markdown text={text} fileLang={fileLang} replaces={replaces} />
          </div>
        )}
      </div>
    </div>
  );
}
