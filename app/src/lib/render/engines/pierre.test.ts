/**
 * Pierre Highlights as an engine, with the real package in node (the highlighter re-ruling, 2026-11-21, test 1): its
 * token names pinned, each one in the role table, what a Java file answers, the stream past 20,000 lines, the focus
 * first and a cancel between pushes.
 */
import { codeToTokens, tokenNames } from "@pierre/highlights";
import { expect, test } from "vitest";

import { createHighlightCore, HIGHLIGHT_MAX_LINES } from "../highlight-core";
import type { HighlightReply, LineTokens } from "../port";
import { PIERRE_ROLE, pierreRole } from "../roles";
import { CHUNK, type RunContext } from "./engine";
import { createPierre, KNOBAS, ONE_CALL, PUSH } from "./pierre";

/** The 75 names of 0.5.2, in its order: a bump that changes them fails here and is read. */
const NAMES = [
  "none", "attribute", "attribute.jsx", "boolean", "comment", "comment.doc", "constant", "constant.builtin", "constructor", "diff.minus",
  "diff.plus", "embedded", "emphasis", "emphasis.strong", "function", "function.definition", "function.method", "keyword", "keyword.control",
  "keyword.declaration", "keyword.import", "keyword.operator", "label", "namespace", "number", "operator", "predictive", "preproc", "primary",
  "property", "property.json_key", "punctuation", "punctuation.bracket", "punctuation.bracket.html", "punctuation.bracket.jsx",
  "punctuation.delimiter", "punctuation.delimiter.html", "punctuation.delimiter.jsx", "punctuation.list_marker", "punctuation.markup",
  "punctuation.special", "selector", "selector.class", "selector.id", "string", "string.escape", "string.regex", "string.special",
  "string.special.symbol", "tag", "tag.doctype", "tag.jsx", "text.jsx", "text.literal", "title", "type", "type.builtin", "type.class", "variable",
  "variable.parameter", "variable.special", "emphasis.strong.italic", "enum", "hint", "keyword.jsdoc", "link_text", "link_uri",
  "markup.strikethrough", "selector.pseudo", "tag.component.jsx", "type.jsdoc", "variable.jsdoc", "variant", "background", "foreground",
];
/** The names that draw plain, on purpose. */
const PLAIN = ["none", "variable", "namespace", "embedded", "primary", "text-jsx", "markup-strikethrough", "diff-plus", "diff-minus", "background", "foreground"];

const dashed = (name: string) => name.replace(/[._]/g, "-");

test("Pierre's token names are the 75 of 0.5.2, and each is in the role table or plain on purpose", () => {
  expect([...tokenNames]).toEqual(NAMES);
  for (const name of NAMES) {
    const key = dashed(name);
    if (PLAIN.includes(key)) expect(pierreRole(key), key).toBe("");
    else expect(PIERRE_ROLE.has(key), key).toBe(true);
  }
});

const SNIPPETS: [lang: string, code: string][] = [
  ["java", '@Override\npublic String name() { return "a\\n" + 42; } // done'],
  ["typescript", 'import { a } from "b";\nconst o = { k: /x+/g, n: null };'],
  ["markdown", "# Title\n\nSee [the notes](https://example.test) and **bold** and *it*.\n- item"],
  ["json", '{"a": [true, 1.5, "x"]}'],
  ["yaml", "key: value\nlist:\n  - name: x"],
  ["xml", '<?xml version="1.0"?>\n<a href="x"><!-- c --></a>'],
  ["rust", "#[derive(Debug)]\nfn main() { let s: String = String::new(); }"],
  ["python", "@dataclass\nclass A:\n    def f(self, x): return f'{x}'"],
  ["css", ".a #b:hover { color: red; }"],
  ["bash", 'export A=1; echo "$A" | grep -q x'],
];

test("ten snippets give only colours that name a type, spelled with dashes", () => {
  for (const [lang, code] of SNIPPETS) {
    const { tokens } = codeToTokens(code, { lang: lang as never, theme: KNOBAS, tokenizeMaxLineLength: 4000 });
    for (const token of tokens.flat()) {
      const color = token.color ?? "";
      expect(color, `${lang} ${token.content}`).toMatch(/^var\(--hls-[a-z-]+\)$/);
      const name = color.slice("var(--hls-".length, -1);
      expect(PLAIN.includes(name) || PIERRE_ROLE.has(name), `${lang}: ${name}`).toBe(true);
    }
  }
});

/** The core with Pierre, its replies collected. */
function core() {
  const replies: HighlightReply[] = [];
  let finished: () => void = () => {};
  const done = new Promise<void>((resolve) => (finished = resolve));
  const handle = createHighlightCore({
    post: (reply) => {
      replies.push(reply);
      if (reply.type === "done" || reply.type === "plain") finished();
    },
  });
  const lineAt = (n: number): LineTokens | undefined => {
    let found: LineTokens | undefined;
    for (const reply of replies) if (reply.type === "lines" && n >= reply.from && n < reply.from + reply.lines.length) found = reply.lines[n - reply.from];
    return found;
  };
  return { ...handle, replies, done, lineAt };
}
const kinds = (line: LineTokens | undefined) => (line ?? []).filter(([, kind]) => kind !== "").map(([text, kind]) => `${kind}:${text.trim()}`);

test("a Java file comes back line by line, its tokens adding up to the text, in the code roles", async () => {
  const c = core();
  const text = '@Override\npublic String name(int n) {\n    // why\n    return "x" + Foo.bar(n);\n}';
  c.handle({ type: "open", id: 1, path: "A.java", text, engine: "pierre" });
  await c.done;
  text.split("\n").forEach((line, i) => expect(c.lineAt(i)!.map(([t]) => t).join("")).toBe(line));
  const all = text.split("\n").flatMap((_, i) => kinds(c.lineAt(i)));
  for (const role of ["attr:", "kw:", "ty:", "fn:", "str:", "com:"]) expect(all.some((k) => k.startsWith(role)), role).toBe(true);
  expect(c.replies.at(-1)).toEqual({ type: "done", id: 1 });
});

test("a Markdown link is a link (its type is link-text and link-uri, not link_text)", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "README.md", text: "See [the notes](https://example.test).", engine: "pierre" });
  await c.done;
  expect(kinds(c.lineAt(0)).some((k) => k.startsWith("link:"))).toBe(true);
});

/** A run of `text` through the engine alone, its posts collected. */
async function run(text: string, lang: string, focus: [number, number] | null = null, cancelAfter = Infinity) {
  const posts: { from: number; lines: LineTokens[] }[] = [];
  let pauses = 0;
  let cancelled = false;
  let held = focus;
  const ctx: RunContext = {
    post: (from, lines) => void (!cancelled && posts.push({ from, lines })),
    pause: async () => {
      pauses += 1;
      if (pauses >= cancelAfter) cancelled = true;
    },
    cancelled: () => cancelled,
    takeFocus: () => {
      const f = held;
      held = null;
      return f;
    },
  };
  await createPierre().run(text, lang, lang, ctx);
  const lines: LineTokens[] = [];
  for (const post of posts) post.lines.forEach((line, i) => (lines[post.from + i] = line));
  return { posts, lines, pauses };
}

test("past 20,000 lines the stream carries the lexer's state: it equals one call on the first 20,000 lines", async () => {
  const body = ["/* opened in the first push", ...Array.from({ length: PUSH + 10 }, (_, i) => `still comment ${i}`), "*/"];
  const tail = Array.from({ length: 25_000 - body.length }, (_, i) => `const v${i} = ${i};`);
  const text = [...body, ...tail].join("\n");
  const streamed = await run(text, "typescript");
  expect(streamed.pauses).toBeGreaterThan(0);
  expect(streamed.lines).toHaveLength(25_000);
  // The block comment opened in push 1 colours a line of push 2.
  expect(kinds(streamed.lines[PUSH + 5])).toEqual([`com:still comment ${PUSH + 4}`]);
  const first = text.split("\n").slice(0, ONE_CALL).join("\n");
  const whole = await run(first, "typescript");
  for (const n of [0, 1, PUSH - 1, PUSH, PUSH + 11, 12_345, ONE_CALL - 2]) expect(streamed.lines[n], `line ${n}`).toEqual(whole.lines[n]);
});

test("the slice holding the focus is answered first", async () => {
  const text = Array.from({ length: 5 * CHUNK }, (_, i) => `let v${i} = ${i};`).join("\n");
  const { posts } = await run(text, "typescript", [3 * CHUNK + 10, 3 * CHUNK + 40]);
  expect(posts[0]!.from).toBe(3 * CHUNK);
  expect(posts.map((p) => p.from).sort((a, b) => a - b)).toEqual([0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK]);
});

test("a cancel between pushes stops the posts", async () => {
  const text = Array.from({ length: 25_000 }, (_, i) => `let v${i} = ${i};`).join("\n");
  const { lines } = await run(text, "typescript", null, 1);
  expect(lines.filter(Boolean).length).toBe(PUSH);
});

test("a text of 100,001 lines is answered plain", async () => {
  const c = core();
  c.handle({ type: "open", id: 1, path: "big.ts", text: "x\n".repeat(HIGHLIGHT_MAX_LINES), engine: "pierre" });
  await c.done;
  expect(c.replies).toEqual([{ type: "plain", id: 1 }]);
});
