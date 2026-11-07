import { expect, test } from "vitest";
import { isViewportBroken } from "./log";

test("a sane viewport is not broken", () => {
  expect(isViewportBroken(1, 1400, 900)).toBe(false);
  expect(isViewportBroken(1.25, 1120, 720)).toBe(false);
});

test("the negative values seen with WebKitGTK are broken", () => {
  expect(isViewportBroken(-0.010416666977107525, -134400, -86400)).toBe(true);
});

test("zero, negative and missing values are broken", () => {
  expect(isViewportBroken(0, 1400, 900)).toBe(true);
  expect(isViewportBroken(1, 0, 900)).toBe(true);
  expect(isViewportBroken(1, 1400, -1)).toBe(true);
  expect(isViewportBroken(Number.NaN, 1400, 900)).toBe(true);
});
