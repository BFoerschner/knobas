/**
 * A room row's state chip (#666).
 *
 * The chip is resolved in SQL (`knobas_core::declared_state!`) and arrives on
 * `EntityRow.state`; what the row owns is drawing it: after the title, with
 * its tone, as text — and nothing at all for the miss.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { EntityRow } from "../ipc/entity";
import EntityLine from "./EntityLine.svelte";

function row(over: Partial<EntityRow> = {}): EntityRow {
  return {
    entity_id: "gitea:tidewater/payout-service#7",
    kind: "pr",
    source_id: "gitea",
    title: "Retry failed SEPA payouts",
    updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
    ...over,
  };
}

function render(entity: EntityRow, showSource = true) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(EntityLine, {
    target,
    props: { row: entity, onopen: vi.fn(), showSource },
  });
  flushSync();
  return {
    target,
    chip: () => target.querySelector<HTMLElement>(".st"),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a row with a state draws its chip after the title, in its tone", () => {
  const screen = render(row({ state: { text: "FAILURE", tone: "err" } }));
  const chip = screen.chip();
  expect(chip?.textContent).toBe("FAILURE");
  expect(chip?.dataset.tone).toBe("err");
  // After the title, inside the title cell: the chip qualifies the item.
  const cell = screen.target.querySelector(".t");
  expect(cell?.textContent).toMatch(/Retry failed SEPA payouts\s*FAILURE/);
  screen.done();
});

test("a row with no state draws no chip", () => {
  const screen = render(row());
  expect(screen.chip()).toBeNull();
  screen.done();
});

test("a state is drawn as text", () => {
  const screen = render(row({ state: { text: "<b>merged</b>", tone: "ok" } }));
  expect(screen.chip()?.textContent).toBe("<b>merged</b>");
  expect(screen.target.querySelector("b")).toBeNull();
  screen.done();
});

test("the source column is drawn unless the room is one source's", () => {
  const all = render(row());
  expect(all.target.querySelector(".src")?.textContent).toBe("gitea");
  all.done();
  const scoped = render(row(), false);
  expect(scoped.target.querySelector(".src")).toBeNull();
  scoped.done();
});
