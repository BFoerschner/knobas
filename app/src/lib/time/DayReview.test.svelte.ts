/**
 * The day review's strip: what a reader sees, and what the two edit buttons
 * send (issue #279).
 *
 * The seam is **a rendered view in, user-visible text and addresses out** —
 * spec #272's testing decisions, and the reason nothing here reaches into the
 * component's state. The bridge is injected, so no Tauri and no database: what
 * the backend does with these calls is `crates/knobas-app/tests/time_ipc.rs`'s,
 * and what is asserted here is that the view asks for the right thing and
 * draws the answer.
 *
 * Every fixture is built with the **local** `Date` constructor. A day is a
 * local thing, these tests run in whatever timezone the machine is set to, and
 * a fixture written as `"…T09:00:00Z"` would render `09:00` only in UTC.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Block, DayBlock, TimerTarget } from "../ipc/time";
import { createRouter } from "../shell/router.svelte";
import DayReview from "./DayReview.svelte";

const DAY = "2026-09-03";
/** The clock the view is given, so *Today* and *Extend to now* are fixed. */
const NOW = new Date(2026, 8, 3, 17, 30, 0, 0);

function at(hour: number, minute = 0, day = 3): string {
  return new Date(2026, 8, day, hour, minute, 0, 0).toISOString();
}

function block(id: number, from: string, to: string, over: Partial<Block> = {}): DayBlock {
  return {
    block: {
      id,
      started_at: from,
      ended_at: to,
      target: { kind: "label", label: "DB config for the migration" },
      kind: "manual",
      ended_by_relaunch: false,
      worklog_id: null,
      ...over,
    },
    title: null,
  };
}

function onTicket(id: number, from: string, to: string, title: string | null): DayBlock {
  return {
    ...block(id, from, to, { target: { kind: "entity", entity_id: "jira:PAY-231" } }),
    title,
  };
}

type Update = [number, string, string, TimerTarget];

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(rows: DayBlock[], over: { update?: () => Promise<DayBlock> } = {}) {
  const updates: Update[] = [];
  const deletes: number[] = [];
  const asked: Array<[string, string]> = [];
  let listed = rows;

  // The address first, then the router: the view is mounted the way the shell
  // mounts it, at an address, so the `day` prop below can be read off the
  // route the way `App.svelte` reads it. Without that the prop is frozen at
  // mount and no test could witness a change of day at all.
  location.hash = `#/time/${DAY}`;
  const router = createRouter();
  app = mount(DayReview, {
    target,
    props: {
      router,
      // A getter, which is how `App.svelte` hands this prop over: the route is
      // rune-backed, so moving a day re-runs the view's read.
      get day() {
        return router.route.view === "time" ? router.route.day : DAY;
      },
      now: () => NOW,
      ports: {
        dayBlocks: (from: string, to: string) => {
          asked.push([from, to]);
          return Promise.resolve(listed);
        },
        updateBlock: (id: number, from: string, to: string, on: TimerTarget) => {
          updates.push([id, from, to, on]);
          // The backend answers with the block as it now stands, and the view
          // re-reads the day. Both are modelled, because the marker going away
          // is a thing the *reader* sees rather than a value in a promise.
          const edited: DayBlock = {
            block: {
              ...listed.find((entry) => entry.block.id === id)!.block,
              started_at: from,
              ended_at: to,
              target: on,
              ended_by_relaunch: false,
            },
            title: null,
          };
          listed = listed.map((entry) => (entry.block.id === id ? edited : entry));
          return over.update ? over.update() : Promise.resolve(edited);
        },
        deleteBlock: (id: number) => {
          deletes.push(id);
          listed = listed.filter((entry) => entry.block.id !== id);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { router, updates, deletes, asked };
}

/** Every segment on the strip, as `"block"` or `"gap"`, in order. */
function strip(): string[] {
  return [...target.querySelectorAll(".strip > .seg")].map((li) =>
    li.classList.contains("gap") ? "gap" : "block",
  );
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function segmentText(index: number): string {
  const segment = target.querySelectorAll(".strip > .seg")[index];
  return (segment?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** A button anywhere in the view, by the words on it. */
function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

test("the day is read as the reader's own midnight-to-midnight", async () => {
  const { asked } = render([]);
  await vi.waitFor(() => expect(asked).toHaveLength(1));

  expect(new Date(asked[0]![0])).toEqual(new Date(2026, 8, 3, 0, 0, 0, 0));
  expect(new Date(asked[0]![1])).toEqual(new Date(2026, 8, 4, 0, 0, 0, 0));
  expect(text()).toContain("Thursday 3 September 2026");
});

test("a manual block draws its target's title, its address and its span", async () => {
  render([onTicket(1, at(9), at(10, 15), "Retry failed SEPA payouts")]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  const line = segmentText(0);
  expect(line).toContain("Retry failed SEPA payouts");
  expect(line).toContain("jira:PAY-231");
  expect(line).toContain("09:00 → 10:15");
  expect(line).toContain("1 h 15 min");
});

/**
 * Migration `0013` keeps no foreign key on `entity_id`, so a block can outlive
 * the entity it was on — that is the whole reason an afternoon survives a
 * purged mirror. What the strip must not then draw is a nameless row.
 */
test("a block whose target the mirror no longer holds is named by its key", async () => {
  render([onTicket(1, at(9), at(10), null)]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));
  expect(segmentText(0)).toContain("PAY-231");
});

test("a label block draws its label", async () => {
  render([block(1, at(9), at(10))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));
  expect(segmentText(0)).toContain("DB config for the migration");
});

test("clicking a target opens its detail through the router", async () => {
  const { router } = render([onTicket(1, at(9), at(10), "Retry failed SEPA payouts")]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  button("Retry failed SEPA payouts")!.click();
  flushSync();

  expect(location.hash).toBe("#/entity/jira:PAY-231");
  expect(router.route).toEqual({
    view: "room",
    ctx: "all",
    detail: { kind: null, entityId: "jira:PAY-231" },
  });
});

/**
 * **A gap is drawn as a gap, and a block of no length is still a block.**
 *
 * The two are opposites and they are the pair this strip is most likely to
 * confuse: a zero-width block is real — a timer started and stopped inside a
 * second, or one the relaunch sweep closed before its first heartbeat — and a
 * gap is time nobody claimed. One fixture holds both, so a strip that emitted
 * an empty gap between the touching blocks, or dropped the zero-length one,
 * fails here rather than in a screenshot.
 */
test("a gap is a gap, and a block of no length is not one", async () => {
  render([
    block(1, at(9), at(10)),
    // Butts straight onto the first, and has no length of its own.
    block(2, at(10), at(10), { target: { kind: "label", label: "a moment" } }),
    block(3, at(10, 40), at(11)),
  ]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "block", "gap", "block"]));

  // The zero-length block is a block: it says what it was on, and it is not
  // labelled as lost time.
  expect(segmentText(1)).toContain("a moment");
  expect(segmentText(1)).not.toContain("unaccounted");

  // ...and the gap is the other thing entirely: no target, and the span it
  // covers said in words.
  expect(segmentText(2)).toContain("40 min unaccounted");
  expect(segmentText(2)).not.toContain("a moment");
});

// -- relaunch-ended blocks ---------------------------------------------------

const stranded = () =>
  block(1, at(9), at(11), { ended_by_relaunch: true, target: { kind: "label", label: "SEPA" } });

test("a relaunch-ended block carries its marker and offers *Extend to now*", async () => {
  render([stranded()]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  expect(segmentText(0)).toContain("ended when knobas closed");
  expect(button("Extend to now")).toBeTruthy();
});

test("an ordinary block carries neither the marker nor *Extend to now*", async () => {
  render([block(1, at(9), at(11))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  expect(text()).not.toContain("ended when knobas closed");
  expect(button("Extend to now")).toBeUndefined();
});

/**
 * **Both halves of *Extend to now*** (story 13; #279's fourth criterion): the
 * end moves to now, and the block stops claiming knobas chose it.
 *
 * The start is asserted too, because "extend" that also moved the start would
 * be a different edit wearing the same button.
 */
test("*Extend to now* moves the end to now and the marker goes", async () => {
  const { updates } = render([stranded()]);
  await vi.waitFor(() => expect(button("Extend to now")).toBeTruthy());

  button("Extend to now")!.click();
  await vi.waitFor(() => expect(updates).toHaveLength(1));
  flushSync();

  const [id, from, to, on] = updates[0]!;
  expect(id).toBe(1);
  expect(from).toBe(at(9));
  expect(new Date(to)).toEqual(NOW);
  expect(on).toEqual({ kind: "label", label: "SEPA" });

  // ...and what the reader is left looking at: a block that runs to now and no
  // longer says knobas closed it, so the button is gone with it.
  await vi.waitFor(() => {
    flushSync();
    expect(segmentText(0)).not.toContain("ended when knobas closed");
  });
  expect(segmentText(0)).toContain("09:00 → 17:30");
  expect(button("Extend to now")).toBeUndefined();
});

// -- editing and deleting ----------------------------------------------------

test("a block's start, end and target can all be moved from the strip", async () => {
  const { updates } = render([block(1, at(9), at(10))]);
  await vi.waitFor(() => expect(button("Edit")).toBeTruthy());

  button("Edit")!.click();
  flushSync();

  const times = [...target.querySelectorAll<HTMLInputElement>("input[type=time]")];
  expect(times.map((field) => field.value)).toEqual(["09:00", "10:00"]);
  set(times[0]!, "09:30");
  set(times[1]!, "11:15");
  set(target.querySelector<HTMLSelectElement>("select")!, "entity");
  set(target.querySelector<HTMLInputElement>("input[type=text]")!, "jira:PAY-231");

  button("Save")!.click();
  await vi.waitFor(() => expect(updates).toHaveLength(1));

  const [id, from, to, on] = updates[0]!;
  expect(id).toBe(1);
  expect(new Date(from)).toEqual(new Date(2026, 8, 3, 9, 30, 0, 0));
  expect(new Date(to)).toEqual(new Date(2026, 8, 3, 11, 15, 0, 0));
  expect(on).toEqual({ kind: "entity", entity_id: "jira:PAY-231" });
});

test("a block can be deleted and the strip stops drawing it", async () => {
  const { deletes } = render([block(1, at(9), at(10)), block(2, at(11), at(12))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "gap", "block"]));

  // The first *Delete* on the strip belongs to the first block.
  [...target.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent?.trim() === "Delete")!
    .click();
  await vi.waitFor(() => expect(deletes).toEqual([1]));
  await vi.waitFor(() => {
    flushSync();
    expect(strip()).toEqual(["block"]);
  });
});

// -- a logged block ----------------------------------------------------------

/**
 * Story 20 on the surface that has to obey it. `worklog_id` is what the strip
 * branches on, and the block is drawn **as read-only** rather than merely
 * without buttons: a row that silently refuses to change is worse than one
 * that says why.
 */
test("a logged block says it is read-only and offers no way to change it", async () => {
  render([block(1, at(9), at(10), { worklog_id: 77 }), block(2, at(11), at(12))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "gap", "block"]));

  expect(segmentText(0)).toContain("logged — read-only");
  expect(target.querySelectorAll(".strip > .seg.locked")).toHaveLength(1);
  // ...and the direction that shows the rule is not simply hiding every
  // button: its unlogged neighbour still has them.
  expect(
    [...target.querySelectorAll<HTMLButtonElement>("button")].filter(
      (candidate) => candidate.textContent?.trim() === "Delete",
    ),
  ).toHaveLength(1);
});

/**
 * The refusal is the backend's sentence, **shown in the view** — the fourth
 * acceptance criterion's "with a reason the view shows". A block can be logged
 * between a read and an edit, so the strip cannot rely on having hidden the
 * buttons.
 */
test("a refused edit shows the reason the backend gave", async () => {
  render([stranded()], {
    update: () =>
      Promise.reject({
        code: "invalid",
        message:
          "this block has been logged to a worklog (#77) and is read-only, so that what knobas shows never disagrees with what the ticket holds.",
      }),
  });
  await vi.waitFor(() => expect(button("Extend to now")).toBeTruthy());

  button("Extend to now")!.click();
  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("logged to a worklog (#77)");
  });
  expect(target.querySelector('[role="alert"]')).toBeTruthy();
});

// -- moving between days -----------------------------------------------------

test("the neighbouring days and today are one click away, and they address", async () => {
  const { router } = render([]);
  await vi.waitFor(() => expect(text()).toContain("Thursday 3 September 2026"));

  target.querySelector<HTMLButtonElement>('[aria-label="The day before"]')!.click();
  flushSync();
  expect(location.hash).toBe("#/time/2026-09-02");
  expect(router.route).toEqual({ view: "time", day: "2026-09-02" });

  // ...and forward again from where the reader now is, not from where the view
  // was mounted: the day is the address's, and the address moved.
  target.querySelector<HTMLButtonElement>('[aria-label="The day after"]')!.click();
  flushSync();
  expect(location.hash).toBe("#/time/2026-09-03");

  // *Today* is offered only while the reader is somewhere else, and it is the
  // clock the view was given -- not `new Date()`.
  target.querySelector<HTMLButtonElement>('[aria-label="The day before"]')!.click();
  flushSync();
  button("Today")!.click();
  flushSync();
  expect(location.hash).toBe("#/time/2026-09-03");
  expect(button("Today"), "*Today* is offered on today, where it does nothing").toBeUndefined();
});

/** Set a form control's value the way a person's typing does. */
function set(field: HTMLInputElement | HTMLSelectElement, value: string) {
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  field.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}
