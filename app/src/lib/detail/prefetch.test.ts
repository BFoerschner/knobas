/**
 * Hovering or focusing a row reads its detail ahead (#753), so the click that
 * opens it draws at once: the row from the mirror is a few milliseconds, but
 * a Gitea view is a live read of a few hundred.
 *
 * What is pinned is the count of reads, which is the cost: a pointer swept
 * across a tile reads nothing, a dwell reads exactly one entity once, and a
 * second hover over a row already read or being read reads nothing more.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";
import { createIntent, DWELL_MS, prefetchable } from "./prefetch";
import { createViewMemory, readAhead, type ViewMemory } from "./view-read";

let rows: string[];
let views: string[];
let memory: ViewMemory;

const detail = (id: string) =>
  ({ row: { entity_id: id, kind: "pr" }, source: { id: "gitea" } }) as unknown as EntityDetail;

function intent() {
  return createIntent((id) =>
    readAhead(
      id,
      (entityId) => {
        rows.push(entityId);
        return Promise.resolve(detail(entityId));
      },
      (entityId) => {
        views.push(entityId);
        return new Promise<SourceView | null>((resolve) => setTimeout(() => resolve(null), 300));
      },
      memory,
    ),
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  rows = [];
  views = [];
  memory = createViewMemory();
});
afterEach(() => vi.useRealTimers());

test("a dwell on a row reads its row and view once", () => {
  const hover = intent();
  hover.enter("gitea:a/b#1");
  vi.advanceTimersByTime(DWELL_MS - 1);
  expect(rows, "not before the dwell").toEqual([]);
  vi.advanceTimersByTime(1);
  expect(rows).toEqual(["gitea:a/b#1"]);
  expect(views).toEqual(["gitea:a/b#1"]);
});

test("a pointer that passes over rows on its way reads none of them", () => {
  const hover = intent();
  for (const id of ["gitea:a/b#1", "gitea:a/b#2", "gitea:a/b#3"]) {
    hover.enter(id);
    vi.advanceTimersByTime(DWELL_MS / 3);
    hover.leave();
  }
  vi.advanceTimersByTime(DWELL_MS * 10);
  expect(rows).toEqual([]);
  expect(views).toEqual([]);
});

test("hovering a row again while its read is in flight, or after, reads nothing more", async () => {
  const hover = intent();
  hover.enter("gitea:a/b#1");
  vi.advanceTimersByTime(DWELL_MS);
  hover.leave();
  hover.enter("gitea:a/b#1");
  vi.advanceTimersByTime(DWELL_MS);
  expect(rows, "in flight").toEqual(["gitea:a/b#1"]);

  await vi.advanceTimersByTimeAsync(400);
  hover.leave();
  hover.enter("gitea:a/b#1");
  vi.advanceTimersByTime(DWELL_MS);
  expect(rows, "answered and remembered").toEqual(["gitea:a/b#1"]);
  expect(views).toEqual(["gitea:a/b#1"]);
});

test("the panel opened while the hover's read is in flight borrows it", () => {
  const hover = intent();
  hover.enter("gitea:a/b#1");
  vi.advanceTimersByTime(DWELL_MS);
  expect(memory.borrow("gitea:a/b#1"), "the hover's reads are lent").not.toBeNull();
});

test("only what the detail panel draws is read ahead", () => {
  expect(prefetchable("gitea:a/b#1", "pr")).toBe(true);
  expect(prefetchable("jira:PAY-1", "ticket")).toBe(true);
  // A note, an asset and a route open views of their own, through reads of
  // their own; reading a mirror row for them would be a read nobody uses.
  expect(prefetchable("note:01J", "note")).toBe(false);
  expect(prefetchable("asset:db-1", "asset")).toBe(false);
  expect(prefetchable("route:r-1", "route")).toBe(false);
});
