/**
 * The highlighting worker's core over the real Shiki (#2254): what a file
 * opened gets back, in which order, and which grammars it loads.
 */
import { expect, test } from "vitest";

import { CHUNK, createHighlightCore, loadBundled, languageOf } from "./highlight-core";
import type { HighlightReply, LineTokens } from "./port";

/** A core whose replies are collected, with the grammars it loaded. */
function core() {
  const replies: HighlightReply[] = [];
  const grammars: string[] = [];
  let finished: () => void = () => {};
  const done = new Promise<void>((resolve) => (finished = resolve));
  const handle = createHighlightCore({
    post: (reply) => {
      replies.push(reply);
      if (reply.type !== "lines") finished();
    },
    load: (id) => {
      grammars.push(id);
      return loadBundled(id);
    },
  });
  /** Line `n`'s tokens, from the last reply that covered it. */
  const lineAt = (n: number): LineTokens | undefined => {
    let found: LineTokens | undefined;
    for (const reply of replies) if (reply.type === "lines" && n >= reply.from && n < reply.from + reply.lines.length) found = reply.lines[n - reply.from];
    return found;
  };
  return { ...handle, replies, grammars, done, lineAt };
}

const kinds = (line: LineTokens | undefined) => (line ?? []).filter(([, kind]) => kind !== "").map(([text, kind]) => `${kind}:${text.trim()}`);

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

test("a file in no language Shiki knows is answered plain, and no grammar loads", async () => {
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
  expect(kinds(c.lineAt(3_010))).toEqual(["kw:let", "num:3010"]);
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

test("a file's name picks its language", () => {
  expect(languageOf("src/main.rs")).toBe("rust");
  expect(languageOf("web/client.ts")).toBe("typescript");
  expect(languageOf("README.md")).toBe("markdown");
  expect(languageOf(".teamcity/settings.kts")).toBe("kotlin");
  expect(languageOf("Dockerfile")).toBe("docker");
  expect(languageOf("LICENSE")).toBeNull();
  expect(languageOf("data.zzq")).toBeNull();
});
