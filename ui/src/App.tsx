import { useEffect, useState, type ReactNode } from "react";
import { PR, isResolved, newCount } from "./lib/threads";
import { StoreProvider, useStore, type FileData, type Tab } from "./store";
import { DiffTab } from "./tabs/DiffTab";
import { ReviewTab, useReviewPayload } from "./tabs/ReviewTab";
import { SummaryTab } from "./tabs/SummaryTab";

interface ManifestEntry {
  name: string;
  label: string;
  path: string;
}

const fetchText = (url: string) => fetch(url).then((r) => r.text());

async function loadAll(): Promise<FileData[]> {
  const manifest: ManifestEntry[] = await fetch("/fixtures/manifest.json").then((r) => r.json());
  return Promise.all(
    manifest.map(async (m) => {
      const [oldText, newText, diffText] = await Promise.all([
        fetchText(`/fixtures/${m.name}/old`),
        fetchText(`/fixtures/${m.name}/new`),
        fetchText(`/fixtures/${m.name}/diff`),
      ]);
      return { ...m, oldText, newText, diffText };
    }),
  );
}

function Panel({ id, children }: { id: Tab; children: ReactNode }) {
  const { tab } = useStore();
  return (
    <section className={`panel${tab === id ? " active" : ""}`} aria-hidden={tab !== id}>
      {children}
    </section>
  );
}

function Header() {
  const { tab, setTab, threads, resolutions } = useStore();
  const payload = useReviewPayload();
  const pending = payload.comments.length + payload.replies.length;
  const withNew = threads.filter((t) => {
    const r = resolutions.get(t.id);
    return !isResolved(t, r) && newCount(t, r) > 0;
  }).length;

  const tabs: { id: Tab; label: string; badge?: ReactNode }[] = [
    { id: "summary", label: "Summary", badge: withNew > 0 ? <span className="badge-new">{withNew} new</span> : null },
    { id: "diff", label: "Diff" },
    { id: "review", label: "Review", badge: pending > 0 ? <span className="badge-count">{pending}</span> : null },
  ];

  return (
    <header className="app-header">
      <div className="pr-title">
        <span className="pr-number">#{PR.number}</span> {PR.title}
      </div>
      <nav className="tabs">
        {tabs.map((t) => (
          <button key={t.id} className={`tab${tab === t.id ? " active" : ""}`} onClick={() => setTab(t.id)}>
            {t.label}
            {t.badge}
          </button>
        ))}
      </nav>
      <button className="action primary finish" onClick={() => setTab("review")}>
        Finish review{pending > 0 ? ` · ${pending}` : ""}
      </button>
    </header>
  );
}

export function App() {
  const [files, setFiles] = useState<FileData[] | null>(null);
  useEffect(() => {
    loadAll().then(setFiles);
  }, []);

  if (!files) return <div className="loading">Loading…</div>;

  return (
    <StoreProvider files={files}>
      <div className="app">
        <Header />
        <div className="panels">
          <Panel id="summary">
            <SummaryTab />
          </Panel>
          <Panel id="diff">
            <DiffTab />
          </Panel>
          <Panel id="review">
            <ReviewTab />
          </Panel>
        </div>
      </div>
    </StoreProvider>
  );
}
