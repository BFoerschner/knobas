/**
 * The schema explorer's parts the XML view draws (the declaration beside a document, the picker, a finding's words) bring
 * their own stylesheet: drawn from an XML file before any schema file was opened, they had none (2026-11-24, long paths at
 * 1100 x 600: the picker's rows unstyled and wider than its list).
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import TfXsdPoint from "./TfXsdPoint.svelte";

const source = (file: string) => readFileSync(join(process.cwd(), "src/lib/tf/render", file), "utf8");

test("every schema part the XML view imports loads xsd.css itself", () => {
  const parts = [...source("doc/TfDocView.svelte").matchAll(/import \w+ from "\.\.\/xsd\/(\w+)\.svelte";/g)].map((m) => m[1]!);
  expect(parts).toEqual(expect.arrayContaining(["TfXsdPage", "TfXsdRuns", "TfXsdPoint"]));
  for (const part of parts) expect(source(`xsd/${part}.svelte`), part).toMatch(/import "\.\.\/\.\.\/css\/xsd\.css";/);
});

test("the picker's sentence keeps its space after the namespace", () => {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const view = mount(TfXsdPoint, { target, props: { namespace: "urn:a", location: "../a.xsd", candidates: [], onpick: () => {}, onclose: () => {} } });
  flushSync();
  expect(document.querySelector(".tf-xsd-pointwhy")?.textContent).toContain("urn:a. It is used");
  unmount(view);
  target.remove();
});
