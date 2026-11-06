import { expect, test } from "vitest";
import { isExternalUrl } from "./external";

test("web and mail links leave the app", () => {
  expect(isExternalUrl("https://github.com/o/r/pull/1#discussion_r1")).toBe(true);
  expect(isExternalUrl("http://example.com")).toBe(true);
  expect(isExternalUrl("HTTPS://EXAMPLE.COM")).toBe(true);
  expect(isExternalUrl("mailto:someone@example.com")).toBe(true);
});

test("relative links, anchors and other schemes stay inside", () => {
  expect(isExternalUrl("#section")).toBe(false);
  expect(isExternalUrl("src/main.rs")).toBe(false);
  expect(isExternalUrl("/pr/manifest.json")).toBe(false);
  expect(isExternalUrl("javascript:alert(1)")).toBe(false);
  expect(isExternalUrl("file:///etc/passwd")).toBe(false);
});
