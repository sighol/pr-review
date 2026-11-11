import { useCallback, useEffect, useState, type ReactNode } from "react";
import { FatalError } from "./components/ErrorBoundary";
import { clearFileCache, fileText, loadManifest, loadSaved, refreshManifest, type Manifest, type SavedState } from "./lib/backend";
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

async function buildLoaded(manifest: Manifest, elapsed: () => string): Promise<Loaded> {
  const files = await Promise.all(
    manifest.files.map(
      async (f): Promise<FileData> => ({
        name: f.path,
        path: f.path,
        oldPath: f.oldPath,
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
  return buildLoaded(manifest, elapsed);
}

/**
 * Re-fetches the whole PR exactly like opening it fresh would (new commits, files, force-push
 * detection) and rebuilds everything derived from it. Resolves to null in a browser, where
 * nothing is fetched and the fixtures never change.
 */
async function reloadAll(): Promise<Loaded | null> {
  const started = performance.now();
  const elapsed = () => `${Math.round(performance.now() - started)}ms`;
  log("info", "refreshing the PR from the backend");
  clearFileCache();
  const manifest = await refreshManifest();
  if (!manifest) return null;
  log(
    "info",
    `refreshed: ${manifest.files.length} files, ${manifest.commits.length} commits, ` +
      `${manifest.threads ? `${manifest.threads.length} threads` : "no threads"} (${elapsed()})`,
  );
  return buildLoaded(manifest, elapsed);
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
  const { tab, setTab, currentFile, threads, resolutions, pr, refresh, refreshing, refreshError } = useStore();
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
      {refreshError && <span className="refresh-error">{refreshError}</span>}
      <button
        className="action"
        onClick={refresh}
        disabled={refreshing}
        title="Re-fetch the PR, the same as closing and reopening the app"
      >
        {refreshing ? "Refreshing…" : "Refresh"}
      </button>
    </header>
  );
}

export function App() {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  useEffect(() => {
    loadAll()
      .then(setLoaded)
      .catch((e) => {
        const details = describeError(e);
        log("error", `loading the PR failed: ${details}`);
        setError(details);
      });
  }, []);
  const refresh = useCallback(() => {
    setRefreshing(true);
    setRefreshError(null);
    reloadAll()
      .then((fresh) => {
        if (fresh) {
          setLoaded(fresh);
          setRefreshKey((k) => k + 1);
        }
      })
      .catch((e) => setRefreshError(e instanceof Error ? e.message : String(e)))
      .finally(() => setRefreshing(false));
  }, []);
  useEffect(() => {
    if (loaded) log("info", `rendered the PR. ${describeViewport()}`);
  }, [loaded]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
      e.preventDefault();
      if (e.key === "ArrowLeft") history.back();
      else history.forward();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (error !== null) return <FatalError title="Could not load the PR" details={error} />;
  if (!loaded) return <div className="loading">Loading…</div>;

  return (
    <StoreProvider
      key={refreshKey}
      files={loaded.files}
      commits={loaded.commits}
      threads={loaded.threads}
      pr={loaded.pr}
      saved={loaded.saved}
      refresh={refresh}
      refreshing={refreshing}
      refreshError={refreshError}
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
