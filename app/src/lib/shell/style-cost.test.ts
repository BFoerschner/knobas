/**
 * Stylesheet shapes that cost the engine a frame, refused here instead of found by a profiler (PR #2479, the
 * smoothness pass of 2026-11-11; `testenv/perf/README.md` measures them).
 *
 * A `:has()` before the subject of a selector, with a subject that names no class, id, tag or attribute, makes
 * Chromium (WebView2 on Windows) throw away the style of every element on the page whenever a row enters or leaves
 * any list: its one invalidation set for `:has()` becomes "the whole subtree". Three such rules in `job.css`
 * restyled 4,098 elements a pass on a Monitors scroll, 21 ms a frame. WebKit does not pay it, so the Mac hides it.
 *
 * Reads every stylesheet and every component's `<style>` off disk, with Svelte's own CSS parser.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { parse, parseCss } from "svelte/compiler";
import { expect, test } from "vitest";

/** `app/src/`, off `process.cwd()` (vitest's root is `app/`), as `house-rules.test.ts` finds it. */
const ROOT = join(process.cwd(), "src");

function walk(dir: string, pattern: RegExp, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, pattern, found);
    else if (pattern.test(entry.name)) found.push(path);
  }
  return found.sort();
}

// The parser's tree, as much of it as these rules read.
type Node = { type: string; name?: string; args?: Node | null; children?: Node[]; selectors?: Node[]; prelude?: Node | string; block?: Node | null; start: number; end: number };

/** Every selector list in a stylesheet, at-rules' blocks included. */
function selectorLists(sheet: Node): Node[] {
  const lists: Node[] = [];
  const visit = (node: Node | null | undefined) => {
    if (!node) return;
    if (node.type === "Rule" && typeof node.prelude === "object") lists.push(node.prelude);
    for (const child of node.children ?? []) visit(child);
    if (node.block) visit(node.block);
  };
  visit(sheet);
  return lists;
}

const PSEUDO_WITH_SELECTORS = new Set(["is", "where", "not", "has", "nth-child", "nth-last-child", "global"]);

/** Whether a compound names something the engine can key an invalidation on: a class, id, tag or attribute. */
function named(compound: Node[]): boolean {
  return compound.some((s) => {
    if (s.type === "ClassSelector" || s.type === "IdSelector" || s.type === "AttributeSelector") return true;
    if (s.type === "TypeSelector") return s.name !== "*";
    // `:is(a, .b)` names something only when every one of its arguments does.
    if (s.type === "PseudoClassSelector" && (s.name === "is" || s.name === "where" || s.name === "global") && s.args) {
      return (s.args.children ?? []).every((complex) => named(subjectOf(complex)));
    }
    return false;
  });
}

const subjectOf = (complex: Node): Node[] => complex.children?.[complex.children.length - 1]?.selectors ?? [];

/** Whether a compound holds a `:has()` anywhere inside it, `:not(:has(…))` included. */
function holdsHas(compound: Node[]): boolean {
  return compound.some((s) => s.type === "PseudoClassSelector" && (s.name === "has" || (s.args && (s.args.children ?? []).some((complex) => (complex.children ?? []).some((rel) => holdsHas(rel.selectors ?? []))))));
}

/** The complex selectors of a list whose `:has()` stands before an unnamed subject, nested lists included. */
function costly(list: Node): Node[] {
  const found: Node[] = [];
  for (const complex of list.children ?? []) {
    const compounds = (complex.children ?? []).map((rel) => rel.selectors ?? []);
    if (compounds.slice(0, -1).some(holdsHas) && !named(compounds[compounds.length - 1] ?? [])) found.push(complex);
    for (const compound of compounds) {
      for (const s of compound) if (s.type === "PseudoClassSelector" && PSEUDO_WITH_SELECTORS.has(s.name ?? "") && s.name !== "has" && s.args) found.push(...costly(s.args));
    }
  }
  return found;
}

/** Each stylesheet's source and parsed tree: the `.css` files, and the `<style>` of every component. */
function sheets(): { file: string; source: string; sheet: Node }[] {
  const out: { file: string; source: string; sheet: Node }[] = [];
  for (const file of walk(ROOT, /\.css$/)) {
    const source = readFileSync(file, "utf8");
    out.push({ file, source, sheet: parseCss(source) as unknown as Node });
  }
  for (const file of walk(ROOT, /\.svelte$/)) {
    const source = readFileSync(file, "utf8");
    if (!source.includes("<style")) continue;
    const css = (parse(source, { modern: true }) as unknown as { css: Node | null }).css;
    if (css) out.push({ file, source, sheet: css });
  }
  return out;
}

/** `file:line  selector` for each costly selector in `source`. */
function offenders(file: string, source: string, sheet: Node): string[] {
  return selectorLists(sheet).flatMap((list) =>
    costly(list).map((complex) => {
      const start = complex.start;
      const end = complex.end;
      return `${file.slice(ROOT.length + 1)}:${source.slice(0, start).split("\n").length}  ${source.slice(start, end)}`;
    }),
  );
}

test("no :has() stands before a subject that names no class, id, tag or attribute", () => {
  expect(sheets().flatMap(({ file, source, sheet }) => offenders(file, source, sheet))).toEqual([]);
});

test("the rule catches the shapes that restyled the whole page, and passes named subjects", () => {
  const caught = (css: string) => offenders(join(ROOT, "x.css"), css, parseCss(css) as unknown as Node).length;
  expect(caught(".tf-panel:has(> header) > :not(header) { color: red }")).toBe(1);
  expect(caught(".tf-panel:has(> header) > header + * { color: red }")).toBe(1);
  expect(caught(".tf-panel:has(> header) > :not(header):last-child { color: red }")).toBe(1);
  expect(caught(".a:not(:has(.b)) > * { color: red }")).toBe(1);
  expect(caught("@media (width > 1px) { .a:has(.b) :is(.c, *) { color: red } }")).toBe(1);
  expect(caught(".a:is(.b:has(.c) *) { color: red }")).toBe(1);
  expect(caught(".tf-panel:has(> header) > .tf-prpick { color: red }")).toBe(0);
  expect(caught(".a:has(.b) :is(.c, td) { color: red }")).toBe(0);
  expect(caught(".a:has(.b) h3 { color: red }")).toBe(0);
  expect(caught(".a:has(.b) > [data-x] { color: red }")).toBe(0);
  expect(caught(".a:has(> .b) { color: red }")).toBe(0);
  expect(caught(".tf-headed > :not(header) { color: red }")).toBe(0);
});

/**
 * The elements a `:has()` before the subject may stand on, each with why its subtree is small. Chromium keeps one
 * invalidation set for all such `:has()` rules, the union of their subjects; an element a rule stands on is checked at
 * each insertion under it, and on a change that set is applied to everything beneath it. On `body`, a board's scroller
 * or a table, that was every cell of every row on each row a scroll drew (2,646 cells a pass on Monitors). On a row it
 * is the row. So such a rule stands on a row-sized element, and a page-sized one gets a class from its markup instead.
 */
const HAS_ANCHORS: Record<string, string> = {
  ".tf-vw-row": "a saved view's row in the rail",
  ".tf-ib": "one inbox item",
  ".tf-member": "one member of a link set",
  ".tf-lk": "one link row",
  ".tf-pi": "one pipeline row",
  ".tf-status": "the status line's few chips",
  ".tf-stg-row": "one settings row",
  "tr": "one table row",
  ":is(tr, li, .tf-rowhost, .tf-r)": "one row",
  ":is(.tf-convo, .tf-cmts)": "a conversation's messages, which change on a post and never while a list scrolls",
};

/** `file:line  anchor` for each `:has()` before a subject that stands on an element `HAS_ANCHORS` does not list. */
function bigAnchors(file: string, source: string, sheet: Node): string[] {
  const found: string[] = [];
  for (const list of selectorLists(sheet)) {
    for (const complex of list.children ?? []) {
      const compounds = complex.children ?? [];
      for (const rel of compounds.slice(0, -1)) {
        const parts = rel.selectors ?? [];
        if (!holdsHas(parts)) continue;
        const head = parts.slice(0, Math.max(1, parts.findIndex((s) => s.type === "PseudoClassSelector" && ((s.name !== "is" && s.name !== "where") || holdsHas([s])))));
        const text = source.slice(head[0]!.start, head[head.length - 1]!.end);
        if (!(text in HAS_ANCHORS)) found.push(`${file.slice(ROOT.length + 1)}:${source.slice(0, complex.start).split("\n").length}  ${text}`);
      }
    }
  }
  return found;
}

test("a :has() before a subject stands only on a row-sized element", () => {
  expect(sheets().flatMap(({ file, source, sheet }) => bigAnchors(file, source, sheet))).toEqual([]);
});

test("the anchor rule names what a :has() stands on", () => {
  const caught = (css: string) => bigAnchors(join(ROOT, "x.css"), css, parseCss(css) as unknown as Node).length;
  expect(caught("body:has(.tf-modal) .tf-toasts { color: red }")).toBe(1);
  expect(caught(".tf-board-wrap:not(:has(.tf-lane + .tf-lane)) .tf-col h3 { color: red }")).toBe(1);
  expect(caught(".tf-tbl:has(th.tf-c-epic) .tf-c-labels { color: red }")).toBe(1);
  expect(caught(".tf-ib:not(:has(.tf-acts :hover)):hover .tf-what a { color: red }")).toBe(0);
  expect(caught("tr:has(.tf-rowacts):hover > td { color: red }")).toBe(0);
  expect(caught(".tf-board-wrap:has(.tf-lane) { color: red }")).toBe(0);
});

/**
 * The complex selectors of a list with a sibling combinator (`+`, `~`) beside a compound that names nothing, nested
 * lists included. In a component's `<style>` Svelte adds its own class to every compound it scopes, so only a
 * `:global` selector there is read.
 */
function unnamedSiblings(list: Node, source: string, scoped: boolean): Node[] {
  const found: Node[] = [];
  for (const complex of list.children ?? []) {
    const rels = complex.children ?? [];
    const read = !scoped || source.slice(complex.start, complex.end).includes(":global");
    const sibling = (rel: Node) => {
      const name = (rel as Node & { combinator?: { name: string } | null }).combinator?.name;
      return name === "+" || name === "~";
    };
    if (read && rels.some((rel, i) => i > 0 && sibling(rel) && (!named(rels[i - 1]!.selectors ?? []) || !named(rel.selectors ?? [])))) found.push(complex);
    for (const rel of rels) {
      for (const s of rel.selectors ?? []) if (s.type === "PseudoClassSelector" && PSEUDO_WITH_SELECTORS.has(s.name ?? "") && s.args) found.push(...unnamedSiblings(s.args, source, scoped && s.name !== "global"));
    }
  }
  return found;
}

/** `file:line  selector` for each sibling rule beside a compound that names nothing. */
function siblingOffenders(file: string, source: string, sheet: Node): string[] {
  return selectorLists(sheet).flatMap((list) =>
    unnamedSiblings(list, source, file.endsWith(".svelte")).map((complex) => `${file.slice(ROOT.length + 1)}:${source.slice(0, complex.start).split("\n").length}  ${source.slice(complex.start, complex.end)}`),
  );
}

/**
 * A sibling combinator beside a compound that names no class, id, tag or attribute makes Chromium restyle a whole
 * list on each row it draws (the list-scroll pass, 2026-11-17). A subject that names nothing (`.tf-md > * + *`,
 * `.tf-headed > header + *`, `.tf-fhead.tf-unread ~ *`) is tried against every element on the page, and trying its
 * `+` or `~` marks each element's parent as having children that sibling rules read: the Tickets list's `tbody`
 * among them. A left side that names nothing (`:not(.tf-rowacts) ~ .tf-rowacts`, `* + *`) goes into the one
 * sibling invalidation set Chromium cannot key on anything. Then each row a windowed list inserted into a marked
 * parent applied that set to the parent's whole subtree: 400 to 530 elements restyled a pass on Tickets' list
 * scroll, 13 to 19 ms at CPU 6x, where the rows drawn are about 50. A subject or neighbour with a class, or a
 * positional pseudo-class on a named subject (`.tf-rowacts:not(:first-child)`), says the same at no such cost.
 */
test("no sibling combinator stands beside a compound that names no class, id, tag or attribute", () => {
  expect(sheets().flatMap(({ file, source, sheet }) => siblingOffenders(file, source, sheet))).toEqual([]);
});

test("the sibling rule catches the shapes that restyled the Tickets list, and passes named neighbours", () => {
  const caught = (css: string, file = "x.css") => siblingOffenders(join(ROOT, file), css, parseCss(css) as unknown as Node).length;
  expect(caught(".tf-md > * + * { color: red }")).toBe(1);
  expect(caught(".tf-headed > header + * { color: red }")).toBe(1);
  expect(caught(".tf-fhead.tf-unread.tf-holdsbelow ~ * { color: red }")).toBe(1);
  expect(caught(".tf-rowend > :not(.tf-rowacts) ~ .tf-rowacts { color: red }")).toBe(1);
  expect(caught("@media (width > 1px) { .a :is(.b + *) { color: red } }")).toBe(1);
  expect(caught(":global(.a > * + *) { color: red }", "x.svelte")).toBe(1);
  expect(caught(".a > * + * { color: red }", "x.svelte")).toBe(0);
  expect(caught(".tf-lane + .tf-lane { color: red }")).toBe(0);
  expect(caught(".tf-md li + li { color: red }")).toBe(0);
  expect(caught(".tf-rowend > .tf-rowacts:not(:first-child) { color: red }")).toBe(0);
  expect(caught(".tf-md > :where(:not(:first-child)) { color: red }")).toBe(0);
  expect(caught(":is(.a, .b) + .c { color: red }")).toBe(0);
});

/** Every element whose class list holds `tf-panel`, with the next tag after it: `header` for a headed panel. */
function panels(): { where: string; headed: boolean; opensWithHeader: boolean }[] {
  const out: { where: string; headed: boolean; opensWithHeader: boolean }[] = [];
  for (const file of walk(ROOT, /\.svelte$/)) {
    const source = readFileSync(file, "utf8");
    for (const m of source.matchAll(/<[a-z]+\b[^>]*?\sclass="([^"]*\btf-panel\b[^"]*)"[^>]*>/g)) {
      const after = source.slice((m.index ?? 0) + m[0].length);
      out.push({
        where: `${file.slice(ROOT.length + 1)}:${source.slice(0, m.index).split("\n").length}`,
        headed: /\btf-headed\b/.test(m[1] ?? ""),
        opensWithHeader: /^\s*<header\b/.test(after),
      });
    }
  }
  return out;
}

test("a panel that opens with its heading is marked tf-headed, and only such a panel", () => {
  const all = panels();
  expect(all.length).toBeGreaterThan(20);
  expect(all.filter((p) => p.headed !== p.opensWithHeader).map((p) => p.where)).toEqual([]);
});

/**
 * Scrollers whose background another rule on the same element paints, each with the rule that does. Nothing else
 * may scroll without an opaque background of its own.
 */
const PAINTED_ELSEWHERE: Record<string, string> = {
  ".tf-jobcard": "always drawn with `tf-ticketcard` (the card's yellow) or `tf-ticketcard tf-sheetcard` (`--sheet`)",
  // The asset tree draws the lines from a row to what it reaches behind its columns, and through them: an opaque
  // strip or column hid them (the screenshot comparison of PR #2479). They scroll on the main thread.
  ".cols": "see-through on purpose: the asset tree's reach lines are drawn behind it (`AssetsView.svelte`)",
  ".col": "see-through on purpose: the asset tree's reach lines are drawn behind it (`AssetsView.svelte`)",
};

/** A background value that paints an opaque colour: a theme token, `--under`, or a colour literal with no alpha. */
function opaque(value: string): boolean {
  const last = value.split(/,(?![^(]*\))/).pop()?.trim() ?? "";
  if (/^var\(--(paper|sheet|sheet-2|field|card|card-2|under|(ok|warn|err|info|muted)-bg)\)$/.test(last)) return true;
  if (/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(last)) return true;
  return /^rgb\(\s*[\d.]+[\s,]+[\d.]+[\s,]+[\d.]+\s*\)$/.test(last);
}

type Decl = { type: string; property: string; value: string };

/** `file:line  selector` for each rule that lets a box scroll and paints no opaque background of its own. */
function seeThroughScrollers(file: string, source: string, sheet: Node): string[] {
  const found: string[] = [];
  const visit = (node: Node | null | undefined) => {
    if (!node) return;
    if (node.type === "Rule" && typeof node.prelude === "object") {
      const decls = ((node.block?.children ?? []) as unknown as Decl[]).filter((d) => d.type === "Declaration");
      const scrolls = decls.some((d) => /^overflow(-x|-y)?$/.test(d.property) && /\b(auto|scroll)\b/.test(d.value));
      const painted = decls.some((d) => /^background(-color)?$/.test(d.property) && opaque(d.value));
      const prelude = node.prelude as Node;
      const selector = source.slice(prelude.start, prelude.end).replace(/\s+/g, " ");
      if (scrolls && !painted && !(selector in PAINTED_ELSEWHERE)) found.push(`${file.slice(ROOT.length + 1)}:${source.slice(0, node.start).split("\n").length}  ${selector}`);
    }
    for (const child of node.children ?? []) visit(child);
    if (node.block) visit(node.block);
  };
  visit(sheet);
  return found;
}

/**
 * Chromium at a display scale under 150% keeps LCD (ClearType) text, and so scrolls a box on its compositor thread
 * only when the box's own background is opaque. Every knobas scroller was transparent, and on Windows each scroll
 * frame was a whole main-thread frame: the Tickets list moved 26 pictures a second with the main thread slowed four
 * times, and 239 with the page's own colour behind it. So a rule that lets a box scroll paints the colour the box
 * sits on: the surface's token, or `--under` where the scroller is drawn on more than one surface.
 */
test("every box that scrolls paints an opaque background of its own", () => {
  expect(sheets().flatMap(({ file, source, sheet }) => seeThroughScrollers(file, source, sheet))).toEqual([]);
});

test("the scroller rule reads a background's colour, the last layer of a shorthand included", () => {
  const caught = (css: string) => seeThroughScrollers(join(ROOT, "x.css"), css, parseCss(css) as unknown as Node).length;
  expect(caught(".a { overflow: auto; }")).toBe(1);
  expect(caught(".a { overflow: hidden auto; background: none; }")).toBe(1);
  expect(caught(".a { overflow-x: auto; background: rgba(0, 0, 0, 0.5); }")).toBe(1);
  expect(caught(".a { overflow-y: scroll; background: linear-gradient(red, transparent); }")).toBe(1);
  expect(caught("@media (width > 1px) { .a { overflow: auto; } }")).toBe(1);
  expect(caught(".a { overflow: hidden; }")).toBe(0);
  expect(caught(".a { overflow: auto; background: var(--paper); }")).toBe(0);
  expect(caught(".a { overflow: auto; background: linear-gradient(red, blue) local, var(--sheet); }")).toBe(0);
  expect(caught(".a { overflow-x: auto; background: var(--x); background-color: var(--under); }")).toBe(0);
  expect(caught(".a { overflow: auto; background: #fff; }")).toBe(0);
});
