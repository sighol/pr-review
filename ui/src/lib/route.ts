import type { Side } from "./diff";

export type Tab = "summary" | "diff" | "review";

export interface Route {
  tab: Tab;
  file?: string;
  thread?: string;
  /** "<side>:<line>", e.g. "new:45". */
  line?: string;
}

export function parseRoute(pathname: string, search: string): Route {
  const path = pathname.replace(/\/+$/, "") || "/";
  if (path === "/review") return { tab: "review" };
  if (path === "/diff") {
    const q = new URLSearchParams(search);
    return {
      tab: "diff",
      file: q.get("file") ?? undefined,
      thread: q.get("thread") ?? undefined,
      line: q.get("line") ?? undefined,
    };
  }
  return { tab: "summary" };
}

export function routeUrl(r: Route): string {
  if (r.tab === "summary") return "/";
  if (r.tab === "review") return "/review";
  const q = new URLSearchParams();
  if (r.file) q.set("file", r.file);
  if (r.thread) q.set("thread", r.thread);
  else if (r.line) q.set("line", r.line);
  const s = q.toString();
  return s ? `/diff?${s}` : "/diff";
}

export function parseLine(s: string | undefined): { side: Side; line: number } | null {
  const m = /^(old|new):(\d+)$/.exec(s ?? "");
  return m ? { side: m[1] as Side, line: Number(m[2]) } : null;
}
