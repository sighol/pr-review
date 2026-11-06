import { expect, test } from "vitest";
import { langFor, tokenize } from "./highlight";

test("languages come from Shiki's own list, by extension", () => {
  expect(langFor("api/src/Foo.kt")).toBe("kt");
  expect(langFor("build.gradle.kts")).toBe("kts");
  expect(langFor("src/main.rs")).toBe("rs");
  expect(langFor("deploy/values.yaml")).toBe("yaml");
  expect(langFor("scripts/run.sh")).toBe("sh");
  expect(langFor("Main.JAVA")).toBe("java");
});

test("a few extensions and file names are mapped by hand", () => {
  expect(langFor("build.gradle")).toBe("groovy");
  expect(langFor("ui/vite.config.mjs")).toBe("javascript");
  expect(langFor("docker/Dockerfile")).toBe("dockerfile");
  expect(langFor("Makefile")).toBe("makefile");
});

test("unknown files get no language", () => {
  expect(langFor("LICENSE")).toBeNull();
  expect(langFor("data/blob.unknownext")).toBeNull();
  // Not a language even though it is a property of every object.
  expect(langFor("a.constructor")).toBeNull();
});

test("Kotlin is actually highlighted", async () => {
  const lines = await tokenize('fun main() { println("hi") }\n', "src/Main.kt");
  expect(lines).not.toBeNull();
  const colors = new Set(lines![0].map((t) => t.color));
  expect(colors.size).toBeGreaterThan(1);
});
