/**
 * Diagnostics — spec §3, Rec 08-24: *"per-source sync log with errors,
 * last-run durations, item counts, FTS index state, re-index button, DB size.
 * The status bar shows the summary; this is where you look when a sync
 * misbehaves."*
 *
 * So the failures worth pinning are the ones that would make a reader stop
 * looking: a run in flight rendered as an instant success, a run log in the
 * wrong order, and an error the log quietly drops.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DbStats, SyncRunRow } from "../ipc/sources";

const NOW = new Date("2026-08-25T12:00:00Z");

const calls = { listSyncRuns: [] as { sourceId: string | null; limit: number }[], reindex: 0 };
let runs: SyncRunRow[] = [];
let stats: DbStats = {
  db_bytes: 222_298_112,
  entity_count: 128,
  item_count: 213,
  per_source: [],
  oldest_synced_at: "2026-08-24T08:00:00Z",
  newest_synced_at: "2026-08-25T11:56:00Z",
};
let statsFail: unknown = null;

vi.mock("../ipc/sources", () => ({
  listSyncRuns: (sourceId: string | null, limit: number) => {
    calls.listSyncRuns.push({ sourceId, limit });
    return Promise.resolve(runs);
  },
  dbStats: () => (statsFail ? Promise.reject(statsFail) : Promise.resolve(stats)),
  reindexFts: () => {
    calls.reindex += 1;
    return Promise.resolve();
  },
}));

const { default: Diagnostics } = await import("./Diagnostics.svelte");

function run(over: Partial<SyncRunRow> = {}): SyncRunRow {
  return {
    id: 1,
    source_id: "jira",
    trigger: "schedule",
    started_at: "2026-08-25T11:56:00Z",
    finished_at: "2026-08-25T11:56:04Z",
    outcome: "ok",
    upserted: 14,
    deleted: 1,
    swept: 2,
    error: null,
    cursor_after: null,
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(props: Record<string, unknown> = {}) {
  app = mount(Diagnostics, { target, props: { now: NOW, ...props } });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

function rows() {
  return [...target.querySelectorAll<HTMLElement>(".log-run")];
}

function button(label: string, within: ParentNode = target) {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
}

beforeEach(() => {
  calls.listSyncRuns = [];
  calls.reindex = 0;
  runs = [];
  statsFail = null;
  stats = {
    db_bytes: 222_298_112,
    entity_count: 128,
    item_count: 213,
    per_source: [],
    oldest_synced_at: "2026-08-24T08:00:00Z",
    newest_synced_at: "2026-08-25T11:56:00Z",
  };
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the dbbar reads the database the way the status bar does", async () => {
  render();
  await settle();
  const bar = target.querySelector(".dbbar")!;
  expect(bar.textContent).toContain("212 MB");
  expect(bar.textContent).toContain("128");
  expect(bar.textContent).toContain("213");
  // Oldest and newest: the pair is what says whether a sync has *stopped*, as
  // opposed to never having started.
  expect(bar.textContent).toContain("11:56");
});

test("lists runs newest first with duration, counts and the error when there is one", async () => {
  runs = [
    run({ id: 3, started_at: "2026-08-25T11:56:00Z", finished_at: "2026-08-25T11:56:04Z" }),
    run({
      id: 2,
      source_id: "gitea",
      trigger: "manual",
      started_at: "2026-08-25T11:40:00Z",
      finished_at: "2026-08-25T11:40:01Z",
      outcome: "unreachable",
      upserted: 0,
      deleted: 0,
      swept: 0,
      error: "connection refused",
    }),
  ];
  render();
  await settle();

  expect(calls.listSyncRuns[0]).toEqual({ sourceId: null, limit: 50 });
  expect(rows().length).toBe(2);
  // The backend answers newest-first; the view must not re-sort it into
  // something else.
  expect(rows()[0]!.textContent).toContain("4.0 s");
  expect(rows()[0]!.textContent).toContain("14");
  expect(rows()[0]!.textContent).toContain("schedule");
  expect(rows()[1]!.textContent).toContain("connection refused");
  expect(rows()[1]!.textContent).toContain("manual");
});

test("a run still in flight shows as running, not as a 0 ms success", async () => {
  runs = [run({ id: 4, finished_at: null, outcome: null, upserted: 0 })];
  render();
  await settle();
  const row = rows()[0]!;
  expect(row.textContent).toContain("running");
  expect(row.textContent).not.toContain("0 ms");
  expect(row.textContent).toContain("—");
});

test("a run's error is text, never markup", async () => {
  runs = [run({ outcome: "error", error: '<b>500</b> from upstream' })];
  render();
  await settle();
  expect(rows()[0]!.textContent).toContain("<b>500</b> from upstream");
  expect(rows()[0]!.querySelector("b")).toBeNull();
});

test("no runs yet says so instead of showing an empty table", async () => {
  runs = [];
  render();
  await settle();
  expect(target.textContent).toContain("No sync runs");
  expect(rows().length).toBe(0);
});

test("Re-index asks first, then calls reindexFts once", async () => {
  render();
  await settle();
  button("Re-index full text")!.click();
  flushSync();
  expect(calls.reindex).toBe(0);

  const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
  // Concurrent, so searches keep working — which is the fact that decides
  // whether a person clicks this in the middle of a working day.
  expect(dialog.textContent).toMatch(/search/i);
  button("Re-index", dialog)!.click();
  await settle();
  expect(calls.reindex).toBe(1);
});

test("cancelling the re-index confirm does nothing", async () => {
  render();
  await settle();
  button("Re-index full text")!.click();
  flushSync();
  button("Cancel", target.querySelector<HTMLElement>('[role="dialog"]')!)!.click();
  await settle();
  expect(calls.reindex).toBe(0);
});

test("dbStats failing leaves the run log readable", async () => {
  statsFail = { code: "not_ready", message: "database is starting", source_id: null };
  runs = [run()];
  render();
  await settle();
  // The two reads are independent, and a diagnostics view that blanked because
  // one of them failed would be least useful exactly when it is most needed.
  expect(rows().length).toBe(1);
  expect(target.querySelector(".dbbar")!.textContent).toContain("—");
});

test("scoping to one source asks for that source's runs only", async () => {
  runs = [];
  render({ sourceId: "gitea" });
  await settle();
  expect(calls.listSyncRuns[0]).toEqual({ sourceId: "gitea", limit: 50 });
});

test("refresh re-reads both the stats and the log", async () => {
  render();
  await settle();
  const before = calls.listSyncRuns.length;
  button("Refresh")!.click();
  await settle();
  expect(calls.listSyncRuns.length).toBe(before + 1);
});
