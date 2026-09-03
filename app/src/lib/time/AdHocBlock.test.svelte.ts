/**
 * *Log an ad-hoc block*: the two ways out, and the reason on screen (#281).
 *
 * The load-bearing claims are about **writes**. *Log to a ticket…* has to move
 * the block before it asks for a draft — the draft is built from the day's
 * unlogged blocks on that ticket, so the wrong order opens a draft this
 * afternoon is missing from — and *Keep local* has to write **nothing at all**,
 * which is the one thing an "it closed" assertion cannot see.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type {
  AdHocBlock as AdHocOffer,
  Block,
  DayBlock,
  Draft,
  ReaderDay,
  SuggestionRule,
  TimerTarget,
} from "../ipc/time";
import AdHocBlock from "./AdHocBlock.svelte";

const PAGE: TimerTarget = { kind: "entity", entity_id: "confluence:ENG:SEPA payout retry design" };
const TICKET = "jira:PAY-231";

/** 09:00–10:30 on a page: an hour and a half with nowhere to go. */
const BLOCK: Block = {
  id: 7,
  started_at: "2026-09-03T09:00:00Z",
  ended_at: "2026-09-03T10:30:00Z",
  target: PAGE,
  kind: "manual",
  ended_by_relaunch: false,
  worklog_id: null,
};

function offering(rule: SuggestionRule = "linked_to_target"): AdHocOffer {
  return {
    block_id: BLOCK.id,
    suggestion: { entity_id: TICKET, title: "Retry failed SEPA payouts", rule },
  };
}

const DRAFT: Draft = {
  entity_id: TICKET,
  day: "2026-09-03",
  started_at: BLOCK.started_at,
  ended_at: BLOCK.ended_at,
  seconds: 90 * 60,
  block_ids: [BLOCK.id],
  candidates: [],
  comment: "",
};

/** What the two bridge calls were made with, in the order they were made. */
type Moved = { id: number; startedAt: string; endedAt: string; target: TimerTarget };
type Asked = { entityId: string; when: ReaderDay };

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(
  offer: AdHocOffer = offering(),
  draft: Draft | null = DRAFT,
  move: () => Promise<DayBlock> = () =>
    Promise.resolve({ block: { ...BLOCK, target: { kind: "entity", entity_id: TICKET } }, title: null }),
) {
  const moved: Moved[] = [];
  const asked: Asked[] = [];
  const order: string[] = [];
  const onclose = vi.fn();
  const onlog = vi.fn();
  app = mount(AdHocBlock, {
    target,
    props: {
      block: BLOCK,
      offer,
      onclose,
      onlog,
      updateBlock: ((id: number, startedAt: string, endedAt: string, to: TimerTarget) => {
        order.push("move");
        moved.push({ id, startedAt, endedAt, target: to });
        return move();
      }) as never,
      worklogDraft: ((entityId: string, when: ReaderDay) => {
        order.push("draft");
        asked.push({ entityId, when });
        return Promise.resolve(draft);
      }) as never,
    },
  });
  flushSync();
  return { moved, asked, order, onclose, onlog };
}

function button(starts: string): HTMLButtonElement {
  const found = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
    (b.textContent ?? "").trim().startsWith(starts),
  );
  expect(found, `there is no ${starts} button`).toBeDefined();
  return found!;
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

/**
 * The suggestion, and **the rule that produced it**, in words.
 *
 * Every rule is exercised, because the failure this guards against is one arm
 * of the `switch` falling through to nothing: a suggestion drawn with no
 * reason under it looks like a suggestion, and the reader has no way to weigh
 * it.
 */
test("it names the suggested ticket and says which rule produced it", () => {
  for (const [rule, reason] of [
    ["linked_to_target", "You linked it to ENG:SEPA payout retry design."],
    ["context_anchor", "It anchors the room this block ran in."],
    ["last_logged", "It is the last ticket you logged time to on this day."],
  ] as [SuggestionRule, string][]) {
    render(offering(rule));
    expect(target.textContent).toContain("PAY-231");
    expect(target.textContent).toContain("Retry failed SEPA payouts");
    expect(target.textContent, `the ${rule} rule has no reason on screen`).toContain(reason);
    if (app) unmount(app);
    app = undefined;
    target.innerHTML = "";
  }
});

/**
 * *Log to a ticket…* moves the block **first**, then asks for the draft.
 *
 * The order is the assertion, off one log rather than two counters: both calls
 * happen either way round, and the wrong way round opens a draft that does not
 * contain the block the dialog is about.
 */
test("Log to a ticket… re-targets the block and then opens the draft", async () => {
  const { moved, asked, order, onclose, onlog } = render();

  button("Log to").click();
  await vi.waitFor(() => expect(onlog).toHaveBeenCalled());
  flushSync();

  expect(order).toEqual(["move", "draft"]);
  expect(moved).toEqual([
    {
      id: BLOCK.id,
      // The block's own times, unchanged: the reader is saying what the
      // stretch was about, not when it happened.
      startedAt: BLOCK.started_at,
      endedAt: BLOCK.ended_at,
      target: { kind: "entity", entity_id: TICKET },
    },
  ]);
  expect(asked[0]?.entityId).toBe(TICKET);
  // The day the block started on, not whatever day the dialog is open on.
  expect(asked[0]?.when.day).toBe("2026-09-03");
  expect(onlog).toHaveBeenCalledWith(DRAFT);
  expect(onclose).toHaveBeenCalled();
});

/**
 * *Keep local* writes **nothing**. Not a re-target, not a draft, not a
 * worklog — the block stays exactly as the timer left it.
 */
test("Keep local closes the dialog and writes nothing at all", () => {
  const { moved, asked, onclose, onlog } = render();

  button("Keep local").click();
  flushSync();

  expect(onclose).toHaveBeenCalled();
  expect(moved, "Keep local moved the block").toEqual([]);
  expect(asked, "Keep local opened a draft").toEqual([]);
  expect(onlog).not.toHaveBeenCalled();
});

/**
 * With no rule firing there is nothing to log to, so the only way out is
 * *Keep local* — and it is the primary action rather than a ghost.
 */
test("with no suggestion there is only Keep local, and it is the default", () => {
  render({ block_id: BLOCK.id, suggestion: null });

  // Every button with words on it. The dialog's own dismiss control is an
  // icon and carries none, so it is not one of the ways *out of this
  // decision* -- what this asserts is that no second answer is offered.
  expect(
    [...target.querySelectorAll("button")]
      .map((b) => (b.textContent ?? "").trim())
      .filter((label) => label !== ""),
  ).toEqual(["Keep local"]);
  expect(button("Keep local").className).toContain("pri");
  expect(target.textContent).toContain("nothing to suggest");
});

/**
 * A refused move is said, never swallowed: the block is still on the page, and
 * a reader who is not told closes this believing the time is on the ticket.
 */
test("a refused re-target keeps the dialog open and says why", async () => {
  const { onclose, onlog } = render(offering(), DRAFT, () =>
    Promise.reject(new Error("this block has been logged to a worklog and is read-only")),
  );

  button("Log to").click();
  await vi.waitFor(() => expect(target.textContent).toContain("read-only"));

  expect(onclose).not.toHaveBeenCalled();
  expect(onlog).not.toHaveBeenCalled();
});
