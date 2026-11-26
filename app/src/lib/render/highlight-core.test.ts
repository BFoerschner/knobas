/**
 * The worker's core across engines (the highlighter re-ruling, 2026-11-21, tests 7 and 8): the net when an engine
 * throws or cannot load, and that an engine is imported only when a job needs it. Stub engines; each engine's own
 * answers are `engines/*.test.ts`.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import type { Engine } from "./engines/engine";
import { createHighlightCore } from "./highlight-core";
import type { EngineId } from "./roles";
import type { HighlightReply } from "./port";

/** An engine that answers every line as one token of `role`, or throws. */
const stub = (role: string, fail = false): Engine => ({
  async run(text, _name, _lang, ctx) {
    if (fail) throw new Error(`${role} fails`);
    ctx.post(0, text.split("\n").map((line) => [[line, role] as const]));
  },
});

function core(engines: Partial<Record<EngineId, () => Promise<Engine>>>) {
  const replies: HighlightReply[] = [];
  const warned: string[] = [];
  const c = createHighlightCore({ post: (reply) => void replies.push(reply), engines, warn: (text) => void warned.push(text) });
  const settle = async () => {
    for (let i = 0; i < 50; i += 1) await new Promise((resolve) => setTimeout(resolve, 1));
  };
  return { ...c, replies, warned, settle };
}

test("a Pierre that throws: TanStack answers Java, Shiki a language only it has, and all three throwing is plain", async () => {
  const c = core({ pierre: async () => stub("kw", true), tanstack: async () => stub("str"), shiki: async () => stub("num") });
  c.handle({ type: "open", id: 1, path: "A.java", text: "x", engine: "pierre" });
  c.handle({ type: "open", id: 2, path: "a.raku", text: "x", engine: "pierre" });
  await c.settle();
  expect(c.replies.filter((r) => r.type !== "failed")).toEqual(
    expect.arrayContaining([
      { type: "lines", id: 1, from: 0, lines: [[["x", "str"]]] },
      { type: "done", id: 1 },
      { type: "lines", id: 2, from: 0, lines: [[["x", "num"]]] },
      { type: "done", id: 2 },
    ]),
  );
  expect(c.warned).toHaveLength(1);
  const all = core({ pierre: async () => stub("kw", true), tanstack: async () => stub("str", true), shiki: async () => stub("num", true) });
  all.handle({ type: "open", id: 3, path: "A.java", text: "x", engine: "pierre" });
  await all.settle();
  expect(all.replies).toEqual([{ type: "plain", id: 3 }]);
});

test("an engine that throws after taking the focus leaves it to the next, which answers the lines in view first", async () => {
  const throwing: Engine = {
    async run(_text, _name, _lang, ctx) {
      ctx.takeFocus();
      throw new Error("half way");
    },
  };
  const firstOf: Engine = {
    async run(_text, _name, _lang, ctx) {
      const focus = ctx.takeFocus();
      ctx.post(focus?.[0] ?? 0, [[["x", "kw"]]]);
    },
  };
  const c = core({ pierre: async () => throwing, tanstack: async () => firstOf });
  c.handle({ type: "open", id: 1, path: "A.java", text: "x\n".repeat(5000), engine: "pierre", focus: [3000, 3040] });
  await c.settle();
  expect(c.replies[0]).toMatchObject({ type: "lines", from: 3000 });
});

test("an engine whose import fails is said once, and no later job imports it again", async () => {
  let tries = 0;
  const c = core({
    pierre: () => {
      tries += 1;
      return Promise.reject(new Error("no WebAssembly"));
    },
    tanstack: async () => stub("str"),
  });
  c.handle({ type: "open", id: 1, path: "A.java", text: "x", engine: "pierre" });
  await c.settle();
  c.handle({ type: "open", id: 2, path: "B.java", text: "y", engine: "pierre" });
  await c.settle();
  expect(c.replies.filter((r) => r.type === "failed")).toEqual([{ type: "failed", engine: "pierre" }]);
  expect(tries).toBe(1);
  expect(c.replies.filter((r) => r.type === "done")).toHaveLength(2);
  // *Try again*: the failure is forgotten and the import tried once more.
  c.handle({ type: "warm", engine: "pierre", retry: true });
  await c.settle();
  expect(tries).toBe(2);
});

test("a Java job under Pierre imports Pierre alone; SQL imports TanStack and not Shiki; a warm-up imports the chosen one", async () => {
  const loaded: string[] = [];
  const count = (id: EngineId, role: string) => async () => (loaded.push(id), stub(role));
  const c = core({ pierre: count("pierre", "kw"), tanstack: count("tanstack", "str"), shiki: count("shiki", "num") });
  c.handle({ type: "open", id: 1, path: "A.java", text: "x", engine: "pierre" });
  await c.settle();
  expect(loaded).toEqual(["pierre"]);
  c.handle({ type: "open", id: 2, path: "q.sql", text: "x", engine: "pierre" });
  await c.settle();
  expect(loaded).toEqual(["pierre", "tanstack"]);
  const warm = core({ pierre: count("pierre", "kw"), tanstack: count("tanstack", "str"), shiki: count("shiki", "num") });
  loaded.length = 0;
  warm.handle({ type: "warm", engine: "tanstack" });
  await warm.settle();
  expect(loaded).toEqual(["tanstack"]);
});

test("a cancelled job posts nothing more", async () => {
  const c = core({ pierre: async () => stub("kw") });
  c.handle({ type: "open", id: 1, path: "A.java", text: "x", engine: "pierre" });
  c.handle({ type: "cancel", id: 1 });
  await c.settle();
  expect(c.replies).toEqual([]);
});

test("the core, the walk and the language table import no engine's package", () => {
  const imports = (file: string) => [...readFileSync(join(process.cwd(), "src/lib/render", file), "utf8").matchAll(/^import[^;]*?from\s+"([^"]+)"/gms)].map((m) => m[1]!);
  for (const file of ["highlight-core.ts", "engines.ts", "languages.ts", "highlight.worker.ts", "roles.ts", "port.ts"])
    for (const from of imports(file)) expect(from, `${file} imports ${from}`).not.toMatch(/^(shiki|@pierre\/highlights|@tanstack\/highlight)/);
});
