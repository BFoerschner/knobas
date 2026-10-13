/**
 * A room row reads its detail ahead when the pointer rests on it or focus
 * lands on it (#753): the wire from the row to `readAhead`. The dwell and the
 * once-only rule are `detail/prefetch.test.ts`'s; this pins that a row is
 * wired, and that a note's row is not.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityRow } from "../ipc/entity";

const reads: string[] = [];
vi.mock("../ipc/entity", () => ({
  getEntity: (id: string) => {
    reads.push(id);
    return Promise.resolve({ row: { entity_id: id, kind: "pr" }, source: { id: "gitea" } });
  },
  entityView: () => Promise.resolve(null),
}));

const { default: EntityLine } = await import("./EntityLine.svelte");
const { DWELL_MS } = await import("../detail/prefetch");
const { viewMemory } = await import("../detail/view-read");

function render(entity: Partial<EntityRow>) {
  const target = document.createElement("div");
  document.body.append(target);
  const row: EntityRow = {
    entity_id: "gitea:tidewater/payout-service#7",
    kind: "pr",
    source_id: "gitea",
    title: "Retry failed SEPA payouts",
    updated_at: null,
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
    ...entity,
  };
  const app = mount(EntityLine, { target, props: { row, onopen: vi.fn() } });
  flushSync();
  const button = target.querySelector<HTMLButtonElement>("button.row")!;
  return {
    button,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  reads.length = 0;
  viewMemory.clear();
});
afterEach(() => vi.useRealTimers());

test("resting the pointer on a row reads its detail ahead, once", () => {
  const screen = render({});
  screen.button.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "mouse" }));
  vi.advanceTimersByTime(DWELL_MS);
  expect(reads).toEqual(["gitea:tidewater/payout-service#7"]);
  screen.done();
});

test("focus on a row reads it ahead too; leaving before the dwell reads nothing", () => {
  const screen = render({});
  screen.button.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "mouse" }));
  screen.button.dispatchEvent(new PointerEvent("pointerleave", { pointerType: "mouse" }));
  vi.advanceTimersByTime(DWELL_MS * 2);
  expect(reads).toEqual([]);
  screen.button.focus();
  vi.advanceTimersByTime(DWELL_MS);
  expect(reads).toEqual(["gitea:tidewater/payout-service#7"]);
  screen.done();
});

test("a note's row is not read ahead: it opens through a read of its own", () => {
  const screen = render({ entity_id: "note:01J9", kind: "note", source_id: "note" });
  screen.button.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "mouse" }));
  vi.advanceTimersByTime(DWELL_MS * 2);
  expect(reads).toEqual([]);
  screen.done();
});

test("a finger landing on a row reads nothing ahead: a touch that scrolls lands on rows too", () => {
  const screen = render({});
  screen.button.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "touch" }));
  vi.advanceTimersByTime(DWELL_MS * 2);
  expect(reads).toEqual([]);
  screen.done();
});
