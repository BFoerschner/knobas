/**
 * The end of a detail page's header (#2287, review of #2238): the detail's own
 * *Refresh* or *Sync now*, then the view's *Sync now*, unless the detail's
 * button already is one. One *Sync now* per header, decided in one place for
 * the pipeline, the run and the Docs reader.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfHeaderRefresh from "./TfHeaderRefresh.svelte";
import { createDetailRefresh } from "./refresh.svelte";
import { detailOf, refreshProbe } from "./refresh.fixture";

/** The role frame's *Sync now*, as `TfKeptPage` hands it to the view. */
const sync = createRawSnippet(() => ({ render: () => `<button type="button" class="tf-btn tf-syncnow">Sync now</button>` }));

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

/** The buttons drawn, each as its label and whether it is the view's (`tf-syncnow`) or the detail's. */
function draw(detail: ReturnType<typeof detailOf> | null): string[] {
  const refresh = createDetailRefresh(refreshProbe().ports);
  view = mount(TfHeaderRefresh, { target, props: { detail, refresh, reread: () => {}, sync } });
  flushSync();
  return [...target.querySelectorAll("button")].map((b) => `${b.textContent} (${b.classList.contains("tf-syncnow") ? "view" : "detail"})`);
}

test("a detail that reads one item draws its Refresh, then the view's Sync now", () => {
  expect(draw(detailOf("teamcity:build:1"))).toEqual(["Refresh (detail)", "Sync now (view)"]);
});

test("a detail whose button is Sync now draws that one, and not the view's", () => {
  expect(draw(detailOf("teamcity:build:1", { refreshes: false } as never))).toEqual(["Sync now (detail)"]);
});

test("with no detail read yet, the view's Sync now alone", () => {
  expect(draw(null)).toEqual(["Sync now (view)"]);
});

test("a turned-off source offers nothing of its own, so the view's Sync now stays", () => {
  const off = detailOf("teamcity:build:1");
  expect(draw({ ...off, source: { ...off.source, enabled: false } } as never)).toEqual(["Sync now (view)"]);
});
