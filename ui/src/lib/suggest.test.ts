import { expect, test } from "vitest";
import { insertBlock } from "./suggest";

const block = "```suggestion\nlet x = 1;\n```";

test("into an empty draft", () => {
  const r = insertBlock("", 0, block);
  expect(r.text).toBe(block);
  expect(r.text.slice(0, r.caret)).toBe("```suggestion\nlet x = 1;");
});

test("after existing text goes on its own line", () => {
  const r = insertBlock("Use this instead:", 17, block);
  expect(r.text).toBe(`Use this instead:\n${block}`);
});

test("in the middle of text is separated on both sides", () => {
  const r = insertBlock("beforeafter", 6, block);
  expect(r.text).toBe(`before\n${block}\nafter`);
});

test("caret at a line start needs no extra newline", () => {
  const r = insertBlock("first\nsecond", 6, block);
  expect(r.text).toBe(`first\n${block}\nsecond`);
});
