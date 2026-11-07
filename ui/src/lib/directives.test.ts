import { describe, expect, test } from "bun:test";
import { describeDirectives, formatDirectives, hasDirectives, parseDirectives } from "./directives";

describe("parseDirectives", () => {
  test("the example from the design", () => {
    const p = parseDirectives(":unicorn: -waiting-for-risk-review +waiting-for-team +risk-review-ongoing +assignee:@sighol");
    expect(p.text).toBe(":unicorn:");
    expect(p.directives).toEqual({
      addLabels: ["waiting-for-team", "risk-review-ongoing"],
      removeLabels: ["waiting-for-risk-review"],
      addAssignees: ["sighol"],
      removeAssignees: [],
    });
    expect(p.spans).toHaveLength(4);
  });

  test("spans point at the directives in the original body", () => {
    const body = "hi +foo there";
    const p = parseDirectives(body);
    expect(p.spans.map((s) => body.slice(s.start, s.end))).toEqual(["+foo"]);
    expect(p.text).toBe("hi there");
  });

  test("unassign, optional @, quoted labels", () => {
    const p = parseDirectives('-assignee:bob +assignee:@me +"needs review" -"type: bug"');
    expect(p.directives).toEqual({
      addLabels: ["needs review"],
      removeLabels: ["type: bug"],
      addAssignees: ["me"],
      removeAssignees: ["bob"],
    });
    expect(p.text).toBe("");
  });

  test("a quoted assignee: is a label", () => {
    expect(parseDirectives('+"assignee:foo"').directives.addLabels).toEqual(["assignee:foo"]);
  });

  test("labels with colons and unknown keys stay labels", () => {
    const d = parseDirectives("+area:ui +reviewer:@x").directives;
    expect(d.addLabels).toEqual(["area:ui", "reviewer:@x"]);
  });

  test("prose that is not a directive is left alone", () => {
    for (const s of ["LGTM +1", "see -3 below", "- a list item", "a+b c-d", "-- rule", "+ plus", "x+foo", "-"]) {
      const p = parseDirectives(s);
      expect(hasDirectives(p.directives)).toBe(false);
      expect(p.text).toBe(s);
    }
  });

  test("code is ignored", () => {
    const body = "run `git -foo +bar` now\n```\n+inblock -other\n```\n~~~\n+tilde\n~~~\n+real";
    const p = parseDirectives(body);
    expect(p.directives.addLabels).toEqual(["real"]);
    expect(p.text).toBe("run `git -foo +bar` now\n```\n+inblock -other\n```\n~~~\n+tilde\n~~~");
  });

  test("an unclosed fence hides the rest", () => {
    expect(hasDirectives(parseDirectives("```\n+foo").directives)).toBe(false);
  });

  test("a line holding only directives disappears, other lines stay", () => {
    const p = parseDirectives("first\n+foo +bar\nsecond +baz\n\nthird");
    expect(p.text).toBe("first\nsecond\n\nthird");
  });

  test("removing a directive does not leave a double space", () => {
    expect(parseDirectives("a +x b").text).toBe("a b");
    expect(parseDirectives("+x a").text).toBe("a");
    expect(parseDirectives("a +x").text).toBe("a");
  });

  test("duplicates count once, ignoring case", () => {
    const d = parseDirectives("+Foo +foo -bar -bar").directives;
    expect(d.addLabels).toEqual(["Foo"]);
    expect(d.removeLabels).toEqual(["bar"]);
  });

  test("contradictions are kept for the backend to reject", () => {
    const d = parseDirectives("+foo -foo").directives;
    expect(d.addLabels).toEqual(["foo"]);
    expect(d.removeLabels).toEqual(["foo"]);
  });

  test("CRLF line endings", () => {
    expect(parseDirectives("a\r\n+foo\r\nb").text).toBe("a\r\nb");
  });
});

describe("formatDirectives", () => {
  test("round trips through the parser", () => {
    const d = parseDirectives('+a -"b c" +assignee:@x -assignee:@y').directives;
    expect(parseDirectives(formatDirectives(d)).directives).toEqual(d);
  });
});

describe("describeDirectives", () => {
  test("lists what will happen", () => {
    const d = parseDirectives("+a -b +assignee:@x").directives;
    expect(describeDirectives(d, true)).toEqual(["Remove: b", "Add: a", "Assign: @x"]);
    expect(describeDirectives(d, false).at(-1)).toBe("no comment text");
  });
});
