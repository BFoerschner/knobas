/**
 * The Day view's read, shared (#1418): the Day tab asks it for the day it
 * draws, and the shell asks it for today's two figures the launcher's board
 * gives. One read, so the board and the Day tab cannot disagree about what
 * "tracked" and "to log" were made of.
 *
 * `day-view.ts` stays the arithmetic; this is the asking.
 */
import type { AdHocBlock, DayBlock, OpenStretch, RunningTimer } from "../../ipc/time";
import { adHocBlock as realAdHocBlock, dayBlocks as realDayBlocks } from "../../ipc/time";
import { dayBounds, dayKey, readerDay } from "../../time/day";
import { dayFigures, dayRows, type DayFigures } from "./day-view";

/** The two reads a day is made of. */
export interface DayReads {
  dayBlocks: typeof realDayBlocks;
  adHocBlock: typeof realAdHocBlock;
}

/** One day, as the Day tab draws it. */
export interface DayRecord {
  day: string;
  blocks: DayBlock[];
  pastHorizon: boolean;
  offers: Record<number, AdHocBlock | null>;
  /** The day's focused time nothing claims (#901). */
  open: OpenStretch[];
}

/**
 * The day, its open stretches, and what `ad_hoc_block` says of each unlogged
 * block, as one answer so they cannot disagree.
 *
 * An offer that fails is left out, and the row falls back on the target's
 * shape. That is not worth blanking the day for.
 */
export async function readDay(io: DayReads, on: string): Promise<DayRecord> {
  const { from, to } = dayBounds(on);
  const record = await io.dayBlocks(from.toISOString(), to.toISOString());
  const offers: Record<number, AdHocBlock | null> = {};
  const when = readerDay(on);
  await Promise.all(
    record.blocks
      .filter((entry) => entry.block.worklog_id === null)
      .map((entry) =>
        io.adHocBlock(entry.block.id, when).then(
          (offer) => void (offers[entry.block.id] = offer),
          () => {},
        ),
      ),
  );
  return { day: on, blocks: record.blocks, pastHorizon: record.past_horizon, offers, open: record.open };
}

/**
 * Today's figures, as the Day tab would show them at `now` with `running` on
 * the clock: the same read, the same rows, the same sums.
 */
export async function readTodayFigures(
  now: Date,
  running: RunningTimer | null,
  io: DayReads = { dayBlocks: realDayBlocks, adHocBlock: realAdHocBlock },
): Promise<DayFigures> {
  const day = dayKey(now);
  const record = await readDay(io, day);
  return dayFigures(
    dayRows({ day, blocks: record.blocks, offers: record.offers, open: record.open, running, now }),
  );
}
