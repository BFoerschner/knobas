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
/** What *Assign…* on a gap sends: no id, because there is no row yet. */
type Create = [string, string, TimerTarget];

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(rows: DayBlock[], over: { update?: () => Promise<DayBlock> } = {}) {
  const updates: Update[] = [];
  const deletes: number[] = [];
  const creates: Create[] = [];
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
        createBlock: (from: string, to: string, on: TimerTarget) => {
          creates.push([from, to, on]);
          // The backend answers with the row it wrote, and the view re-reads
          // the day — so the strip after an assignment is modelled too, which
          // is where the gap going away is visible.
          const made = block(90 + creates.length, from, to, { target: on });
          listed = [...listed, made];
          return Promise.resolve(made);
        },
      },
    },
  });
  flushSync();
  return { router, updates, deletes, creates, asked };
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

/** The room-bar heading: the day it names, and how much of it is on the strip. */
function heading(): string {
  return (target.querySelector("h1")?.textContent ?? "").replace(/\s+/g, " ").trim();
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

/**
 * The heading's reading is the **blocks'** total, never the day around them.
 * An hour, a forty-minute hole and twenty minutes is 1 h 20 min tracked, where
 * a heading measuring first start to last end would say 2 h — so the gap the
 * strip exists to show is the one thing this number must not absorb.
 *
 * It moves with the strip too: a total computed once at mount would go on
 * claiming time for a block the reader has since deleted.
 */
test("the heading totals the blocks on the strip and not the day around them", async () => {
  const { deletes } = render([block(1, at(9), at(10)), block(2, at(10, 40), at(11))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "gap", "block"]));

  // The whole heading, not a substring: `"1 h 20 min tracked"` *contains*
  // `"20 min tracked"`, so a total that went stale would pass a loose
  // assertion on the second reading below. The mutant that found this one
  // survived until the assertion was tightened.
  expect(heading()).toBe("Thursday 3 September 2026 1 h 20 min tracked");

  // The first *Delete* on the strip belongs to the first block.
  button("Delete")!.click();
  await vi.waitFor(() => expect(deletes).toEqual([1]));
  await vi.waitFor(() => {
    flushSync();
    expect(heading()).toBe("Thursday 3 September 2026 20 min tracked");
  });
});

/**
 * A block that ran through midnight is on **both** days it touched — the
 * backend returns it on each, deliberately — so on the second of them the
 * strip opens with the previous evening's clock. Without the day carried
 * beside it, `23:30 → 01:00` reads as a strip drawn out of order rather than
 * as work that started the night before.
 *
 * This is the wiring of `clockReading`'s second argument; `day.test.ts` owns
 * the arithmetic. Both are needed: a strip that computed the marker perfectly
 * and never passed the day would render nothing at all.
 */
test("a block that ran through midnight says which day its edges fell on", async () => {
  render([block(1, at(23, 30, 2), at(1, 0, 4))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  expect(segmentText(0)).toContain("23:30 (−1) → 01:00 (+1)");
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

/**
 * **A field the reader did not touch does not move the stamp.**
 *
 * The form is minute-granular and the stamps are not, so an *Edit* opened to
 * change only the target and saved would otherwise rebuild both edges from the
 * fields and truncate the seconds off each — a rounding policy arrived at by
 * accident, which is what `CONTEXT.md`'s minute granularity, no rounding
 * forbids. The block below carries seconds on both edges, and they have to
 * come back unchanged.
 */
test("saving without touching the times leaves the stamps exactly as they were", async () => {
  const from = new Date(2026, 8, 3, 9, 0, 37, 0).toISOString();
  const to = new Date(2026, 8, 3, 10, 15, 12, 0).toISOString();
  const { updates } = render([block(1, from, to)]);
  await vi.waitFor(() => expect(button("Edit")).toBeTruthy());

  button("Edit")!.click();
  flushSync();
  // Only the target is changed; both time fields are left as the form filled
  // them, which is the case this rule is about.
  set(target.querySelector<HTMLInputElement>("input[type=text]")!, "another label");
  button("Save")!.click();
  await vi.waitFor(() => expect(updates).toHaveLength(1));

  expect(updates[0]![1]).toBe(from);
  expect(updates[0]![2]).toBe(to);
});

/**
 * `TimerTarget` is exactly-one, on the wire, in the Rust and in the schema. A
 * field that kept `jira:PAY-231` after a switch to *A label* would submit an
 * entity id as the sentence a person is supposed to have written — and the
 * backend would take it, because a label is free text.
 */
test("switching which half the target is clears the other half's text", async () => {
  render([onTicket(1, at(9), at(10), "Retry failed SEPA payouts")]);
  await vi.waitFor(() => expect(button("Edit")).toBeTruthy());

  button("Edit")!.click();
  flushSync();
  const value = () => target.querySelector<HTMLInputElement>("input[type=text]")!.value;
  expect(value()).toBe("jira:PAY-231");

  set(target.querySelector<HTMLSelectElement>("select")!, "label");
  expect(value(), "the entity id would be submitted as the label").toBe("");

  // ...and switching back is the block's own target again, not a field the
  // reader now has to retype.
  set(target.querySelector<HTMLSelectElement>("select")!, "entity");
  expect(value()).toBe("jira:PAY-231");
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

test("the neighbouring days and today are one click away, and each is an address", async () => {
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

// -- passive blocks and *Assign…* (#282) --------------------------------------

/** A passive block: knobas' guess at what was open, on the ticket. */
function passive(id: number, from: string, to: string): DayBlock {
  return {
    ...block(id, from, to, {
      kind: "passive",
      target: { kind: "entity", entity_id: "jira:PAY-231" },
    }),
    title: "Retry failed SEPA payouts",
  };
}

/** Type into the assign form, whichever segment opened it. */
function assignAs(kind: "entity" | "label", value: string) {
  const select = target.querySelector<HTMLSelectElement>('select[id^="assign-kind-"]')!;
  select.value = kind;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  const field = target.querySelector<HTMLInputElement>('input[aria-label="What this time was on"]')!;
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

/**
 * Story 22: the two kinds are **distinguishable on one strip**, and in words
 * as well as in styling — colour alone is not a reading.
 *
 * Both directions, on one strip, because a marker drawn on every block and a
 * marker drawn on none would each pass half of this.
 */
test("a passive block is drawn as one and a manual block beside it is not", async () => {
  render([passive(1, at(9), at(10)), block(2, at(10), at(11))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "block"]));

  const segments = target.querySelectorAll(".strip > .seg");
  expect(segments[0]!.classList.contains("passive")).toBe(true);
  expect(segments[1]!.classList.contains("passive")).toBe(false);
  expect(segmentText(0)).toContain("what was open");
  expect(segmentText(1)).not.toContain("what was open");
});

/**
 * A passive block offers *Assign…* and nothing else.
 *
 * Not editable and not deletable on purpose: the next day read reconciles the
 * day's unassigned passive blocks back to what the heartbeats support, so an
 * edit would be undone under the reader's hands. Assigning is what takes the
 * block out of that reconciliation, by making it manual.
 */
test("a passive block offers *Assign…* and neither *Edit* nor *Delete*", async () => {
  render([passive(1, at(9), at(10))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  expect(button("Assign…")).toBeTruthy();
  expect(button("Edit")).toBeUndefined();
  expect(button("Delete")).toBeUndefined();
});

/**
 * **The first *Assign…* path**: a passive block, assigned, is `update_block`
 * over the block's own span — which is where the backend writes
 * `kind: "manual"`.
 *
 * The times are asserted as the block's own: an assignment that moved either
 * edge would be a different edit wearing the same button.
 */
test("*Assign…* on a passive block sends the block's own span and the chosen target", async () => {
  const { updates, creates } = render([passive(1, at(9), at(10, 30))]);
  await vi.waitFor(() => expect(button("Assign…")).toBeTruthy());

  button("Assign…")!.click();
  flushSync();
  assignAs("label", "  DB config for the migration  ");
  button("Assign")!.click();

  await vi.waitFor(() => expect(updates).toHaveLength(1));
  expect(creates).toHaveLength(0);
  expect(updates[0]).toEqual([
    1,
    at(9),
    at(10, 30),
    { kind: "label", label: "DB config for the migration" },
  ]);
});

/**
 * **The second *Assign…* path**: a gap has no row, so it is `create_block`
 * spanning exactly the unaccounted stretch — and the strip stops drawing a gap
 * there.
 */
test("*Assign…* on a gap writes a block spanning it and the gap goes", async () => {
  const { creates, updates } = render([block(1, at(9), at(10)), block(2, at(11), at(12))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "gap", "block"]));

  button("Assign…")!.click();
  flushSync();
  assignAs("entity", "jira:PAY-231");
  button("Assign")!.click();

  await vi.waitFor(() => expect(creates).toHaveLength(1));
  expect(updates).toHaveLength(0);
  expect(creates[0]).toEqual([at(10), at(11), { kind: "entity", entity_id: "jira:PAY-231" }]);

  await vi.waitFor(() => {
    flushSync();
    expect(strip()).toEqual(["block", "block", "block"]);
  });
});

/**
 * **The heading does not count knobas' own guesses as tracked time.**
 *
 * A passive block says *what was open — not tracked* in the same view, so a
 * total that added it would contradict, in one line, every block it summed.
 * Both numbers are asserted, and they are different numbers on purpose: a
 * heading that simply dropped passive rows and one that counted them into
 * *offered* are told apart only by the second reading.
 */
test("the heading counts manual time as tracked and passive time as offered", async () => {
  render([block(1, at(9), at(10)), passive(2, at(10), at(10, 45))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "block"]));

  expect(heading()).toContain("1 h tracked");
  expect(heading()).toContain("45 min offered");
});

/** With nothing passive on the day, the second reading is absent entirely. */
test("a day with no passive blocks says nothing about offered time", async () => {
  render([block(1, at(9), at(10))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block"]));

  expect(heading()).toContain("1 h tracked");
  expect(heading()).not.toContain("offered");
});

/** One form at a time: opening *Assign…* on a gap closes the one on a block. */
test("only one assign form is open at a time", async () => {
  render([passive(1, at(9), at(10)), block(2, at(11), at(12))]);
  await vi.waitFor(() => expect(strip()).toEqual(["block", "gap", "block"]));

  button("Assign…")!.click();
  flushSync();
  expect(target.querySelectorAll('input[aria-label="What this time was on"]')).toHaveLength(1);

  // The gap's, which is the second *Assign…* on the strip.
  [...target.querySelectorAll<HTMLButtonElement>("button")]
    .filter((candidate) => candidate.textContent?.trim() === "Assign…")[1]!
    .click();
  flushSync();
  expect(target.querySelectorAll('input[aria-label="What this time was on"]')).toHaveLength(1);
});

/**
 * A refused assignment says why, in the backend's own words, and **leaves the
 * form open with what the reader typed still in it** — the rule the edit path
 * follows, and the reason it matters here is that the sentence has to sit
 * beside the field it is about rather than beside a form that has gone.
 */
test("a refused assignment shows the reason the backend gave", async () => {
  render([passive(1, at(9), at(10))], {
    update: () =>
      Promise.reject({
        code: "invalid",
        message: "PAY-999 is not an entity knobas knows",
      }),
  });
  await vi.waitFor(() => expect(button("Assign…")).toBeTruthy());

  button("Assign…")!.click();
  flushSync();
  assignAs("entity", "jira:PAY-999");
  button("Assign")!.click();

  await vi.waitFor(() => {
    flushSync();
    expect(text()).toContain("PAY-999 is not an entity knobas knows");
  });
  const field = target.querySelector<HTMLInputElement>(
    'input[aria-label="What this time was on"]',
  );
  expect(field, "the form closed under the refusal it was meant to explain").not.toBeNull();
  expect(field!.value).toBe("jira:PAY-999");
});
