/**
 * The Time view's Day tab, as arithmetic (#827): which row each block is, the
 * five figures, and where the strip puts each row.
 *
 * Instants are built with the local `Date` constructor for `day.test.ts`'s
 * reason: a day is a local thing and these run in whatever zone the suite is
 * pinned to.
 */
import { expect, test } from "vitest";

import type { AdHocBlock, Block, DayBlock, RunningTimer } from "../../ipc/time";
import { dayFigures, dayRows, hm, offerRuns, place, stripWindow, type DayInput, type DayRow } from "./day-view";

const DAY = "2026-08-22";

function at(hour: number, minute = 0, day = 22): string {
  return new Date(2026, 7, day, hour, minute, 0, 0).toISOString();
}

function block(id: number, from: string, to: string, over: Partial<Block> = {}, title: string | null = null): DayBlock {
  return {
    block: {
      id,
      started_at: from,
      ended_at: to,
      target: { kind: "entity", entity_id: "jira:PAY-231" },
      kind: "manual",
      ended_by_relaunch: false,
      worklog_id: null,
      ...over,
    },
    title,
  };
}

const onTicket = null;
function offer(entity_id: string, rule: "linked_to_target" | "last_logged" = "linked_to_target"): AdHocBlock {
  return { suggestion: { entity_id, title: null, rule } };
}

function states(rows: DayRow[]) {
  return rows.map((row) => [row.state, row.from, row.to]);
}

test("each block is one row with its state, each open stretch is a row, and a gap that is neither is no row", () => {
  const blocks = [
    block(1, at(8, 50), at(9, 25), { worklog_id: 7, target: { kind: "entity", entity_id: "jira:PAY-228" } }),
    block(2, at(9, 25), at(9, 40), { kind: "passive", target: { kind: "entity", entity_id: "gitea:payments#139" } }),
    block(3, at(9, 40), at(11, 50)),
    block(5, at(13, 10), at(13, 58), { target: { kind: "label", label: "Standup and planning" } }),
  ];
  const rows = dayRows({
    day: DAY,
    blocks,
    offers: { 2: offer("jira:PAY-228"), 3: onTicket, 5: offer("jira:PAY-200", "last_logged") },
    open: [{ started_at: at(12, 35), ended_at: at(13, 10) }],
    running: null,
    now: new Date(2026, 7, 22, 15, 0),
  });
  // 11:50 to 12:35 is neither a block nor focused time: lunch, as far as
  // knobas knows, and not a row.
  expect(states(rows)).toEqual([
    ["logged", at(8, 50), at(9, 25)],
    ["offered", at(9, 25), at(9, 40)],
    ["unlogged", at(9, 40), at(11, 50)],
    ["open", at(12, 35), at(13, 10)],
    ["label", at(13, 10), at(13, 58)],
  ]);
  expect(rows[3]!.target).toBeNull();
  expect(rows[3]!.block).toBeNull();
  expect(rows[1]!.suggestion).toEqual({ entity_id: "jira:PAY-228", title: null, rule: "linked_to_target" });
  expect(rows[4]!.suggestion?.entity_id).toBe("jira:PAY-200");
});

test("a block on something that is not a ticket needs one, however it is spelled", () => {
  // `ad_hoc_block` answering an offer is the backend saying "not a ticket",
  // which is true of a page as much as of a label.
  const page = block(4, at(10), at(11), { target: { kind: "entity", entity_id: "confluence:ENG-SEPA" } });
  const rows = dayRows({ day: DAY, blocks: [page], offers: { 4: { suggestion: null } }, running: null, now: new Date() });
  expect(rows.map((row) => [row.state, row.suggestion])).toEqual([["label", null]]);
});

test("a passive block on a ticket offers that ticket", () => {
  const passive = block(2, at(10), at(10, 20), { kind: "passive" });
  const rows = dayRows({ day: DAY, blocks: [passive], offers: { 2: onTicket }, running: null, now: new Date() });
  expect(rows[0]!.suggestion?.entity_id).toBe("jira:PAY-231");
});

test("before the backend has answered, an entity block reads as unlogged and a label as needing a ticket", () => {
  const rows = dayRows({
    day: DAY,
    blocks: [block(1, at(9), at(10)), block(2, at(10), at(11), { target: { kind: "label", label: "x" } })],
    offers: {},
    running: null,
    now: new Date(),
  });
  expect(rows.map((row) => row.state)).toEqual(["unlogged", "label"]);
});

test("the running timer is a row that runs to now, and the gap before it is no row", () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    started_at: at(13, 58),
    last_heartbeat: at(14, 30),
  };
  const rows = dayRows({
    day: DAY,
    blocks: [block(3, at(9, 40), at(11, 50))],
    offers: { 3: onTicket },
    running,
    now: new Date(2026, 7, 22, 14, 41),
  });
  expect(states(rows)).toEqual([
    ["unlogged", at(9, 40), at(11, 50)],
    ["running", at(13, 58), at(14, 41)],
  ]);
  expect(rows.filter((row) => row.state === "running")).toHaveLength(1);
});

test("a timer that started yesterday runs on today's strip from midnight, and on no other day", () => {
  const running: RunningTimer = {
    target: { kind: "label", label: "night shift" },
    started_at: at(23, 0, 21),
    last_heartbeat: at(1, 0),
  };
  const today = dayRows({ day: DAY, blocks: [], offers: {}, running, now: new Date(2026, 7, 22, 1, 30) });
  expect(states(today)).toEqual([["running", at(0, 0), at(1, 30)]]);
  const before = dayRows({ day: "2026-08-20", blocks: [], offers: {}, running, now: new Date(2026, 7, 22, 1, 30) });
  expect(before).toEqual([]);
});

test("the five figures: tracked counts the running timer, offered and open stand beside it, open is its rows", () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    started_at: at(13, 58),
    last_heartbeat: at(14, 30),
  };
  const rows = dayRows({
    day: DAY,
    blocks: [
      block(1, at(8, 50), at(9, 25), { worklog_id: 7 }),
      block(2, at(9, 25), at(9, 40), { kind: "passive" }),
      block(3, at(9, 40), at(11, 50)),
      block(5, at(13, 10), at(13, 58), { target: { kind: "label", label: "Standup" } }),
    ],
    offers: { 2: onTicket, 3: onTicket, 5: offer("jira:PAY-200") },
    open: [
      // Two stretches, 25 and 10 minutes: *open* is their sum.
      { started_at: at(12, 10), ended_at: at(12, 20) },
      { started_at: at(12, 35), ended_at: at(13, 0) },
    ],
    running,
    now: new Date(2026, 7, 22, 14, 42),
  });
  expect(dayFigures(rows)).toEqual({
    // 35 + 130 + 48 + 44 (running)
    tracked: 257,
    logged: 35,
    // Only the ticket block: a label cannot be logged, and the timer is not stopped.
    toLog: 130,
    offered: 15,
    open: 35,
  });
});

test("durations read the prototype's way: minutes, never rounded", () => {
  expect([hm(0), hm(45), hm(60), hm(130), hm(257)]).toEqual(["0m", "45m", "1h", "2h 10m", "4h 17m"]);
});

test("the strip runs 08:00 to 18:00 and widens by whole hours to hold a day outside it", () => {
  const plain = dayRows({ day: DAY, blocks: [block(1, at(9), at(10))], offers: {}, running: null, now: new Date() });
  expect(stripWindow(DAY, plain)).toEqual({ from: 8, to: 18 });
  const late = dayRows({
    day: DAY,
    blocks: [block(1, at(6, 30), at(7)), block(2, at(19, 10), at(20, 5))],
    offers: {},
    running: null,
    now: new Date(),
  });
  expect(stripWindow(DAY, late)).toEqual({ from: 6, to: 21 });
});

test("a row is placed by its share of the window, in percent", () => {
  const window = { from: 8, to: 18 };
  expect(place(DAY, window, at(9), at(11, 30))).toEqual({ left: 10, width: 25 });
  // Clipped at either edge rather than drawn off the strip.
  expect(place(DAY, window, at(7), at(8, 30))).toEqual({ left: 0, width: 5 });
  expect(place(DAY, window, at(17), at(19))).toEqual({ left: 90, width: 10 });
});

// #1108: job → its pull request → job is one piece of work, so one *Add*.
const passive = (id: number, from: string, to: string, entity_id: string) =>
  block(id, from, to, { kind: "passive", target: { kind: "entity", entity_id } });
const PR = "gitea:payments#139";

function runsOf(blocks: DayBlock[], offers: DayInput["offers"], open: DayInput["open"] = []) {
  const rows = dayRows({ day: DAY, blocks, offers, open, running: null, now: new Date(2026, 7, 22, 18) });
  return offerRuns(rows).map((run) => [run.entityId, run.rows.map((row) => row.block?.block.id), Math.floor(run.ms / 60_000)]);
}

test("adjacent offered rows that go to one ticket, by their own or by a confirmed link, are one run", () => {
  const blocks = [
    passive(1, at(9), at(9, 20), "jira:PAY-231"),
    passive(2, at(9, 20), at(9, 25), PR),
    passive(3, at(9, 25), at(9, 47), "jira:PAY-231"),
  ];
  expect(runsOf(blocks, { 1: onTicket, 2: offer("jira:PAY-231"), 3: onTicket })).toEqual([
    ["jira:PAY-231", [1, 2, 3], 47],
  ]);
});

test("a guess never joins a run: last_logged or context_anchor in the middle breaks it", () => {
  const blocks = [
    passive(1, at(9), at(9, 20), "jira:PAY-231"),
    passive(2, at(9, 20), at(9, 25), PR),
    passive(3, at(9, 25), at(9, 47), "jira:PAY-231"),
  ];
  expect(runsOf(blocks, { 1: onTicket, 2: offer("jira:PAY-231", "last_logged"), 3: onTicket })).toEqual([]);
  const anchored = { suggestion: { entity_id: "jira:PAY-231", title: null, rule: "context_anchor" as const } };
  expect(runsOf(blocks, { 1: onTicket, 2: anchored, 3: onTicket })).toEqual([]);
  // No suggestion at all breaks it too.
  expect(runsOf(blocks, { 1: onTicket, 2: { suggestion: null }, 3: onTicket })).toEqual([]);
});

test("another ticket, or a row that is not offered, breaks a run; a run is two rows or more", () => {
  const blocks = [
    passive(1, at(9), at(9, 10), "jira:PAY-231"),
    passive(2, at(9, 10), at(9, 20), "jira:PAY-231"),
    passive(3, at(9, 20), at(9, 30), "jira:PAY-228"),
    passive(4, at(9, 30), at(9, 40), "jira:PAY-231"),
    block(5, at(9, 40), at(10)),
    passive(6, at(10), at(10, 10), "jira:PAY-231"),
    passive(8, at(10, 20), at(10, 30), "jira:PAY-231"),
    passive(9, at(10, 30), at(10, 40), "jira:PAY-231"),
  ];
  const offers = { 1: onTicket, 2: onTicket, 3: onTicket, 4: onTicket, 5: onTicket, 6: onTicket, 8: onTicket, 9: onTicket };
  // 10:10 to 10:20 is open time: nothing claims it, so it breaks the run too.
  expect(runsOf(blocks, offers, [{ started_at: at(10, 10), ended_at: at(10, 20) }])).toEqual([
    ["jira:PAY-231", [1, 2], 20],
    ["jira:PAY-231", [8, 9], 20],
  ]);
});
