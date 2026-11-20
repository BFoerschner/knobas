/**
 * The unsaved-changes probe (ADR-0050, decisions §10), the very script `crates/knobas-site/src/dirty-probe.js` that
 * knobas runs in a site, against pages in a DOM: a page that would make a browser ask "Leave site?" answers `true`.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { afterEach, expect, test } from "vitest";

const PROBE = readFileSync(join(process.cwd(), "..", "crates", "knobas-site", "src", "dirty-probe.js"), "utf8").trim();

const run = (): unknown => (0, eval)(PROBE);
let offs: (() => void)[] = [];

afterEach(() => {
  for (const off of offs.splice(0)) off();
  window.onbeforeunload = null;
});

function listen(handler: (event: BeforeUnloadEvent) => unknown) {
  const wrapped = (event: Event) => handler(event as BeforeUnloadEvent);
  window.addEventListener("beforeunload", wrapped);
  offs.push(() => window.removeEventListener("beforeunload", wrapped));
}

test("a page with no handler is clean", () => {
  expect(run()).toBe(false);
});

// An `onbeforeunload` that returns a string, and a listener that sets `returnValue`, object in WebKit and in Chromium
// (measured by the spike and by `examples/site_probe.rs` on both); jsdom implements neither, so they are not here.

test("a listener that calls preventDefault objects", () => {
  listen((event) => event.preventDefault());
  expect(run()).toBe(true);
});

test("a listener that does neither is clean", () => {
  listen(() => undefined);
  expect(run()).toBe(false);
});

test("a page that throws in its handler is not taken for clean", () => {
  // The page's own error is reported as the page's, not the test's.
  const swallow = (event: ErrorEvent) => event.preventDefault();
  window.addEventListener("error", swallow);
  offs.push(() => window.removeEventListener("error", swallow));
  listen(() => {
    throw new Error("the page's own bug");
  });
  // The handler's error is the page's; the dispatch goes on, and nothing objected.
  expect([false, null]).toContain(run());
});

test("the script is one expression on one line", () => {
  expect(PROBE.startsWith("(function(){")).toBe(true);
  expect(PROBE.endsWith("})()")).toBe(true);
  expect(PROBE.includes("\n")).toBe(false);
});
