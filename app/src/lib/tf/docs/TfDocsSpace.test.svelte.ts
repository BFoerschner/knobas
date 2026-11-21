/**
 * A space's own page (`#/docs/space/<key>`): every page in reading order, and at a real space's size only the rows
 * near the screen drawn (2026-11-10, Björn's 5,000 pages).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DocSpace } from "../../ipc/entity";
import TfDocsSpace from "./TfDocsSpace.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

function space(n: number): DocSpace {
  return {
    source_id: "confluence",
    source_name: "confluence",
    key: "HB",
    name: "Engineering handbook",
    pages: Array.from({ length: n }, (_, i) => ({ entity_id: `confluence:${i}`, title: `Page ${i}`, parent: i < 10 ? null : `confluence:${i % 10}` })),
  };
}

const rows = () => [...target.querySelectorAll<HTMLAnchorElement>(".tf-dh-row")];

test("a short space draws every page, its parents first, each under the one that holds it", () => {
  page = mount(TfDocsSpace, { target, props: { space: space(30) } });
  flushSync();
  expect(rows()).toHaveLength(30);
  expect(rows().slice(0, 3).map((row) => row.textContent)).toEqual(["Page 0", "Page 10", "Page 20"]);
  expect(rows()[0]!.classList.contains("tf-dh-later")).toBe(false);
  expect(rows()[1]!.classList.contains("tf-dh-later")).toBe(true);
});

test("a space of thousands draws the rows near the screen, and says how many it holds", () => {
  page = mount(TfDocsSpace, { target, props: { space: space(2_250) } });
  flushSync();
  expect(rows().length).toBeGreaterThan(0);
  expect(rows().length).toBeLessThan(200);
  expect(target.querySelector(".tf-sub")?.textContent).toContain("2250");
});
