import { expect, test } from "vitest";
import { relativeTime } from "./time";

const now = Date.parse("2026-03-10T12:00:00Z");

test("relative times", () => {
  expect(relativeTime("2026-03-10T11:59:30Z", now)).toBe("just now");
  expect(relativeTime("2026-03-10T11:59:00Z", now)).toBe("1 minute ago");
  expect(relativeTime("2026-03-10T11:15:00Z", now)).toBe("45 minutes ago");
  expect(relativeTime("2026-03-10T11:00:00Z", now)).toBe("1 hour ago");
  expect(relativeTime("2026-03-09T12:00:00Z", now)).toBe("1 day ago");
  expect(relativeTime("2026-03-05T12:00:00Z", now)).toBe("5 days ago");
});

test("old timestamps show the date", () => {
  expect(relativeTime("2026-01-02T03:04:05Z", now)).toBe("2026-01-02");
});

test("future and unparseable timestamps do not break", () => {
  expect(relativeTime("2026-03-10T12:05:00Z", now)).toBe("just now");
  expect(relativeTime("not a date", now)).toBe("not a date");
});
