import { expect, test } from "vitest";

import { argsParts, commandParts, nameParts, pathParts, programOf, wordsShown } from "./path";

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

test("a command's arguments keep the folder or file at their end, and give up what comes before it (#2518)", () => {
  // An *Open in…* line: the checkout's folder name is what says which one it opens.
  expect(argsParts(String.raw`"C:\Users\some one\source\repos\payout-service-the-repository-name-that-is-long"`)).toEqual({
    head: String.raw`"C:\Users\some one\source\repos\\`.slice(0, -1),
    tail: 'payout-service-the-repository-name-that-is-long"',
    after: "",
  });
  expect(argsParts("--goto /Users/mara/src/payout-service/src/sepa/retry.rs:12")).toEqual({ head: "--goto /Users/mara/src/payout-service/src/sepa/", tail: "retry.rs:12", after: "" });
  // A quoted last part with a space in it is kept whole, its closing quote too.
  expect(argsParts(String.raw`--line 3 "C:\Users\mara\notes\a file of mine.md"`)).toEqual({ head: String.raw`--line 3 "C:\Users\mara\notes\\`.slice(0, -1), tail: 'a file of mine.md"', after: "" });
  // Arguments after the path (the Diff tool's line): the path's folder is kept, and what follows it gives way at its end.
  expect(argsParts(String.raw`-C C:\Users\mara\src\payout-service difftool --gui --no-prompt 5d77e1a c90d11e --`)).toEqual({
    head: String.raw`-C C:\Users\mara\src\\`.slice(0, -1),
    tail: "payout-service",
    after: " difftool --gui --no-prompt 5d77e1a c90d11e --",
  });
  // Arguments with no path in them have no end to keep: all of them give way, at their end.
  expect(argsParts("--goto {file}:{line}")).toEqual({ head: "--goto {file}:{line}", tail: "", after: "" });
  expect(argsParts("")).toEqual({ head: "", tail: "", after: "" });
});

test("a list shows its first three and counts the rest", () => {
  expect(wordsShown(["a", "b", "c", "d", "e"])).toEqual({ shown: ["a", "b", "c"], more: 2 });
  expect(wordsShown(["a", "b", "c"])).toEqual({ shown: ["a", "b", "c"], more: 0 });
  expect(wordsShown(["a"])).toEqual({ shown: ["a"], more: 0 });
  expect(wordsShown([])).toEqual({ shown: [], more: 0 });
});

test("a file's name gives up its middle and keeps its last characters and its extension (#2531)", () => {
  // The end keeps the extension and the twelve characters before it, so a version or a date at the end still shows.
  expect(nameParts("payout-db client certificate bundle 2026-11.p12")).toEqual({ head: "payout-db client certificate bu", tail: "ndle 2026-11.p12" });
  expect(nameParts("wg.conf")).toEqual({ head: "", tail: "wg.conf" });
  // No extension: the last twelve characters.
  expect(nameParts("a-very-long-name-without-any-extension")).toEqual({ head: "a-very-long-name-without-a", tail: "ny-extension" });
  // A dot that starts the name, or words after the last dot, is no extension.
  expect(nameParts(".config-for-the-payments-team-hub")).toEqual({ head: ".config-for-the-payme", tail: "nts-team-hub" });
  expect(nameParts("release.notes-for-the-whole-payments-team")).toEqual({ head: "release.notes-for-the-whole-p", tail: "ayments-team" });
  // Head and tail always make the whole name, a character outside the BMP never split.
  for (const name of ["x.tar.gz", "ledger-signing.pem", "Ünïcödé file name with spaces and more words.docx", "a long name with an emoji 🔑🔑🔑 at the end.txt"]) {
    const { head, tail } = nameParts(name);
    expect(head + tail).toBe(name);
    expect(tail.charCodeAt(0) >= 0xdc00 && tail.charCodeAt(0) <= 0xdfff).toBe(false);
  }
});
