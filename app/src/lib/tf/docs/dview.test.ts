/**
 * A view drawn in a Docs page's body (D1's checks, 2026-11-25): the schema's box, the XML and JSON view's box and *API*'s
 * inline form are `.tf-dview`, and the body's rules for its own elements stop at them, so a declaration's heading is not
 * a page heading nor the lines' `pre` a code block.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

const LIB = join(process.cwd(), "src/lib");
const BOUNDARY = ":where(:not(.tf-dview *))";
/** The elements a page body styles that a view also draws. */
const ELEMENTS = /^\.tf-doc (?::is\()?(h2|h3|p|ul|ol|li|code|pre|table|th|td|:where\(strong, b\))\b/;

test("every rule of docs.css for the body's own elements stops at a view drawn in it", () => {
  const css = readFileSync(join(LIB, "tf/css/docs.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const selectors = [...css.matchAll(/(^|\})\s*([^{}@]+)\{/g)].flatMap((m) => m[2]!.split(/,(?![^()]*\))/).map((s) => s.trim()));
  // A page heading's tools and a task's paragraph are the page's own classes, which no view draws.
  const element = selectors.filter((s) => ELEMENTS.test(s) && !/\.tf-(tools|task)\b/.test(s) && !/^\.tf-doc (h2|li|pre) [.>a-z]/.test(s) && !/^\.tf-doc (li|pre) >/.test(s));
  expect(element.length, "no element rule found at all").toBeGreaterThan(8);
  expect(element.filter((s) => !s.includes(BOUNDARY))).toEqual([]);
  expect(css).toMatch(/\.tf-doc \.tf-dview \{[^}]*font: var\(--wt-ui, 400\) calc\(var\(--t-sm\) \* var\(--fs\)\)\/1\.45 var\(--sans\);/);
});

test("the views drawn in a page are .tf-dview", () => {
  const read = (path: string) => readFileSync(join(LIB, path), "utf8");
  expect(read("tf/render/xsd/TfXsdInline.svelte")).toContain('class="tf-xsd-inline tf-dview"');
  expect(read("tf/docs/TfDocBlocks.svelte")).toContain('class="tf-doc-xv tf-dview"');
  expect(read("tf/render/TfOasInline.svelte")).toContain('class="tf-oas-inline tf-dview"');
});
