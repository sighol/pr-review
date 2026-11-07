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

/** Re-fetches the PR's status and discussion (not its commits/files) from GitHub. Resolves to null in a browser. */
export async function refreshManifest(): Promise<Manifest | null> {
  if (!inTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<Manifest>("refresh");
}

/** Opens the file in $PR_REVIEW_EDITOR (or `code`). No-op in a browser. */
export async function openInEditor(path: string): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_in_editor", { path });
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
export function submitReview(event: ReviewEvent, body: string, directives: Directives): Promise<SubmitResult | null> {
  if (!inTauri()) return Promise.resolve(null);
  flushSaves(); // so the database has everything you typed
  const run = queue.then(async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<SubmitResult>("submit_review", { event, body, directives });
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
