// Code's repository rows hold their height where nothing hovers (the ux28 gate, 2026-11-21). jsdom lays nothing out, so
// the rule is held here as the stylesheet writes it; `code-rows-still-touch` in `testenv/ui-checks/trips.mjs` holds the
// rows themselves in Chrome.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";

const CSS = join(process.cwd(), "src", "lib", "tf", "css");
/** One stylesheet with its comments taken out and its white space folded, so a rule reads as one line. */
const flat = (name: string) =>
  readFileSync(join(CSS, name), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ");

test("where nothing hovers, a repository row's end keeps a small button's height, so the checkout's Clone grows no row", () => {
  // The group stands in the row only where nothing hovers; elsewhere it hangs outside it and holds no room.
  expect(flat("chrome.css")).toContain("@media (hover: hover) { .tf-rowend { position: relative; min-height: 1px; }");
  // *Clone* and *Open in…* are small buttons (`TfOpenIn`, size `sm`), as tall as `--control-h-sm`.
  expect(flat("code.css")).toContain("@media not (hover: hover) { .tf-crepos td.tf-right .tf-rowend { min-height: var(--control-h-sm); } }");
});
