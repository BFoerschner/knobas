/**
 * A windowed table's columns are set before a row draws (the code-list-columns pass, 2026-11-20; Björn: "when i scroll
 * the code repositories down the width of the columns keeps jumping"). A windowed list draws only the rows near the
 * viewport, so a table that sizes its columns to its rows (`table-layout: auto`, the default) takes new widths from
 * each window of rows as it scrolls. Every table whose body is windowed (`{@attach <win>.list}` on its `tbody`, or a
 * `TfWindowedRows` in it) has `table-layout: fixed` from a rule that reaches it, and a `colgroup` or a `thead` to take
 * its widths from: a fixed table without either takes them from its first row, which is the window's spacer once the
 * list has scrolled (one cell across them all). `testenv/perf/columns.mjs` measures the same in a browser, divs too.
 *
 * Reads every component and stylesheet off disk, with Svelte's own parser.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { parse, parseCss } from "svelte/compiler";
import { expect, test } from "vitest";

/** `app/src/`, off `process.cwd()` (vitest's root is `app/`). */
const ROOT = join(process.cwd(), "src");

function walk(dir: string, pattern: RegExp, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, pattern, found);
    else if (pattern.test(entry.name)) found.push(path);
  }
  return found.sort();
}

// As much of the parsers' trees as this reads.
type Node = {
  type: string;
  name?: string;
  start: number;
  end: number;
  value?: unknown;
  property?: string;
  attributes?: Node[];
  fragment?: { nodes: Node[] };
  expression?: Node;
  children?: Node[];
  selectors?: Node[];
  prelude?: Node | string;
  block?: Node | null;
  consequent?: { nodes: Node[] } | null;
  alternate?: { nodes: Node[] } | null;
  body?: { nodes: Node[] } | null;
  fallback?: { nodes: Node[] } | null;
  then?: { nodes: Node[] } | null;
  catch?: { nodes: Node[] } | null;
  pending?: { nodes: Node[] } | null;
};

/** The classes of an element's static `class` attribute. */
function classesOf(el: Node): string[] {
  const attr = (el.attributes ?? []).find((a) => a.type === "Attribute" && a.name === "class");
  if (!attr || !Array.isArray(attr.value)) return [];
  return (attr.value as Node[]).flatMap((part) => (part.type === "Text" ? String((part as unknown as { data: string }).data).split(/\s+/).filter(Boolean) : []));
}

/** The nodes directly under a node, through every block (`{#if}`, `{#each}`, snippets) but not into elements. */
function childNodes(node: Node): Node[] {
  const out: Node[] = [];
  for (const f of [node.fragment, node.consequent, node.alternate, node.body, node.fallback, node.then, node.catch, node.pending]) if (f) out.push(...f.nodes);
  return out;
}

/** The elements under `node` (through blocks), with each element's own nodes not entered. */
function elementsUnder(node: Node): Node[] {
  const out: Node[] = [];
  const visit = (n: Node) => {
    for (const child of childNodes(n)) {
      if (child.type === "RegularElement" || child.type === "Component") out.push(child);
      else visit(child);
    }
  };
  visit(node);
  return out;
}

type Table = { file: string; line: number; classes: string[]; ancestors: string[][]; colgroup: boolean; thead: boolean };

/** Every table in `source` whose body is windowed, with its classes, its ancestors' and how it sets its widths. */
function windowedTables(file: string, source: string): Table[] {
  const ast = parse(source, { modern: true }) as unknown as { fragment: { nodes: Node[] } };
  const found: Table[] = [];
  const windowed = (body: Node): boolean => {
    if (body.type === "Component" && body.name === "TfWindowedRows") return true;
    if (body.type !== "RegularElement" || body.name !== "tbody") return false;
    return (body.attributes ?? []).some((a) => a.type === "AttachTag" && a.expression && /\.list$/.test(source.slice(a.expression.start, a.expression.end)));
  };
  const visit = (node: Node, ancestors: string[][]) => {
    for (const child of childNodes(node)) {
      if (child.type === "RegularElement" && child.name === "table") {
        const inside = elementsUnder(child);
        if (inside.some(windowed)) {
          found.push({
            file: file.slice(ROOT.length + 1),
            line: source.slice(0, child.start).split("\n").length,
            classes: classesOf(child),
            ancestors,
            colgroup: inside.some((e) => e.name === "colgroup"),
            thead: inside.some((e) => e.name === "thead"),
          });
        }
      }
      visit(child, child.type === "RegularElement" ? [classesOf(child), ...ancestors] : ancestors);
    }
  };
  visit(ast as unknown as Node, []);
  return found;
}

/** The complex selectors (each a list of compounds' classes) of every rule that sets `table-layout: fixed`. */
function fixedSelectors(sheet: Node): string[][][] {
  const out: string[][][] = [];
  const visit = (node: Node | null | undefined) => {
    if (!node) return;
    if (node.type === "Rule" && typeof node.prelude === "object" && node.block) {
      const fixed = (node.block.children ?? []).some((d) => d.type === "Declaration" && d.property === "table-layout" && String(d.value).trim() === "fixed");
      if (fixed) {
        for (const complex of node.prelude.children ?? []) {
          out.push((complex.children ?? []).map((rel) => (rel.selectors ?? []).filter((s) => s.type === "ClassSelector").map((s) => s.name!)));
        }
      }
    }
    for (const child of node.children ?? []) visit(child);
    if (node.block) visit(node.block);
  };
  visit(sheet);
  return out;
}

/** Whether a selector reaches the table: its last compound's classes are all the table's, each earlier one an ancestor's. */
function reaches(selector: string[][], table: Pick<Table, "classes" | "ancestors">): boolean {
  const subject = selector[selector.length - 1] ?? [];
  if (subject.length === 0 || !subject.every((c) => table.classes.includes(c))) return false;
  let at = 0;
  for (const compound of selector.slice(0, -1).reverse()) {
    if (compound.length === 0) continue;
    while (at < table.ancestors.length && !compound.every((c) => table.ancestors[at]!.includes(c))) at++;
    if (at === table.ancestors.length) return false;
    at++;
  }
  return true;
}

function allFixedSelectors(): string[][][] {
  const out: string[][][] = [];
  for (const file of walk(ROOT, /\.css$/)) out.push(...fixedSelectors(parseCss(readFileSync(file, "utf8")) as unknown as Node));
  for (const file of walk(ROOT, /\.svelte$/)) {
    const source = readFileSync(file, "utf8");
    if (!source.includes("table-layout")) continue;
    const css = (parse(source, { modern: true }) as unknown as { css: Node | null }).css;
    if (css) out.push(...fixedSelectors(css));
  }
  return out;
}

/** `file:line .classes  why` for each windowed table whose columns follow its rows. */
function loose(tables: Table[], fixed: string[][][]): string[] {
  return tables.flatMap((t) => {
    const why = [!fixed.some((s) => reaches(s, t)) ? "no table-layout: fixed reaches it" : null, !t.colgroup && !t.thead ? "no colgroup or thead to take its widths from" : null].filter(Boolean);
    return why.length ? [`${t.file}:${t.line} table.${t.classes.join(".")}  ${why.join("; ")}`] : [];
  });
}

function everyWindowedTable(): Table[] {
  return walk(ROOT, /\.svelte$/).flatMap((file) => {
    const source = readFileSync(file, "utf8");
    return source.includes("<table") ? windowedTables(file, source) : [];
  });
}

test("every windowed table sets its columns before a row draws", () => {
  const tables = everyWindowedTable();
  // The Code page's repositories and pull requests, Tickets' list, a pipeline's runs, a repository's branches, pull
  // requests and builds, a pull request's commits: if this finds fewer, the walk has stopped seeing them.
  expect(tables.length).toBeGreaterThanOrEqual(8);
  expect(loose(tables, allFixedSelectors())).toEqual([]);
});

test("the check finds a table that sizes its columns to its rows, and passes one that does not", () => {
  const check = (markup: string, css: string) => loose(windowedTables(join(ROOT, "x.svelte"), markup), fixedSelectors(parseCss(css) as unknown as Node));
  const body = `<tbody {@attach win.list}>{#each rows as r (r)}<tr><td>{r}</td></tr>{/each}</tbody>`;
  expect(check(`<table class="tf-tbl">${body}</table>`, `.tf-x { table-layout: fixed; }`)).toHaveLength(1);
  expect(check(`<table class="tf-tbl tf-x">${body}</table>`, `.tf-x { table-layout: fixed; }`)[0]).toMatch(/no colgroup or thead/);
  expect(check(`<table class="tf-tbl tf-x"><colgroup><col /></colgroup>${body}</table>`, `.tf-x { table-layout: fixed; }`)).toEqual([]);
  expect(check(`<div class="tf-list"><table class="tf-tbl"><thead><tr><th>A</th></tr></thead>${body}</table></div>`, `.tf-list .tf-tbl { table-layout: fixed; }`)).toEqual([]);
  // A rule for a table elsewhere does not reach this one.
  expect(check(`<div class="tf-cbody"><table class="tf-tbl"><colgroup><col /></colgroup>${body}</table></div>`, `.tf-list .tf-tbl { table-layout: fixed; }`)).toHaveLength(1);
  expect(check(`<table class="tf-tbl"><colgroup><col /></colgroup><TfWindowedRows rows={rows} /></table>`, `.tf-tbl { table-layout: auto; }`)).toHaveLength(1);
  // A body that is not windowed draws every row, so its columns never change while it scrolls.
  expect(check(`<table class="tf-tbl"><tbody>{#each rows as r (r)}<tr><td>{r}</td></tr>{/each}</tbody></table>`, ``)).toEqual([]);
});
