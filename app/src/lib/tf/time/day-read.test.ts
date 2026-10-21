/**
 * Today's figures as the launcher's board asks for them (#1418): the Day
 * tab's read and arithmetic, at `now`, with the timer that is running.
 *
 * Local instants, for `day-view.test.ts`'s reason: a day is a local thing.
 */
import { expect, test, vi } from "vitest";

import type { AdHocBlock, Block, DayBlock, DayRecord as Wire } from "../../ipc/time";
import { readTodayFigures, type DayReads } from "./day-read";

const at = (hour: number, minute = 0) => new Date(2026, 7, 22, hour, minute).toISOString();

function block(id: number, from: string, to: string, over: Partial<Block> = {}): DayBlock {
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
    title: null,
  };
}

test("today's figures are the Day tab's: tracked counts the timer, to log only what is on a ticket and unlogged", async () => {
  const blocks = [
    // An hour, logged.
    block(1, at(8), at(9), { worklog_id: 7 }),
    // Half an hour on a ticket, not logged: to log.
    block(2, at(9), at(9, 30)),
    // A quarter on a label: tracked, and nothing a worklog can go to.
    block(3, at(9, 30), at(9, 45), { target: { kind: "label", label: "Standup" } }),
    // Passive: offered, beside tracked rather than in it.
    block(4, at(10), at(11), { kind: "passive" }),
  ];
  const record: Wire = { blocks, open: [], past_horizon: false };
  const dayBlocks = vi.fn(async (_from: string, _to: string) => record);
  const offers: Record<number, AdHocBlock | null> = { 2: null, 3: { suggestion: null } };
  const io: DayReads = { dayBlocks, adHocBlock: async (id) => offers[id] ?? null };

  const figures = await readTodayFigures(
    new Date(2026, 7, 22, 14, 0),
    { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: at(13, 50), last_heartbeat: at(14) },
    io,
  );

  // 60 + 30 + 15 + the timer's 10.
  expect(figures.tracked).toBe(115);
  expect(figures.toLog).toBe(30);
  // The day asked for is the local day of `now`.
  const [from, to] = dayBlocks.mock.calls[0]!;
  expect(new Date(from).getDate()).toBe(22);
  expect(new Date(to).getTime() - new Date(from).getTime()).toBe(24 * 3_600_000);
});
