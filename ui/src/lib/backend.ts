import type { Commit } from "../store";
import type { Directives } from "./directives";
import type { PrInfo, ThreadJson } from "./threads";

export interface Manifest {
  /** Present when the backend opened a real PR; absent for the fake fixtures. */
  pr?: PrInfo;
  /** Review threads and the PR conversation; present with `pr`. */
  threads?: ThreadJson[];
  base: string;
  commits: Commit[];
  files: { path: string; oldPath?: string; dir: string; added: number; removed: number; changedBy: number[] }[];
}

export interface FileKey {
  path: string;
  /** The file's path at the PR base, when it was renamed in the PR. */
  oldPath?: string;
  dir: string;
}

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const cache = new Map<string, Promise<string>>();
function cached(key: string, load: () => Promise<string>): Promise<string> {
  let p = cache.get(key);
  if (!p) {
    p = load();
    cache.set(key, p);
  }
  return p;
}

/** Drops every cached file text/diff: a refresh can shift which commit a state number means. */
export function clearFileCache() {
  cache.clear();
}

// Commit oid of each state n >= 1 (state n is commit n); state 0 has no single oid, see `floorOid`.
let stateOids: string[] = [];
// commits[n - 1].floor, i.e. what "state 0" means for a range ending at commit n.
let commitFloors: string[] = [];
let manifestBase = "";

function setStateOids(manifest: Manifest) {
  stateOids = manifest.commits.map((c) => c.oid);
  commitFloors = manifest.commits.map((c) => c.floor);
  manifestBase = manifest.base;
}

/**
 * The oid "state 0" resolves to for a range ending at commit `asOf`: the PR base as of that
 * point, which jumps forward if an earlier commit already merged the PR's base branch in (see
 * `ManifestCommit::floor` on the backend). `asOf` 0 is the degenerate case of a range that never
 * reaches any real commit; `manifestBase` is as good a fallback as any there.
 */
function floorOid(asOf: number): string {
  return asOf >= 1 ? commitFloors[asOf - 1] : manifestBase;
}

export async function loadManifest(): Promise<Manifest> {
  if (!inTauri()) return fetch("/pr/manifest.json").then((r) => r.json());
  const { invoke } = await import("@tauri-apps/api/core");
  const manifest = await invoke<Manifest>("open_pr");
  setStateOids(manifest);
  return manifest;
}

/**
 * Re-fetches the PR exactly like opening it fresh would: git (new commits/files, force-push
 * detection) and GitHub (metadata, discussion). Resolves to null in a browser, where nothing is
 * fetched. Updates the state oids like `loadManifest`, since a refresh can change what a state
 * number means.
 */
export async function refreshManifest(): Promise<Manifest | null> {
  if (!inTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  const manifest = await invoke<Manifest>("refresh");
  setStateOids(manifest);
  return manifest;
}

/** Opens the file in the given editor program (e.g. `code`, `zed`, `idea`). No-op in a browser. */
export async function openInEditor(path: string, program: string): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_in_editor", { path, program });
}

/**
 * The file's content at a state (see `stateOf` in range.ts). `asOf` is the commit number the
 * state was computed relative to (`stateOf`'s second argument) -- needed to resolve state 0,
 * which has no single oid of its own; ignored for state n >= 1, a real commit's own oid.
 */
export function fileText(file: FileKey, state: number, asOf: number): Promise<string> {
  const cacheKey = state === 0 ? `text:${file.path}:0@${asOf}` : `text:${file.path}:${state}`;
  return cached(cacheKey, async () => {
    if (!inTauri()) return fetch(`/pr/files/${file.dir}/text-${state}`).then((r) => r.text());
    const { invoke } = await import("@tauri-apps/api/core");
    const oid = state === 0 ? floorOid(asOf) : stateOids[state - 1];
    return invoke<string>("file_text", { oid, path: file.path, oldPath: file.oldPath ?? null });
  });
}

/** `git diff` of the file from one state to another. */
export function fileDiff(file: FileKey, from: number, to: number): Promise<string> {
  return cached(`diff:${file.path}:${from}-${to}`, async () => {
    if (!inTauri()) return fetch(`/pr/files/${file.dir}/diff-${from}-${to}`).then((r) => r.text());
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("file_diff", {
      fromOid: from === 0 ? floorOid(to) : stateOids[from - 1],
      toOid: to === 0 ? floorOid(to) : stateOids[to - 1],
      path: file.path,
      oldPath: file.oldPath ?? null,
    });
  });
}

/** Review state saved in the database for this PR (Tauri only). */
export interface SavedState {
  approvals: { filePath: string; commitOid: string }[];
  drafts: {
    draftKey: string;
    filePath: string;
    side: "old" | "new";
    startLine: number;
    endLine: number;
    body: string;
    /** The file changed since the draft was written, so its line may be wrong. */
    stale: boolean;
  }[];
  replies: { threadId: string; body: string }[];
  resolutions: { threadId: string; resolvedThrough: string | null; reopened: boolean }[];
  reviewBody: string;
  /** Force-pushes (history rewrites) detected across restarts, oldest first. */
  forcePushes: { oldOid: string; newOid: string; detectedAt: string }[];
}

/** The saved review state, or null in a browser, where nothing is persisted. */
export async function loadSaved(): Promise<SavedState | null> {
  if (!inTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SavedState>("saved_state");
}

// Saves run one at a time, in order, so a delete can never overtake the save before it.
let queue: Promise<unknown> = Promise.resolve();
function save(command: string, args: Record<string, unknown>) {
  if (!inTauri()) return;
  queue = queue
    .then(async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke(command, args);
    })
    .catch((e) => console.error(`saving (${command}) failed:`, e));
}

const pending = new Map<string, { timer: ReturnType<typeof setTimeout>; run: () => void }>();
const cancelSave = (id: string) => {
  clearTimeout(pending.get(id)?.timer);
  pending.delete(id);
};
function saveSoon(id: string, run: () => void) {
  cancelSave(id);
  pending.set(id, {
    timer: setTimeout(() => {
      pending.delete(id);
      run();
    }, 400),
    run,
  });
}

/** Runs every delayed save now. */
export function flushSaves() {
  for (const [id, { timer, run }] of [...pending]) {
    clearTimeout(timer);
    pending.delete(id);
    run();
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("focusout", flushSaves);
  window.addEventListener("pagehide", flushSaves);
}

export type ReviewEvent = "APPROVE" | "COMMENT" | "REQUEST_CHANGES";

export interface SubmitResult {
  /** Null when only label and assignee changes were made and no review was posted. */
  reviewUrl: string | null;
  /** The review went through but the conversation comment did not. */
  conversationError: string | null;
  /** What the label and assignee changes did; `unapplied` is what is left to retry after an error. */
  directives: { changes: string[]; error: string | null; unapplied: Directives };
  /** The discussion after the review; null if refetching it failed. */
  threads: ThreadJson[] | null;
  refreshError: string | null;
}

/**
 * Sends the saved review to GitHub. Resolves to null in a browser, where nothing is sent.
 * Rejects with GitHub's or git's message when the submit fails; nothing is posted then.
 */
export function submitReview(
  event: ReviewEvent,
  body: string,
  directives: Directives,
  onProgress?: (message: string) => void,
): Promise<SubmitResult | null> {
  if (!inTauri()) return Promise.resolve(null);
  flushSaves(); // so the database has everything you typed
  const run = queue.then(async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = onProgress
      ? await listen<string>("review-progress", (e) => onProgress(e.payload))
      : undefined;
    try {
      return await invoke<SubmitResult>("submit_review", { event, body, directives });
    } finally {
      unlisten?.();
    }
  });
  queue = run.catch(() => undefined);
  return run;
}

export const persist = {
  acknowledgeDraft: (draftKey: string) => save("acknowledge_draft", { draftKey }),
  fileApprovals: (filePath: string, commitOids: string[]) => save("set_file_approvals", { filePath, commitOids }),
  /** Typing is saved after a short pause. */
  draft: (d: { draftKey: string; filePath: string; side: "old" | "new"; startLine: number; endLine: number; body: string }) =>
    saveSoon(`draft:${d.draftKey}`, () => save("save_draft", { draft: d })),
  deleteDraft: (draftKey: string) => {
    cancelSave(`draft:${draftKey}`);
    save("delete_draft", { draftKey });
  },
  reply: (threadId: string, body: string) => saveSoon(`reply:${threadId}`, () => save("save_reply", { threadId, body })),
  resolution: (threadId: string, resolvedThrough: string | null, reopened: boolean) =>
    save("set_resolution", { threadId, resolvedThrough, reopened }),
  reviewBody: (body: string) => saveSoon("review-body", () => save("set_review_body", { body })),
};
