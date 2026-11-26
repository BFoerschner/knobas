/**
 * A theme's selected text (ADR-0055): the rules name `<html>`'s own `::selection`, which Chromium's highlights inherit,
 * and the rules for every element's only under `data-sel-each`, an engine whose highlights do not inherit. A rule for
 * every element's that matched in Chromium made each restyle with a theme's selection on cost about 1.8 times its
 * price without (2026-11-23, `testenv/perf/theme-switch.mjs`).
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { selectionInherits } from "./selection";

const APP = process.cwd();
const css = readFileSync(join(APP, "src/lib/tf/tf.css"), "utf8")
  .replace(/^@import "(\.\/css\/[^"]+)";$/gm, (_, part: string) => readFileSync(join(APP, "src/lib/tf", part), "utf8"))
  .replace(/\/\*[\s\S]*?\*\//g, "");

/** Every selector in the sheet that names `::selection` and reads `data-sel`. */
const themed = [...css.matchAll(/([^{}]+)\{[^{}]*\}/g)]
  .flatMap((m) => m[1]!.split(","))
  .map((s) => s.trim())
  .filter((s) => s.includes("::selection") && s.includes("data-sel"));

test("each side's two ways of being drawn have a rule on <html>'s own ::selection and one for every element's under data-sel-each", () => {
  const own = themed.filter((s) => /^:root\[[^\s]*::selection$/.test(s));
  const each = themed.filter((s) => /\s::selection$/.test(s));
  expect(own).toHaveLength(4);
  expect(each).toHaveLength(4);
  for (const side of ["light", "dark"]) expect(own.filter((s) => s.includes(`[data-sel~="${side}"]`)), side).toHaveLength(2);
  expect(themed).toHaveLength(own.length + each.length);
});

test("no rule for every element's ::selection reads data-sel without data-sel-each", () => {
  for (const selector of themed.filter((s) => /\s::selection$/.test(s))) expect(selector).toMatch(/^:root\[data-sel-each\]/);
});

test("highlights inherit where an element's ::selection reads its parent's, and not where it reads as its own or cannot be read", () => {
  const fake = (answer: string | (() => never)) => {
    const added: string[] = [];
    const el = (tag: string) => ({ tag, className: "", style: { cssText: "" }, textContent: "", append() {}, remove: () => added.push(`-${tag}`) });
    const doc = {
      body: { append: () => added.push("+body") },
      head: { append: () => added.push("+head") },
      createElement: el,
      defaultView: { navigator: { userAgent: "a browser" }, getComputedStyle: () => (typeof answer === "string" ? { backgroundColor: answer } : answer()) },
    };
    return { doc: doc as unknown as Document, added };
  };
  const yes = fake("rgb(1, 2, 3)");
  expect(selectionInherits(yes.doc)).toBe(true);
  expect(yes.added).toEqual(["+head", "+body", "-div", "-style"]);
  expect(selectionInherits(fake("rgba(0, 0, 0, 0)").doc)).toBe(false);
  const thrown = fake(() => { throw new Error("no pseudo"); });
  expect(selectionInherits(thrown.doc)).toBe(false);
  expect(thrown.added).toContain("-div");
  expect(selectionInherits(undefined)).toBe(false);
});
