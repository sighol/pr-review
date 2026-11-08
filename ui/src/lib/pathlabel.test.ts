import { describe, expect, test } from "vitest";
import { compressPath } from "./pathlabel";

describe("compressPath", () => {
  test("leaves short paths untouched", () => {
    expect(compressPath("api/alpha/MyFile.kt", 60)).toBe("api/alpha/MyFile.kt");
  });

  test("leaves paths with no middle directories untouched even when long", () => {
    const path = "a-very-very-very-very-very-very-long-file-name.kt";
    expect(compressPath(path, 10)).toBe(path);
  });

  test("collapses middle directories to their first letter until it fits", () => {
    const path = "api/alpha/bravo/charlie/delta/MyFile.kt";
    expect(compressPath(path, 22)).toBe("api/a/b/c/d/MyFile.kt");
  });

  test("stops once the path fits, leaving other middle directories intact", () => {
    const path = "api/alpha/bravo/MyFile.kt";
    expect(compressPath(path, 22)).toBe("api/alpha/b/MyFile.kt");
  });

  test("never touches the first directory or the filename", () => {
    const path = "api/alpha/bravo/charlie/delta/MyFile.kt";
    expect(compressPath(path, 1)).toBe("api/a/b/c/d/MyFile.kt");
  });
});
