import { FileMatrix } from "../components/FileMatrix";
import { Markdown } from "../components/Markdown";
import { ThreadBox } from "../components/ThreadBox";
import { CONVERSATION_ID, PR, isResolved, newCount, sortThreads } from "../lib/threads";
import { useStore } from "../store";

const excerpt = (s: string) => {
  const flat = s.replace(/\s+/g, " ").trim();
  return flat.length > 90 ? `${flat.slice(0, 90)}…` : flat;
};

export function SummaryTab() {
  const { threads, resolutions, fileByName, jumpTo } = useStore();
  const lineThreads = sortThreads(
    threads.filter((t) => t.file !== null),
    resolutions,
  );
  const conversation = threads.find((t) => t.id === CONVERSATION_ID);
  const open = lineThreads.filter((t) => !isResolved(t, resolutions.get(t.id)));
  const withNew = open.filter((t) => newCount(t, resolutions.get(t.id)) > 0);

  return (
    <div className="page">
      <h1>
        {PR.title} <span className="pr-number">#{PR.number}</span>
      </h1>
      <p className="muted">
        opened by <strong>{PR.author}</strong>
      </p>
      <section className="card">
        <Markdown text={PR.body} />
      </section>

      <h2>Files</h2>
      <FileMatrix />

      <h2>Discussions on code</h2>
      <p className="muted">
        {open.length} open · {withNew.length} with new comments · {lineThreads.length - open.length} resolved
      </p>
      <div className="index">
        {lineThreads.map((t) => {
          const r = resolutions.get(t.id);
          const resolved = isResolved(t, r);
          const n = newCount(t, r);
          const file = fileByName.get(t.file!);
          return (
            <button className="index-row" key={t.id} onClick={() => jumpTo(t.file!, t.side, t.line, t.id)}>
              <span className={`status ${resolved ? "resolved" : "open"}`}>{resolved ? "✓ Resolved" : "● Open"}</span>
              {n > 0 && <span className="badge-new">{n} new</span>}
              <span className="loc">
                {file?.path}:{t.line}
              </span>
              <span className="excerpt">{excerpt(t.comments[t.comments.length - 1].body)}</span>
              <span className="jump">Jump →</span>
            </button>
          );
        })}
      </div>

      {conversation && (
        <>
          <h2>Conversation</h2>
          <div className="conversation">
            <ThreadBox thread={conversation} />
          </div>
        </>
      )}
    </div>
  );
}
