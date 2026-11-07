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
import { toasts } from "../shell/toasts.svelte";
import type { SourceSyncStatus } from "../ipc/sources";

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

test("a press that read the item ends in one toast, in the page's words if it had any, and a failure keeps its line (F-359)", async () => {
  toasts.items = [];
  const detail = detailOf("teamcity:build:1");
  const probe = refreshProbe();
  const refresh = createDetailRefresh(probe.ports);
  await refresh.refresh(detail, () => {});
  await refresh.refresh(detail, () => {});
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Refreshed from Tidewater CI"]);
  await refresh.refresh(detail, () => refresh.answer("Page is up to date, version 14"));
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Page is up to date, version 14"]);
  toasts.items = [];
  await refresh.refresh(detail, () => refresh.answer(null));
  expect(toasts.items).toEqual([]);
});

test("while the source's last sync failed, the toast adds that to what the refresh read (F-362)", async () => {
  toasts.items = [];
  const detail = detailOf("teamcity:build:1");
  const status = (last_outcome: SourceSyncStatus["last_outcome"], source_id = detail.source.id): SourceSyncStatus => ({
    source_id,
    running: false,
    run_id: 1,
    started_at: null,
    last_finished_at: null,
    last_outcome,
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
  });
  const after = async (rows: SourceSyncStatus[] | Error) => {
    const syncStatus = () => (rows instanceof Error ? Promise.reject(rows) : Promise.resolve(rows));
    await createDetailRefresh({ ...refreshProbe().ports, syncStatus }).refresh(detail, () => {});
    return toasts.items.map((toast) => toast.text);
  };
  expect(await after([status("unauthorized")])).toEqual(["Refreshed from Tidewater CI. Its sign-in is still failing."]);
  expect(await after([status("unreachable")])).toEqual(["Refreshed from Tidewater CI. Its last sync couldn't reach it."]);
  expect(await after([status("error")])).toEqual(["Refreshed from Tidewater CI. Its last sync failed."]);
  // Another source's trouble, a source that ended well and a status that can't be read add nothing.
  expect(await after([status("unauthorized", "another")])).toEqual(["Refreshed from Tidewater CI"]);
  expect(await after([status("ok")])).toEqual(["Refreshed from Tidewater CI"]);
  expect(await after(new Error("no status"))).toEqual(["Refreshed from Tidewater CI"]);
  toasts.items = [];
});
