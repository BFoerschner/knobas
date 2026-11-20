import { expect, test } from "vitest";

import { commandParts, pathParts, programOf, wordsShown } from "./path";

test("a path splits after its last separator, so the folder or file it names is its end", () => {
  expect(pathParts("/Users/mara/src/payout-service")).toEqual({ head: "/Users/mara/src/", tail: "payout-service" });
  expect(pathParts(String.raw`C:\Users\some one\source\repos\the-repository-name-that-is-long`)).toEqual({
    head: String.raw`C:\Users\some one\source\repos\\`.slice(0, -1),
    tail: "the-repository-name-that-is-long",
  });
  // A command holding a path keeps the path's end, whatever comes before it.
  expect(pathParts("code /Users/mara/src/payout-service")).toEqual({ head: "code /Users/mara/src/", tail: "payout-service" });
});

test("a trailing separator stays on the end, and a path with none is all end", () => {
  expect(pathParts("/Users/mara/src/")).toEqual({ head: "/Users/mara/", tail: "src/" });
  expect(pathParts("payout-service")).toEqual({ head: "", tail: "payout-service" });
  expect(pathParts("/")).toEqual({ head: "", tail: "/" });
  expect(pathParts(String.raw`C:\\`.slice(0, -1))).toEqual({ head: "", tail: "C:\\" });
});

test("a command's program is its first word, quotes grouping it as the backend splits it", () => {
  expect(programOf(String.raw`"C:\Program Files\Some Vendor\bin\orchestra-render.exe" --profile "C:\Users\x\a b.json"`)).toBe(
    String.raw`C:\Program Files\Some Vendor\bin\orchestra-render.exe`,
  );
  expect(programOf("  plantuml -tsvg -pipe")).toBe("plantuml");
  expect(programOf("'/Users/some one/bin/tool'")).toBe("/Users/some one/bin/tool");
  expect(programOf("/opt/x/b\"a b\"c --y")).toBe("/opt/x/ba bc");
  expect(programOf("")).toBe("");
});

test("a command splits into its first word as typed, cut as a path, and the rest (the renderers-columns pass)", () => {
  expect(commandParts(String.raw`"C:\Program Files\Vendor\bin\render.exe" --profile "C:\Users\a b.json"`)).toEqual({
    head: '"C:\\Program Files\\Vendor\\bin\\',
    tail: 'render.exe"',
    rest: String.raw`--profile "C:\Users\a b.json"`,
  });
  expect(commandParts("  plantuml   -tsvg -pipe ")).toEqual({ head: "", tail: "plantuml", rest: "-tsvg -pipe" });
  expect(commandParts("/usr/local/bin/d2 - -")).toEqual({ head: "/usr/local/bin/", tail: "d2", rest: "- -" });
  expect(commandParts("'/Users/some one/bin/tool'")).toEqual({ head: "'/Users/some one/bin/", tail: "tool'", rest: "" });
  expect(commandParts("")).toEqual({ head: "", tail: "", rest: "" });
});

test("a list shows its first three and counts the rest", () => {
  expect(wordsShown(["a", "b", "c", "d", "e"])).toEqual({ shown: ["a", "b", "c"], more: 2 });
  expect(wordsShown(["a", "b", "c"])).toEqual({ shown: ["a", "b", "c"], more: 0 });
  expect(wordsShown(["a"])).toEqual({ shown: ["a"], more: 0 });
  expect(wordsShown([])).toEqual({ shown: [], more: 0 });
});
