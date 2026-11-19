import { expect, test } from "vitest";

import { pathParts, programOf } from "./path";

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
