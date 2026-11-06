import { useEffect, useState, type ReactNode } from "react";
import { FatalError } from "./components/ErrorBoundary";
import { fileText, loadManifest, loadSaved, type SavedState } from "./lib/backend";
import { describeError, describeViewport, log } from "./lib/log";
import { routeUrl } from "./lib/route";
import { ALL_THREADS, PR, isResolved, newCount, threadFromJson, type FakeThread, type PrInfo } from "./lib/threads";
import { StoreProvider, useStore, type Commit, type FileData, type Tab } from "./store";
import { DiffTab } from "./tabs/DiffTab";
import { ReviewTab, useReviewPayload } from "./tabs/ReviewTab";
import { SummaryTab } from "./tabs/SummaryTab";

interface Loaded {
  files: FileData[];
  commits: Commit[];
  pr: PrInfo;
  threads: FakeThread[];
  saved: SavedState | null;
}

export async function loadAll(): Promise<Loaded> {
  const started = performance.now();
  const elapsed = () => `${Math.round(performance.now() - started)}ms`;
  log("info", "loading the PR from the backend");
  const manifest = await loadManifest();
  log(
    "info",
    `got the manifest: ${manifest.files.length} files, ${manifest.commits.length} commits, ` +
      `${manifest.threads ? `${manifest.threads.length} threads` : "no threads"} (${elapsed()})`,
  );
  const files = await Promise.all(
    manifest.files.map(
      async (f): Promise<FileData> => ({
        name: f.path,
        path: f.path,
        dir: f.dir,
        added: f.added,
        removed: f.removed,
        changedBy: f.changedBy,
        newText: await fileText(f, Math.max(...f.changedBy)),
      }),
    ),
  );
  log("info", `read the latest text of ${files.length} files (${elapsed()})`);
  const now = Date.now();
  const saved = await loadSaved();
  if (saved) {
    log(
      "info",
      `saved state: ${saved.approvals.length} approvals, ${saved.drafts.length} drafts, ` +
        `${saved.replies.length} replies (${elapsed()})`,
    );
  }
  return {
    files,
    commits: manifest.commits,
    pr: manifest.pr ?? PR,
    // The fake threads only fit the fixtures, which have no `threads`.
    threads: manifest.threads ? manifest.threads.map((t) => threadFromJson(t, now)) : ALL_THREADS,
    saved,
  };
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
  const { tab, setTab, currentFile, threads, resolutions, pr } = useStore();
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
        <span className="pr-number">#{pr.number}</span> {pr.title}
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
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    loadAll()
      .then(setLoaded)
      .catch((e) => {
        const details = describeError(e);
        log("error", `loading the PR failed: ${details}`);
        setError(details);
      });
  }, []);
  useEffect(() => {
    if (loaded) log("info", `rendered the PR. ${describeViewport()}`);
  }, [loaded]);

  if (error !== null) return <FatalError title="Could not load the PR" details={error} />;
  if (!loaded) return <div className="loading">Loading…</div>;

  return (
    <StoreProvider
      files={loaded.files}
      commits={loaded.commits}
      threads={loaded.threads}
      pr={loaded.pr}
      saved={loaded.saved}
    >
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
