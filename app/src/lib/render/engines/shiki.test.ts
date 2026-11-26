/**
 * Shiki as an engine, through the worker's core with Shiki chosen (#2254; the highlighter re-ruling, 2026-11-21): what
 * a file opened gets back, in which order, which grammars it loads, and its scopes' roles.
 */
import { expect, test } from "vitest";

import { CHUNK, createHighlightCore, languageOf } from "../highlight-core";
import type { HighlightMessage, HighlightReply, LineTokens } from "../port";
import { createShiki, loadBundled } from "./shiki";

/**
 * A core whose replies are collected, with the grammars it loaded. A fresh
 * highlighter compiles its grammar's patterns inside the first line's time
 * limit, which a loaded machine can spend before the line ends, so the helper
 * sets no bound a test could race (#2457).
 */
function core(tokenizeTimeLimit = 60_000) {
  const replies: HighlightReply[] = [];
  const grammars: string[] = [];
  let finished: () => void = () => {};
  const done = new Promise<void>((resolve) => (finished = resolve));
  const shiki = createShiki({
    tokenizeTimeLimit,
    load: (id: string) => {
      grammars.push(id);
      return loadBundled(id);
    },
  });
  const inner = createHighlightCore({
    post: (reply) => {
      replies.push(reply);
      if (reply.type !== "lines") finished();
    },
    engines: { shiki: async () => shiki },
  });
  // Every job asks Shiki.
  const handle = { handle: (message: HighlightMessage) => inner.handle(message.type === "open" ? { ...message, engine: "shiki" } : message) };
  /** Line `n`'s tokens, from the last reply that covered it. */
  const lineAt = (n: number): LineTokens | undefined => {
    let found: LineTokens | undefined;
    for (const reply of replies) if (reply.type === "lines" && n >= reply.from && n < reply.from + reply.lines.length) found = reply.lines[n - reply.from];
    return found;
  };
  return { ...handle, replies, grammars, done, lineAt };
}

const kinds = (line: LineTokens | undefined) => (line ?? []).filter(([, kind]) => kind !== "").map(([text, kind]) => `${kind}:${text.trim()}`);

test("a line that runs out of its time limit ends in one token holding the rest of it", async () => {
  const c = core(1e-6);
  c.handle({ type: "open", id: 1, path: "web/client.ts", text: "export const a: number = 1;" });
  await c.done;

  expect(c.lineAt(0)!.map(([text]) => text).join("")).toBe("export const a: number = 1;");
  expect(kinds(c.lineAt(0))).not.toContain("num:1");
});

test("a Rust file comes back as tokens of the knobas kinds, loading the Rust grammar alone", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "src/main.rs", text: 'fn main() {\n    // go\n    let n = 42;\n    run("now");\n}' });
  await c.done;

  expect(kinds(c.lineAt(0))).toContain("kw:fn");
  expect(kinds(c.lineAt(1))).toEqual(["com:// go"]);
  expect(kinds(c.lineAt(2))).toEqual(expect.arrayContaining(["kw:let", "num:42"]));
  expect(kinds(c.lineAt(3))).toContain('str:"now"');
  expect(c.lineAt(0)!.map(([text]) => text).join("")).toBe("fn main() {");
  expect(c.replies.at(-1)).toEqual({ type: "done", id: 1 });
  expect(c.grammars).toEqual(["rust"]);
});

test("opening a TypeScript file after it loads the TypeScript grammar and nothing else", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "a.rs", text: "fn a() {}" });
  await c.done;
  const second = core();
  second.handle({ type: "open", id: 2, path: "web/client.ts", text: "export const a: number = 1;" });
  await second.done;

  expect(c.grammars).toEqual(["rust"]);
  expect(second.grammars).toEqual(["typescript"]);
  expect(kinds(second.lineAt(0))).toEqual(expect.arrayContaining(["kw:export", "kw:const", "num:1"]));
});

test("a file in no language any engine knows is answered plain, and no grammar loads", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "notes/plan.zzq", text: "fn main() {}" });
  await c.done;

  expect(c.replies).toEqual([{ type: "plain", id: 1 }]);
  expect(c.grammars).toEqual([]);
});

test("the lines in view of a long file come back before the pass from the top reaches them", async () => {
  const c = core();
  const text = Array.from({ length: 10 * CHUNK }, (_, i) => `let v${i} = ${i};`).join("\n");
  c.handle({ type: "open", id: 1, path: "BULK.rs", text, focus: [3_000, 3_040] });
  await c.done;

  const first = c.replies[0];
  expect(first).toMatchObject({ type: "lines", from: 3_000 });
  expect(first?.type === "lines" && first.lines).toHaveLength(40);
  expect(kinds(c.lineAt(3_010))).toEqual(["kw:let", "op:=", "num:3010", "punct:;"]);
  expect(c.lineAt(10 * CHUNK - 1)).toBeDefined();
});

test("a block comment opened in the first chunk colours a line in a later one", async () => {
  const c = core();
  const text = ["/* opened here", ...Array.from({ length: CHUNK + 10 }, () => "still a comment"), "*/", "fn after() {}"].join("\n");
  c.handle({ type: "open", id: 1, path: "a.rs", text });
  await c.done;

  expect(kinds(c.lineAt(CHUNK + 5))).toEqual(["com:still a comment"]);
  expect(kinds(c.lineAt(CHUNK + 12))).toContain("kw:fn");
});

test("Java's class names are types and null a constant; an annotation is an attribute (ADR-0051)", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "A.java", text: "@Override\nString s = null;\nint n = 0;" });
  await c.done;

  expect(kinds(c.lineAt(0))).toEqual(["attr:@Override"]);
  expect(kinds(c.lineAt(1))).toEqual(["ty:String", "op:=", "con:null", "punct:;"]);
  expect(kinds(c.lineAt(2))).toEqual(["kw:int", "op:=", "num:0", "punct:;"]);
});

test("Rust's let and struct stay keywords, Kotlin's annotation is an attribute", async () => {
  const rust = core();
  rust.handle({ type: "open", id: 1, path: "a.rs", text: "struct S;\nlet x = 1;" });
  await rust.done;
  expect(kinds(rust.lineAt(0))).toContain("kw:struct");
  expect(kinds(rust.lineAt(1))).toContain("kw:let");

  const kotlin = core();
  kotlin.handle({ type: "open", id: 1, path: "a.kt", text: '@Suppress("x") class Q' });
  await kotlin.done;
  expect(kinds(kotlin.lineAt(0))).toEqual(expect.arrayContaining(["attr:@Suppress", "kw:class", "ty:Q"]));
});

test("in a data language a property is a key, whichever scope cut it", async () => {
  const yaml = core();
  yaml.handle({ type: "open", id: 1, path: "a.yaml", text: "name: knobas" });
  await yaml.done;
  expect(kinds(yaml.lineAt(0))).toEqual(["key:name", "op::", "str:knobas"]);

  const json = core();
  json.handle({ type: "open", id: 1, path: "a.json", text: '{"a": true}' });
  await json.done;
  expect(kinds(json.lineAt(0))).toEqual(expect.arrayContaining(["key:a", "con:true"]));
});

test("a TOML key and a dotenv key are keys", async () => {
  const toml = core();
  toml.handle({ type: "open", id: 1, path: "Cargo.toml", text: 'name = "knobas"' });
  await toml.done;
  expect(kinds(toml.lineAt(0))).toContain("key:name");
  const env = core();
  env.handle({ type: "open", id: 1, path: ".env", text: "RETRY_LIMIT=3" });
  await env.done;
  expect(kinds(env.lineAt(0))).toContain("key:RETRY_LIMIT");
});

test("a file's name picks its language", () => {
  expect(languageOf("src/main.rs")).toBe("rust");
  expect(languageOf("web/client.ts")).toBe("typescript");
  expect(languageOf("README.md")).toBe("markdown");
  expect(languageOf(".teamcity/settings.kts")).toBe("kotlin");
  expect(languageOf("Dockerfile")).toBe("docker");
  expect(languageOf("LICENSE")).toBeNull();
  expect(languageOf("data.zzq")).toBeNull();
});
