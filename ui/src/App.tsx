import { useEffect, useState } from "react";
import { DiffView } from "./components/DiffView";

interface ManifestEntry {
  name: string;
  label: string;
  path: string;
}

interface Loaded {
  name: string;
  path: string;
  oldText: string;
  newText: string;
  diffText: string;
}

const fetchText = (url: string) => fetch(url).then((r) => r.text());

export function App() {
  const [manifest, setManifest] = useState<ManifestEntry[]>([]);
  const [current, setCurrent] = useState<string>("");
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [wholeFile, setWholeFile] = useState(false);

  useEffect(() => {
    fetch("/fixtures/manifest.json")
      .then((r) => r.json())
      .then((m: ManifestEntry[]) => {
        setManifest(m);
        setCurrent(new URLSearchParams(location.search).get("fixture") ?? m[0]?.name ?? "");
      });
  }, []);

  useEffect(() => {
    const entry = manifest.find((m) => m.name === current);
    if (!entry) return;
    let cancelled = false;
    setLoaded(null);
    Promise.all([
      fetchText(`/fixtures/${entry.name}/old`),
      fetchText(`/fixtures/${entry.name}/new`),
      fetchText(`/fixtures/${entry.name}/diff`),
    ]).then(([oldText, newText, diffText]) => {
      if (!cancelled) setLoaded({ name: entry.name, path: entry.path, oldText, newText, diffText });
    });
    return () => {
      cancelled = true;
    };
  }, [manifest, current]);

  return (
    <div className="app">
      <header className="toolbar">
        <select value={current} onChange={(e) => setCurrent(e.target.value)}>
          {manifest.map((m) => (
            <option key={m.name} value={m.name}>
              {m.label}
            </option>
          ))}
        </select>
        <span className="path">{loaded?.path}</span>
        <button onClick={() => setWholeFile((w) => !w)}>{wholeFile ? "Show changed hunks" : "Show whole file"}</button>
        <span className="hint">Drag a line number on a changed line to comment</span>
      </header>
      <main>
        {loaded ? (
          <DiffView
            key={loaded.name}
            fixtureName={loaded.name}
            path={loaded.path}
            oldText={loaded.oldText}
            newText={loaded.newText}
            diffText={loaded.diffText}
            wholeFile={wholeFile}
          />
        ) : (
          <div className="loading">Loading…</div>
        )}
      </main>
    </div>
  );
}
