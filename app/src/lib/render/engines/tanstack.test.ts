/**
 * TanStack Highlight as an engine, with the real package in node (the highlighter re-ruling, 2026-11-21, test 2): its
 * twenty classes, a language it lacks throws, a long line is one plain token, XML goes in as HTML, and a file it lacks
 * reaches the next engine.
 */
import { createHighlighter, type HighlightTokenClass } from "@tanstack/highlight/core";
import * as languages from "@tanstack/highlight/languages";
import { expect, test } from "vitest";

import { createHighlightCore } from "../highlight-core";
import type { HighlightReply, LineTokens } from "../port";
import { TANSTACK_ROLE } from "../roles";
import type { RunContext } from "./engine";
import { createTanstack, linesOf } from "./tanstack";

const CLASSES: HighlightTokenClass[] = [
  "attr", "code-inline", "command", "comment", "deleted", "function", "heading", "inserted", "keyword", "link",
  "literal", "meta", "number", "operator", "property", "selector", "string", "tag", "type", "variable",
];

const SNIPPETS: [lang: string, code: string][] = [
  ["java", '@Override\npublic String name() { return "a" + 42; } // done'],
  ["ts", 'import { a } from "b";\nconst o = { k: 1, n: null }; o.k = 2;'],
  ["markdown", "# Title\n\nSee [the notes](https://example.test) and `code`."],
  ["json", '{"a": [true, 1.5, "x"]}'],
  ["yaml", "key: value\nlist:\n  - name: x"],
  ["html", '<a href="x"><!-- c --></a>'],
  ["css", ".a #b:hover { color: red; }"],
  ["shell", 'export A=1; echo "$A" | grep -q x'],
  ["diff", "--- a\n+++ b\n-old\n+new"],
  ["python", "@dataclass\nclass A:\n    def f(self, x): return x"],
  ["sql", "SELECT id FROM t WHERE n = 1;"],
  ["rust", "fn main() { let s: String = String::new(); }"],
  ["kotlin", 'fun main(args: Array<String>) { val x = "a" }'],
  ["makefile", "all: build\n\t$(CC) -o out main.c"],
];

test("fourteen snippets produce exactly TanStack's twenty classes, each in the role table", () => {
  const h = createHighlighter({ languages: Object.values(languages) });
  const seen = new Set<string>();
  for (const [lang, code] of SNIPPETS) for (const token of h.tokenize(code, { lang }).tokens) if (token.className) seen.add(token.className);
  for (const name of seen) expect(CLASSES, name).toContain(name);
  expect(Object.keys(TANSTACK_ROLE).sort()).toEqual([...CLASSES].sort());
});

async function run(text: string, name: string, lang = name) {
  const posts: { from: number; lines: LineTokens[] }[] = [];
  const ctx: RunContext = { post: (from, lines) => void posts.push({ from, lines }), pause: async () => {}, cancelled: () => false, takeFocus: () => null };
  await createTanstack().run(text, name, lang, ctx);
  const lines: LineTokens[] = [];
  for (const post of posts) post.lines.forEach((line, i) => (lines[post.from + i] = line));
  return lines;
}
const kinds = (line: LineTokens | undefined) => (line ?? []).filter(([, kind]) => kind !== "").map(([text, kind]) => `${kind}:${text.trim()}`);

test("a C preprocessor line (TanStack's meta) is an attribute", async () => {
  const lines = await run("#include <stdio.h>", "c");
  expect(kinds(lines[0])).toContain("attr:#include");
});

test("an annotation is an attribute, and a Java line's tokens add up to it", async () => {
  const text = '@Override\npublic String name() { return "a"; }';
  const lines = await run(text, "java");
  expect(kinds(lines[0])).toContain("attr:@Override");
  text.split("\n").forEach((line, i) => expect(lines[i]!.map(([t]) => t).join("")).toBe(line));
});

test("a language it lacks throws, and is not answered as plain text", async () => {
  await expect(run('resource "x" "y" {}', "terraform")).rejects.toThrow(/no terraform/);
});

test("a line of 4,000 characters or more is one plain token", () => {
  const long = `const a = "${"x".repeat(4000)}";`;
  expect(linesOf([{ value: "const", className: "keyword" }, { value: long.slice(5) }], "typescript")).toEqual([[[long, ""]]]);
});

test("XML is cut through HTML", async () => {
  const lines = await run('<dependency scope="test"/>', "html", "xml");
  expect(kinds(lines[0])).toEqual(expect.arrayContaining(["tag:dependency", "attr:scope"]));
});

test("under TanStack Terraform is Pierre's; under Pierre SQL is TanStack's, and Pierre is not loaded for it", async () => {
  const replies: HighlightReply[] = [];
  const loaded: string[] = [];
  const core = createHighlightCore({
    post: (reply) => void replies.push(reply),
    engines: {
      tanstack: async () => (loaded.push("tanstack"), createTanstack()),
      pierre: async () => (loaded.push("pierre"), (await import("./pierre")).createPierre()),
      shiki: async () => (loaded.push("shiki"), (await import("./shiki")).createShiki()),
    },
  });
  const until = async (n: number) => {
    for (let i = 0; i < 200 && replies.filter((r) => r.type === "done").length < n; i += 1) await new Promise((resolve) => setTimeout(resolve, 5));
  };
  core.handle({ type: "open", id: 1, path: "main.tf", text: 'resource "aws_s3_bucket" "b" {}', engine: "tanstack" });
  await until(1);
  expect(loaded).toEqual(["pierre"]);
  core.handle({ type: "open", id: 2, path: "q.sql", text: "SELECT id FROM t WHERE n = 1;", engine: "pierre" });
  await until(2);
  expect(loaded).toEqual(["pierre", "tanstack"]);
  const sql = replies.find((r) => r.type === "lines" && r.id === 2);
  expect(sql?.type === "lines" && sql.lines[0]!.some(([text, kind]) => text === "SELECT" && kind === "kw")).toBe(true);
});
