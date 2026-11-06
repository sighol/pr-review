import { describe, expect, test } from "vitest";
import { parseLine, parseRoute, routeUrl } from "./route";

describe("routes", () => {
  test("/ and unknown paths are the summary", () => {
    expect(parseRoute("/", "")).toEqual({ tab: "summary" });
    expect(parseRoute("/nope", "")).toEqual({ tab: "summary" });
  });

  test("/review and trailing slashes", () => {
    expect(parseRoute("/review", "")).toEqual({ tab: "review" });
    expect(parseRoute("/review/", "")).toEqual({ tab: "review" });
  });

  test("/diff carries file, thread and line", () => {
    expect(parseRoute("/diff", "?file=small-main&thread=t4")).toEqual({
      tab: "diff",
      file: "small-main",
      thread: "t4",
      line: undefined,
    });
  });

  test("routeUrl round-trips", () => {
    for (const r of [
      { tab: "summary" as const },
      { tab: "review" as const },
      { tab: "diff" as const },
      { tab: "diff" as const, file: "small-main" },
      { tab: "diff" as const, file: "small-main", thread: "t4" },
      { tab: "diff" as const, file: "small-main", line: "new:45" },
    ]) {
      const [path, search = ""] = routeUrl(r).split("?");
      expect(parseRoute(path, search ? `?${search}` : "")).toMatchObject(r);
    }
  });

  test("a thread wins over a line in the URL", () => {
    expect(routeUrl({ tab: "diff", file: "f", thread: "t1", line: "new:3" })).toBe("/diff?file=f&thread=t1");
  });

  test("parseLine", () => {
    expect(parseLine("old:12")).toEqual({ side: "old", line: 12 });
    expect(parseLine("x:1")).toBeNull();
    expect(parseLine(undefined)).toBeNull();
  });
});
