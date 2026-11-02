/**
 * The Day tab's read (#1418).
 *
 * Local instants, for `day-view.test.ts`'s reason: a day is a local thing.
 */
import { expect, test, vi } from "vitest";

import type { Block, DayBlock } from "../../ipc/time";
import { dayOffsetMinutes } from "../../time/day";
import { readDay, type DayReads } from "./day-read";

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

test("the Day tab asks for each offer with the offset the draft sends for that day", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  try {
    vi.setSystemTime(new Date(2026, 10, 5, 12));
    const asked: number[] = [];
    const io: DayReads = {
      dayBlocks: async () => ({ blocks: [block(1, new Date(2026, 9, 20, 9).toISOString(), new Date(2026, 9, 20, 10).toISOString())], open: [], past_horizon: false }),
      adHocBlock: async (_id, when) => {
        asked.push(when.offsetMinutes);
        return null;
      },
    };
    await readDay(io, "2026-10-20");
    expect(asked).toEqual([120]);
    expect(asked[0]).toBe(dayOffsetMinutes("2026-10-20"));
  } finally {
    vi.useRealTimers();
  }
});
