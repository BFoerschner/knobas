// Builds at 6x (2026-11-15): a scroll of a pipeline's runs lays out only what a window's move adds. jsdom lays nothing
// out, so the rules that make it so are held here as the stylesheets write them; `just frame-budget --real` holds
// the scroll itself (`builds-main`, `pipeline-history`).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

const CSS = join(process.cwd(), "src", "lib", "tf", "css");
const builds = readFileSync(join(CSS, "builds.css"), "utf8");
const chrome = readFileSync(join(CSS, "chrome.css"), "utf8");
const tokens = readFileSync(join(CSS, "tokens.css"), "utf8");
/** One stylesheet with its comments taken out and its white space folded, so a rule reads as one line. */
const flat = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");

/** The block `@supports (scrollbar-gutter: stable) { … }`, braces balanced. */
function supportsBlock(css: string): string {
  const at = css.indexOf("@supports (scrollbar-gutter: stable) {");
  expect(at, "the Builds scroller's gutter block").toBeGreaterThanOrEqual(0);
  let depth = 0;
  for (let i = css.indexOf("{", at); i < css.length; i++) {
    if (css[i] === "{") depth++;
    if (css[i] === "}" && --depth === 0) return css.slice(at, i + 1);
  }
  throw new Error("unbalanced @supports block");
}

test("Builds' scroller holds no size container where the engine keeps a gutter, and its body is the host's width", () => {
  const block = supportsBlock(flat(builds));
  // A size container lays out all it holds whenever anything in it changes: 12 ms of a 6x frame on each window move.
  expect(block).toContain(".tf-shost:has(> .tf-bmain) { container-type: normal; }");
  // The bar's 8 px kept whether it shows or not, and taken back by the clip, so a band reaches the edge while nothing
  // scrolls (the spec's Layout): the boxes `100cqw` gave.
  expect(block).toContain(".tf-shost > .tf-bmain { scrollbar-gutter: stable; }");
  expect(block).toContain(".tf-shost > .tf-bmain > .tf-sclip { margin-right: -8px; }");
  expect(block).toContain(".tf-shost > .tf-bmain > .tf-sclip > .tf-sbody { width: 100%; }");
  expect(flat(tokens)).toMatch(/::-webkit-scrollbar \{ width: 8px;/);
});

test("the header names the width at which Open in gives up its words, the host's width less its own padding", () => {
  const block = supportsBlock(flat(builds));
  expect(block).toContain(".tf-bmain .tf-fhead.tf-bhead { container: bhead / inline-size; }");
  const host = Number(/@container bmain \(max-width: (\d+)px\)/.exec(flat(builds))?.[1]);
  const header = Number(/@container bhead \(max-width: (\d+)px\)/.exec(block)?.[1]);
  const padding = /\.tf-fhead \{ padding: \d+px (\d+)px/.exec(flat(chrome));
  expect(host).toBe(1130);
  expect(padding, "the header's side padding").not.toBeNull();
  expect(header).toBe(host - 2 * Number(padding![1]));
});

test("each run row is a fixed table of its own in a block body, its widths the cells' own", () => {
  const css = flat(builds);
  expect(css).toContain(".tf-runs .tf-runtbl { display: block; width: auto; }");
  expect(css).toContain(".tf-runs .tf-runtbl > colgroup { display: none; }");
  expect(css).toContain(".tf-runs .tf-runtbl > tbody { display: block; }");
  expect(css).toContain(".tf-runs .tf-runtbl > tbody > tr { display: table; width: 100%; table-layout: fixed; }");
  expect(css).toContain(".tf-runs .tf-runtbl > tbody > tr > td { box-sizing: border-box; }");
  // Not contained: a row's own layer snaps it to a whole pixel and moves its text.
  expect(css).not.toMatch(/\.tf-runtbl > tbody > tr \{[^}]*contain:/);
});

test("a running run's bar is the duration bar and goes where it goes, so its clock fits the 70 px (#2574)", () => {
  const css = flat(builds);
  for (const width of [1060, 1000]) {
    const rule = new RegExp(`@container \\(max-width: ${width}px\\) \\{[^@]*\\.tf-durbar \\{ display: none; \\}`);
    expect(css).toMatch(rule);
  }
  expect(css).not.toContain("tf-progress");
});
