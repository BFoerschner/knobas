/** Which highlighter draws a language under each choice (the highlighter re-ruling, 2026-11-21, test 6). */
import { expect, test } from "vitest";

import { engineOf, languageOf, languageOfFence, walk } from "./engines";

test("under Pierre: Java is Pierre's, SQL TanStack's, Crystal TanStack's, Raku Shiki's, a made-up name nobody's", () => {
  expect(walk("pierre", "java")[0]).toBe("pierre");
  expect(walk("pierre", "sql")).toEqual(["tanstack", "shiki", "pierre"]);
  expect(walk("pierre", "crystal")[0]).toBe("tanstack");
  expect(walk("pierre", "raku")).toEqual(["shiki"]);
  expect(walk("pierre", "made-up")).toEqual([]);
});

test("under TanStack: Terraform is Pierre's, SQL TanStack's", () => {
  expect(walk("tanstack", "terraform")[0]).toBe("pierre");
  expect(walk("tanstack", "sql")[0]).toBe("tanstack");
});

test("under Shiki: SQL is Shiki's, tsrx Pierre's, ejs TanStack's", () => {
  expect(walk("shiki", "sql")[0]).toBe("shiki");
  expect(walk("shiki", "tsrx")[0]).toBe("pierre");
  expect(walk("shiki", "ejs")).toEqual(["tanstack"]);
});

test("an engine that failed is skipped, and a weak one is tried last, never dropped", () => {
  expect(walk("pierre", "java", new Set(["pierre"]))[0]).toBe("tanstack");
  expect(walk("pierre", "sql", new Set(["tanstack", "shiki"]))).toEqual(["pierre"]);
});

test("a path, a fence and a kept value pick as before", () => {
  expect(languageOf("src/main.rs")).toBe("rust");
  expect(languageOf("Dockerfile")).toBe("docker");
  expect(languageOf(".teamcity/settings.kts")).toBe("kotlin");
  expect(languageOf("app.properties")).toBe("properties");
  expect(languageOf("LICENSE")).toBeNull();
  expect(languageOfFence("ts")).toBe("typescript");
  expect(languageOfFence("Bash")).toBe("shellscript");
  expect(engineOf(null)).toBe("pierre");
  expect(engineOf("shiki")).toBe("shiki");
  expect(engineOf("arborium")).toBe("pierre");
});
