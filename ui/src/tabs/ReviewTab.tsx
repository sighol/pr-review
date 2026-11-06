import { useMemo, useState } from "react";
import { CommentBox, describeRange } from "../components/CommentBox";
import { ThreadBox } from "../components/ThreadBox";
import { CONVERSATION_ID, isResolved, newCount, type FakeThread } from "../lib/threads";
import { useRangeData } from "../lib/data";
import { snippetFor } from "../lib/snippet";
import { isRealDraft, pendingReplies, threadLine, useStore, type Draft, type FileData } from "../store";

const MAX_SNIPPET = 10;

type Event = "APPROVE" | "COMMENT" | "REQUEST_CHANGES";

function Snippet({ file, draft }: { file: FileData; draft: Draft }) {
  const { commits } = useStore();
  const full = useMemo(() => ({ from: 0, to: commits.length }), [commits.length]);
  const data = useRangeData(file, full);
  const lines = useMemo(() => (data ? snippetFor(data, draft.range) : []), [data, draft.range]);
  const shown = lines.slice(0, MAX_SNIPPET);
  return (
    <div className="snippet">
      {shown.map((l) => (
        <div className={`snip-line ${l.kind}`} key={l.lineNo}>
          <span className="snip-no">{l.lineNo}</span>
          <span className="snip-text">{l.text || "​"}</span>
        </div>
      ))}
      {lines.length > shown.length && <div className="snip-more">… {lines.length - shown.length} more lines</div>}
    </div>
  );
}

function ThreadStatus({ thread }: { thread: FakeThread }) {
  const { resolutions } = useStore();
  const r = resolutions.get(thread.id);
  const resolved = isResolved(thread, r);
  const n = newCount(thread, r);
  return (
    <>
      <span className={`status ${resolved ? "resolved" : "open"}`}>{resolved ? "✓ Resolved" : "● Open"}</span>
      {n > 0 && <span className="badge-new">{n} new</span>}
    </>
  );
}

export function useReviewPayload() {
  const { drafts, replies, fileByName } = useStore();
  return useMemo(() => {
    const comments = [...drafts.values()].filter(isRealDraft).map((d) => ({
      path: fileByName.get(d.file)?.path ?? d.file,
      side: d.range.side === "new" ? "RIGHT" : "LEFT",
      ...(d.range.start !== d.range.end
        ? { start_line: d.range.start, start_side: d.range.side === "new" ? "RIGHT" : "LEFT" }
        : {}),
      line: d.range.end,
      body: d.text,
    }));
    const threadReplies = pendingReplies(replies).map(([thread_id, body]) => ({ thread_id, body }));
    return { comments, replies: threadReplies };
  }, [drafts, replies, fileByName]);
}

export function ReviewTab() {
  const { files, fileByName, threads, drafts, replies, jumpTo, reviewBody, setReviewBody } = useStore();
  const payload = useReviewPayload();
  const pending = payload.comments.length + payload.replies.length;
  const [submitted, setSubmitted] = useState<{ event: Event; payload: unknown } | null>(null);

  const groups = useMemo(() => {
    const out: { key: string; title: string; draftItems: Draft[]; replyThreads: typeof threads }[] = [];
    for (const f of files) {
      const draftItems = [...drafts.values()]
        .filter((d) => d.file === f.name)
        .sort((a, b) => a.range.start - b.range.start);
      const replyThreads = threads.filter((t) => t.file === f.name && replies.has(t.id));
      if (draftItems.length || replyThreads.length) out.push({ key: f.name, title: f.path, draftItems, replyThreads });
    }
    const conv = threads.filter((t) => t.file === null && replies.has(t.id));
    if (conv.length) out.push({ key: "conversation", title: "Conversation", draftItems: [], replyThreads: conv });
    return out;
  }, [files, drafts, replies, threads]);

  const body = reviewBody.trim();
  const canSubmit: Record<Event, boolean> = {
    APPROVE: true,
    COMMENT: body !== "" || pending > 0,
    REQUEST_CHANGES: body !== "",
  };
  const submit = (event: Event) => setSubmitted({ event, payload: { event, body: reviewBody, ...payload } });
  const label = (name: string) => (pending > 0 ? `${name} · ${pending} comment${pending === 1 ? "" : "s"}` : name);

  return (
    <div className="review">
      <div className="page">
        <h1>Review</h1>
        <p className="muted">
          {pending === 0
            ? "You have no pending comments."
            : `${pending} pending comment${pending === 1 ? "" : "s"} will be sent with your review.`}
        </p>

        {groups.map((g) => (
          <section key={g.key} className="review-group">
            <h2>{g.title}</h2>
            {g.draftItems.map((d) => {
              const file = fileByName.get(d.file)!;
              return (
                <div className="review-item" key={d.key}>
                  <div className="review-item-head">
                    <span className="loc">
                      {file.path} · {describeRange(d.range)}
                    </span>
                    <span className="status new-thread">New thread</span>
                    <span className="spacer" />
                    <button className="link" onClick={() => jumpTo(d.file, d.range.side, d.range.end)}>
                      Jump to line →
                    </button>
                  </div>
                  <Snippet file={file} draft={d} />
                  <CommentBox draft={d} preview />
                </div>
              );
            })}
            {g.replyThreads.map((t) => (
              <div className="review-item" key={t.id}>
                <div className="review-item-head">
                  <span className="loc">
                    {t.id === CONVERSATION_ID ? "Conversation" : `${fileByName.get(t.file!)?.path}:${threadLine(t)}`} · reply
                  </span>
                  <ThreadStatus thread={t} />
                  <span className="spacer" />
                  {t.file && (
                    <button className="link" onClick={() => jumpTo(t.file!, t.side, threadLine(t))}>
                      Jump to line →
                    </button>
                  )}
                </div>
                <ThreadBox thread={t} expanded preview />
              </div>
            ))}
          </section>
        ))}
      </div>

      <div className="submit-bar">
        <div className="submit-inner">
          <textarea
            value={reviewBody}
            placeholder="Leave a comment on the whole review (required to request changes)"
            onChange={(e) => setReviewBody(e.target.value)}
          />
          <div className="submit-actions">
            <button className="action" disabled={!canSubmit.COMMENT} onClick={() => submit("COMMENT")}>
              {label("Comment")}
            </button>
            <button
              className="action reject"
              disabled={!canSubmit.REQUEST_CHANGES}
              title={canSubmit.REQUEST_CHANGES ? undefined : "GitHub requires a comment to request changes"}
              onClick={() => submit("REQUEST_CHANGES")}
            >
              {label("Request changes")}
            </button>
            <button className="action approve" disabled={!canSubmit.APPROVE} onClick={() => submit("APPROVE")}>
              {label("Approve")}
            </button>
          </div>
          {submitted && (
            <div className="submitted">
              <div className="muted">Prototype: nothing was sent. This is what the backend would post to GitHub:</div>
              <pre>{JSON.stringify(submitted.payload, null, 2)}</pre>
              <button className="link" onClick={() => setSubmitted(null)}>
                dismiss
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
