import { useMemo, useState } from "react";
import { CommentBox, describeRange } from "../components/CommentBox";
import { DirectiveTextarea } from "../components/DirectiveTextarea";
import { ExternalLink } from "../components/ExternalLink";
import { ProgressPopup } from "../components/ProgressPopup";
import { ThreadBox } from "../components/ThreadBox";
import { inTauri, type ReviewEvent as Event, type SubmitResult } from "../lib/backend";
import { describeDirectives, hasDirectives, parseDirectives } from "../lib/directives";
import { CONVERSATION_ID, isResolved, newCount, type FakeThread } from "../lib/threads";
import { useRangeData } from "../lib/data";
import { snippetFor } from "../lib/snippet";
import { isRealDraft, pendingReplies, threadLine, useStore, type Draft, type FileData } from "../store";

const MAX_SNIPPET = 10;

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
  const { files, fileByName, threads, drafts, replies, jumpTo, reviewBody, setReviewBody, acknowledgeDraft, submitReview } =
    useStore();
  const payload = useReviewPayload();
  const pending = payload.comments.length + payload.replies.length;
  // In a browser nothing is sent; the payload that would be sent is shown instead.
  const [submitted, setSubmitted] = useState<{ event: Event; payload: unknown } | null>(null);
  const [sending, setSending] = useState<Event | null>(null);
  const [progressLines, setProgressLines] = useState<string[] | null>(null);
  const [result, setResult] = useState<SubmitResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bumped by "Preview all"; each draft/reply box switches itself to preview mode when this changes.
  const [previewAllTick, setPreviewAllTick] = useState(0);
  const staleCount = [...drafts.values()].filter((d) => d.stale && isRealDraft(d)).length;

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
    if (conv.length) out.push({ key: "conversation", title: "Activity", draftItems: [], replyThreads: conv });
    return out;
  }, [files, drafts, replies, threads]);

  // GitHub's rules apply to the text that is left once the directives are taken out.
  const parsed = useMemo(() => parseDirectives(reviewBody), [reviewBody]);
  const body = parsed.text;
  const changes = hasDirectives(parsed.directives);
  const idle = sending === null && staleCount === 0;
  const canSubmit: Record<Event, boolean> = {
    APPROVE: idle,
    COMMENT: idle && (body !== "" || pending > 0 || changes),
    REQUEST_CHANGES: idle && body !== "",
  };
  const submit = async (event: Event) => {
    setError(null);
    setResult(null);
    setSubmitted(null);
    setSending(event);
    setProgressLines(inTauri() ? ["Submitting review..."] : null);
    try {
      const sent = await submitReview(event, parsed.text, parsed.directives, (message) =>
        setProgressLines((prev) => [...(prev ?? []), message]),
      );
      if (sent) setResult(sent);
      else {
        const directives = changes ? { directives: parsed.directives } : {};
        setSubmitted({ event, payload: { event, body: parsed.text, ...directives, ...payload } });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setSending(null);
      setProgressLines(null);
    }
  };
  const label = (name: Event, text: string) =>
    sending === name ? "Sending…" : pending > 0 ? `${text} · ${pending} comment${pending === 1 ? "" : "s"}` : text;
  const blockedHint = staleCount > 0 ? "Check the drafts marked out of date first" : undefined;

  return (
    <div className="review">
      {progressLines && <ProgressPopup lines={progressLines} />}
      <div className="page">
        <div className="review-head">
          <h1>Review</h1>
          <span className="spacer" />
          <button className="action small" disabled={pending === 0} onClick={() => setPreviewAllTick((t) => t + 1)}>
            Preview all
          </button>
        </div>
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
                  {d.stale && isRealDraft(d) && (
                    <div className="stale-warning">
                      <span>The file changed since you wrote this, so it may now be on the wrong line.</span>
                      <span className="spacer" />
                      <button className="action small" onClick={() => acknowledgeDraft(d.key)}>
                        Keep as is
                      </button>
                    </div>
                  )}
                  <Snippet file={file} draft={d} />
                  <CommentBox draft={d} preview previewAllTick={previewAllTick} />
                </div>
              );
            })}
            {g.replyThreads.map((t) => (
              <div className="review-item" key={t.id}>
                <div className="review-item-head">
                  <span className="loc">
                    {t.id === CONVERSATION_ID ? "Activity" : `${fileByName.get(t.file!)?.path}:${threadLine(t)}`} · reply
                  </span>
                  <ThreadStatus thread={t} />
                  <span className="spacer" />
                  {t.file && (
                    <button className="link" onClick={() => jumpTo(t.file!, t.side, threadLine(t))}>
                      Jump to line →
                    </button>
                  )}
                </div>
                <ThreadBox thread={t} expanded preview previewAllTick={previewAllTick} />
              </div>
            ))}
          </section>
        ))}
      </div>

      <div className="submit-bar">
        <div className="submit-inner">
          <DirectiveTextarea
            value={reviewBody}
            placeholder="Leave a comment on the whole review (required to request changes)"
            onChange={setReviewBody}
          />
          {changes && (
            <div className="directive-summary">
              {describeDirectives(parsed.directives, body !== "" || pending > 0).join(" · ")}
            </div>
          )}
          <div className="submit-actions">
            <button
              className="action"
              disabled={!canSubmit.COMMENT}
              title={blockedHint}
              onClick={() => submit("COMMENT")}
            >
              {label("COMMENT", "Comment")}
            </button>
            <button
              className="action reject"
              disabled={!canSubmit.REQUEST_CHANGES}
              title={blockedHint ?? (body === "" ? "GitHub requires a comment to request changes" : undefined)}
              onClick={() => submit("REQUEST_CHANGES")}
            >
              {label("REQUEST_CHANGES", "Request changes")}
            </button>
            <button
              className="action approve"
              disabled={!canSubmit.APPROVE}
              title={blockedHint}
              onClick={() => submit("APPROVE")}
            >
              {label("APPROVE", "Approve")}
            </button>
          </div>
          {error && <div className="submit-note error">Nothing was posted. {error}</div>}
          {result && (
            <div
              className={`submit-note ${result.conversationError || result.refreshError || result.directives.error ? "warn" : "ok"}`}
            >
              <div>
                {result.reviewUrl ? (
                  <>
                    Review submitted: <ExternalLink href={result.reviewUrl}>{result.reviewUrl}</ExternalLink>
                  </>
                ) : (
                  "No review was posted."
                )}
              </div>
              {result.directives.changes.length > 0 && <div>{result.directives.changes.join(" · ")}</div>}
              {result.directives.error && (
                <div>
                  A label or assignee change failed ({result.directives.error}). The changes that did not go through
                  are back in the review text; fix the problem and press Comment to retry.
                </div>
              )}
              {result.conversationError && (
                <div>
                  Your conversation comment was not posted ({result.conversationError}). It is still in your drafts.
                </div>
              )}
              {result.refreshError && (
                <div>Could not refresh the threads ({result.refreshError}). Restart pr-review to see your comments.</div>
              )}
            </div>
          )}
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
