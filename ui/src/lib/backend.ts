import type { Commit } from "../store";
import type { PrInfo } from "./threads";

export interface Manifest {
  /** Present when the backend opened a real PR; absent for the fake fixtures. */
  pr?: PrInfo;
  base: string;
  commits: Commit[];
  files: { path: string; dir: string; added: number; removed: number; changedBy: number[] }[];
}

export interface FileKey {
  path: string;
  dir: string;
}

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const cache = new Map<string, Promise<string>>();
function cached(key: string, load: () => Promise<string>): Promise<string> {
  let p = cache.get(key);
  if (!p) {
    p = load();
    cache.set(key, p);
  }
  return p;
}

// Commit oid of each file state: state 0 is the PR base, state n is commit n.
let stateOids: string[] = [];

export async function loadManifest(): Promise<Manifest> {
  if (!inTauri()) return fetch("/pr/manifest.json").then((r) => r.json());
  const { invoke } = await import("@tauri-apps/api/core");
  const manifest = await invoke<Manifest>("open_pr");
  stateOids = [manifest.base, ...manifest.commits.map((c) => c.oid)];
  return manifest;
}

/** The file's content at a state (see `stateOf` in range.ts). */
export function fileText(file: FileKey, state: number): Promise<string> {
  return cached(`text:${file.path}:${state}`, async () => {
    if (!inTauri()) return fetch(`/pr/files/${file.dir}/text-${state}`).then((r) => r.text());
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("file_text", { oid: stateOids[state], path: file.path });
  });
}

/** `git diff` of the file from one state to another. */
export function fileDiff(file: FileKey, from: number, to: number): Promise<string> {
  return cached(`diff:${file.path}:${from}-${to}`, async () => {
    if (!inTauri()) return fetch(`/pr/files/${file.dir}/diff-${from}-${to}`).then((r) => r.text());
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("file_diff", { fromOid: stateOids[from], toOid: stateOids[to], path: file.path });
  });
}
