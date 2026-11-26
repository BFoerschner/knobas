/**
 * XML and JSON in a comparison (C1): the counts' line, a row a part at its path, opened to both values, `n` stepping
 * through, a re-indented document the same, and a side that is not well formed handing the change to Lines.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfDocDiff from "./TfDocDiff.svelte";
import { localDocEngine } from "../../../render/doc-port";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let said = "";
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  said = "";
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(before: string | null, after: string | null, kind: "xml" | "json") {
  view = mount(TfDocDiff, { target, props: { before, after, kind, engine: localDocEngine(), onsay: (w: string) => (said = w) } });
  flushSync();
}
async function until(what: () => boolean, rounds = 200) {
  for (let i = 0; i < rounds && !what(); i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 5));
    flushSync();
  }
  flushSync();
}
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rows = () => [...target.querySelectorAll<HTMLElement>(".tf-xsd-crow")];

const POM = `<project><groupId>t</groupId><version>2.4.1</version><dependencies><dependency><artifactId>pg</artifactId><version>42.7.4</version></dependency></dependencies></project>`;

test("a re-indented POM is the same document; a changed one lists its parts by path, opened to both values", async () => {
  draw(POM, POM.replace(/></g, ">\n    <"), "xml");
  await until(() => text(target.querySelector(".tf-xsd-csum")) !== "");
  expect(text(target.querySelector(".tf-xsd-csum"))).toBe("The same document on both sides. 7 elements read the same, however they are written.");
  expect(said).toBe("The same document on both sides.");
  unmount(view!);
  draw(POM, POM.replace("42.7.4", "42.7.5").replace("<groupId>t</groupId>", "<groupId>t</groupId><name>Order import</name>"), "xml");
  await until(() => rows().length > 0);
  expect(said).toBe("1 element changed, 1 added, 0 removed, 0 moved");
  expect(rows().map((r) => `${r.dataset["mark"]} ${text(r.querySelector(".tf-xsd-cname"))} | ${text(r.querySelector(".tf-xsd-csay"))}`)).toEqual([
    "added /project/name | Order import",
    "changed /project/dependencies/dependency/version | text: 42.7.4 → 42.7.5",
  ]);
  rows()[1]!.querySelector("button")!.click();
  flushSync();
  expect(text(rows()[1]!.querySelector(".tf-xsd-cfacts"))).toBe("text 42.7.4 → 42.7.5 Line 1 before, line 1 after.");
});

test("JSON: rows at their jq paths; n steps through and opens each; a side that is not JSON hands it to Lines", async () => {
  draw(`{"a":1,"list":[1,2]}`, `{"a":2,"list":[1,2,3]}`, "json");
  await until(() => rows().length > 0);
  expect(rows().map((r) => text(r.querySelector(".tf-xsd-cname")))).toEqual([".a", ".list[2]"]);
  const stage = view as unknown as { nextChange: (step: 1 | -1, entering?: boolean) => boolean };
  expect(stage.nextChange(1, true)).toBe(true);
  expect(stage.nextChange(1)).toBe(true);
  flushSync();
  expect(rows()[1]!.classList.contains("tf-xsd-cur")).toBe(true);
  expect(rows()[1]!.querySelector(".tf-xsd-cfacts")).not.toBeNull();
  expect(stage.nextChange(1)).toBe(false);
  unmount(view!);
  draw(`{"a":`, `{"a":1}`, "json");
  await until(() => text(target.querySelector(".tf-xsd-csum")) !== "");
  expect(text(target.querySelector(".tf-xsd-csum"))).toMatch(/^The side before is not JSON: .* Lines has the change\.$/);
});
