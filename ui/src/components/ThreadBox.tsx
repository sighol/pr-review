import { useEffect, useMemo, useRef, useState } from "react";
import { langFor } from "../lib/highlight";
import { asOfRange, rangeLabel } from "../lib/range";
import { firstNewIndex, isResolved, newCount, replacedLines, type FakeThread } from "../lib/threads";
import { fullDateTime } from "../lib/time";
import { useStore } from "../store";
import { ExternalLink } from "./ExternalLink";
import { Markdown } from "./Markdown";

interface Props {
  thread: FakeThread;
  /** Never collapse, even when resolved (used on the Review tab). */
  expanded?: boolean;
  /** Show the write/preview toggle under the reply draft (Review tab only). */
  preview?: boolean;
  /** Set when the thread's code has changed and it is shown next to this (closest matching) line. */
  outdatedAt?: number;
  /** Bumped by "Preview all" on the Review tab to switch the reply draft to preview mode. */
  previewAllTick?: number;
}

export function ThreadBox({ thread, expanded = false, preview = false, outdatedAt, previewAllTick }: Props) {
  const { resolutions, resolveThread, reopenThread, fileByName, rangeFor, viewAsOf } = useStore();
  const resolution = resolutions.get(thread.id);
  const resolved = isResolved(thread, resolution);
  const newFrom = firstNewIndex(thread, resolution);
  const nNew = newCount(thread, resolution);
  const [collapsed, setCollapsed] = useState(resolved && !expanded);
  const file = thread.file ? fileByName.get(thread.file) : undefined;
  const fileLang = file ? langFor(file.path) : null;
  const replaces = useMemo(
    () => (file ? replacedLines(thread, file.newText) : null),
    [file, thread.side, thread.line, thread.startLine],
  );
  const range = file ? rangeFor(file) : null;
  const asOf = asOfRange(thread);
  // The comment's commit link only matters when it differs from what the range is currently showing.
  const showCommitLink = !!file && !!range && thread.commit > 0 && thread.commit !== range.to;

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
    <div className={`comment thread ${resolved ? "is-resolved" : "is-open"}${outdatedAt !== undefined ? " is-outdated" : ""}`}>
      {outdatedAt !== undefined && (
        <div className="outdated-note">
          <strong>Outdated</strong> · written on #{thread.commit} at line {thread.originalLine}; that code has changed, so
          this is shown at the closest matching line ({outdatedAt}).
        </div>
      )}
      <div className="thread-head">
        <span className={`status ${resolved ? "resolved" : "open"}`}>{resolved ? "✓ Resolved" : "● Open"}</span>
        {nNew > 0 && <span className="badge-new">{nNew} new</span>}
        <span className="gh-status" title="GitHub's own resolved flag">
          GitHub: {thread.githubResolved ? "resolved" : "unresolved"}
        </span>
        <span className="spacer" />
        {resolved && !expanded && (
          <button className="action" onClick={() => setCollapsed(true)}>
            Hide
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
          <div key={c.id} className="thread-item">
            {i === newFrom && <div className="new-divider">New since you resolved</div>}
            <div className={`thread-comment${isNew ? " new" : ""}`}>
              <div className="comment-head">
                <span>
                  <strong>{c.author}</strong> ·{" "}
                  {c.url ? (
                    <ExternalLink
                      href={c.url}
                      title={c.createdAt ? fullDateTime(c.createdAt) : "Open this comment on GitHub"}
                    >
                      {c.when}
                    </ExternalLink>
                  ) : (
                    c.when
                  )}
                  {i === 0 && showCommitLink && (
                    <>
                      {" · "}
                      <button
                        className="link"
                        title={`Show the diff as the reviewer saw it: ${rangeLabel(asOf)}`}
                        onClick={() => viewAsOf(thread)}
                      >
                        #{thread.commit}
                      </button>
                    </>
                  )}
                  {isNew && <span className="badge-new inline">NEW</span>}
                </span>
              </div>
              <Markdown text={c.body} fileLang={fileLang} replaces={replaces} />
            </div>
          </div>
        );
      })}
      <ReplyBox
        threadId={thread.id}
        fileLang={fileLang}
        replaces={replaces}
        preview={preview}
        previewAllTick={previewAllTick}
      />
    </div>
  );
}

export function ReplyBox({
  threadId,
  fileLang,
  replaces,
  preview = false,
  previewAllTick,
}: {
  threadId: string;
  fileLang: string | null;
  replaces: string[] | null;
  preview?: boolean;
  previewAllTick?: number;
}) {
  const { replies, setReply } = useStore();
  const text = replies.get(threadId);
  const [mode, setMode] = useState<"write" | "preview">("write");
  const lastPreviewAllTick = useRef(previewAllTick);

  useEffect(() => {
    if (previewAllTick !== undefined && previewAllTick !== lastPreviewAllTick.current) {
      lastPreviewAllTick.current = previewAllTick;
      setMode("preview");
    }
  }, [previewAllTick]);

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
        {preview && (
          <div className="mode-toggle">
            <button className={`mode-tab${mode === "write" ? " active" : ""}`} onClick={() => setMode("write")}>
              Write
            </button>
            <button className={`mode-tab${mode === "preview" ? " active" : ""}`} onClick={() => setMode("preview")}>
              Preview
            </button>
          </div>
        )}
        <textarea
          autoFocus={text === ""}
          className="reply-box"
          value={text}
          placeholder="Reply"
          onChange={(e) => setReply(threadId, e.target.value)}
          onBlur={() => text.trim() === "" && setReply(threadId, null)}
          style={preview && mode === "preview" ? { display: "none" } : undefined}
        />
        {preview && mode === "preview" && (
          <div className="preview">
            {text.trim() !== "" ? (
              <Markdown text={text} fileLang={fileLang} replaces={replaces} />
            ) : (
              <div className="muted">Nothing to preview yet.</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
