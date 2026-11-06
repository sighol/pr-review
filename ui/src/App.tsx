import { useEffect, useState, type ReactNode } from "react";
import { routeUrl } from "./lib/route";
import { PR, isResolved, newCount } from "./lib/threads";
import { StoreProvider, useStore, type Commit, type FileData, type Tab } from "./store";
import { DiffTab } from "./tabs/DiffTab";
import { ReviewTab, useReviewPayload } from "./tabs/ReviewTab";
import { SummaryTab } from "./tabs/SummaryTab";

interface Manifest {
  commits: Commit[];
  files: { path: string; dir: string; added: number; removed: number; changedBy: number[]; from: number[] }[];
}

interface Loaded {
  files: FileData[];
  commits: Commit[];
}

const fetchText = (url: string) => fetch(url).then((r) => r.text());

async function loadAll(): Promise<Loaded> {
  const manifest: Manifest = await fetch("/pr/manifest.json").then((r) => r.json());
  const files = await Promise.all(
    manifest.files.map(async (f): Promise<FileData> => {
      const dir = `/pr/files/${f.dir}`;
      const [newText, versions] = await Promise.all([
        fetchText(`${dir}/new`),
        Promise.all(
          f.from.map(async (k) => {
            const [oldText, diffText] = await Promise.all([fetchText(`${dir}/old-${k}`), fetchText(`${dir}/diff-${k}`)]);
            return [k, { oldText, diffText }] as const;
          }),
        ),
      ]);
      return {
        name: f.path,
        path: f.path,
        added: f.added,
        removed: f.removed,
        changedBy: f.changedBy,
        newText,
        versions: new Map(versions),
      };
    }),
  );
  return { files, commits: manifest.commits };
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
  const { tab, setTab, currentFile, threads, resolutions } = useStore();
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
          <a
            key={t.id}
            href={routeUrl(t.id === "diff" ? { tab: "diff", file: currentFile } : { tab: t.id })}
            className={`tab${tab === t.id ? " active" : ""}`}
            onClick={(e) => {
              if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
              e.preventDefault();
              setTab(t.id);
            }}
          >
            {t.label}
            {t.badge}
          </a>
        ))}
      </nav>
      <button className="action primary finish" onClick={() => setTab("review")}>
        Finish review{pending > 0 ? ` · ${pending}` : ""}
      </button>
    </header>
  );
}

export function App() {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  useEffect(() => {
    loadAll().then(setLoaded);
  }, []);

  if (!loaded) return <div className="loading">Loading…</div>;

  return (
    <StoreProvider files={loaded.files} commits={loaded.commits}>
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
