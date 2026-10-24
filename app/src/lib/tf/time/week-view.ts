/**
 * The Time view's Week tab, as arithmetic (#828, R-13.4): what each cell
 * says, the four figures, and the held worklogs' notes.
 *
 * Pure, over the reads the rooms UI's week timesheet already makes:
 * `week_timesheet` for the rows, the timer for the running stretch, and
 * `pending_writes` for why a worklog is held. The component asks; this decides.
 *
 * ## Minutes, floored per cell
 *
 * `CONTEXT.md`'s timesheet is minute-granular with no rounding, and
 * `week.ts`'s `minutesOf` floors. Each cell floors once, and every total adds
 * the cells as drawn, so a row's *Week* column is the sum of the numbers
 * beside it.
 *
 * ## The running timer counts as tracked
 *
 * The Day tab draws the running timer as a row and counts it into *tracked*
 * and *to log*. The week's read knows only blocks, and the timer is not one
 * until it stops, so today's cell on its target gets the stretch since today's
 * midnight here, and the two tabs say the same number about the same morning.
 */
import type { QueuedWrite } from "../../ipc/sources";
import type { RunningTimer, TimerTarget, Week, WeekCell } from "../../ipc/time";
import { minutesLabel } from "../../time/format";
import { unconfirmedSend, withdrawnWorklog } from "../../shell/write-queue.svelte";
import { targetReading } from "../../shell/timer";
import { dayBounds, dayKey, dayLabel } from "../../time/day";
import { isWeekend, minutesOf } from "../../time/week";

export interface WeekCellView {
  /** `YYYY-MM-DD`. */
  day: string;
  weekend: boolean;
  /** Whole minutes, floored. */
  tracked: number;
  logged: number;
  held: number;
  toLog: number;
  /** Whether the logged, held and unlogged bar is drawn: never on the no-target row. */
  bar: boolean;
  /** `"1h 30m to log"`, `"1h held"`, `"logged"`; `null` for an empty cell or the no-target row. */
  note: string | null;
}

export interface WeekRowView {
  /** `null` for the "no target, app open" row. */
  target: TimerTarget | null;
  title: string | null;
  cells: WeekCellView[];
  /** The row's tracked minutes over the week. */
  total: number;
}

export interface WeekFigures {
  tracked: number;
  logged: number;
  held: number;
  /** Unlogged minutes on a target: the no-target row has nowhere to go. */
  toLog: number;
}

export interface WeekView {
  rows: WeekRowView[];
  /** Tracked minutes per day, every row. */
  dayTotals: number[];
  figures: WeekFigures;
}

const sameTarget = (a: TimerTarget | null, b: TimerTarget): boolean =>
  a !== null &&
  (a.kind === "entity" ? b.kind === "entity" && a.entity_id === b.entity_id : b.kind === "label" && a.label === b.label);

/** Seconds the timer has run on `day` (`YYYY-MM-DD`), up to `now`. */
function runningOn(day: string, running: RunningTimer, now: Date): number {
  const { from, to } = dayBounds(day);
  const start = Math.max(Date.parse(running.started_at), from.getTime());
  const end = Math.min(now.getTime(), to.getTime());
  return Math.max(0, Math.floor((end - start) / 1000));
}

function cellView(
  day: string,
  index: number,
  cell: WeekCell | undefined,
  noTarget: boolean,
  runningSeconds = 0,
): WeekCellView {
  const tracked = minutesOf((cell?.tracked_seconds ?? 0) + runningSeconds);
  const logged = minutesOf(cell?.logged_seconds ?? 0);
  const held = minutesOf(cell?.held_seconds ?? 0);
  const toLog = minutesOf((cell?.unlogged_seconds ?? 0) + runningSeconds);
  const empty = tracked === 0 && logged === 0 && held === 0;
  const note = noTarget || empty ? null : toLog > 0 ? `${minutesLabel(toLog)} to log` : held > 0 ? `${minutesLabel(held)} held` : "logged";
  return { day, weekend: isWeekend(index), tracked, logged, held, toLog, bar: !noTarget && !empty, note };
}

/** The week as the tab draws it, with the running timer counted into today. */
export function weekView(week: Week, running: RunningTimer | null, now: Date): WeekView {
  const rows = [...week.rows];
  let runningRow = -1;
  const today = week.days.indexOf(dayKey(now));
  if (running !== null && today >= 0) {
    runningRow = rows.findIndex((row) => sameTarget(row.target, running.target));
    if (runningRow < 0) {
      const open = rows.findIndex((row) => row.target === null);
      runningRow = open < 0 ? rows.length : open;
      rows.splice(runningRow, 0, { target: running.target, title: null, cells: [] });
    }
  }
  const extra = running === null || today < 0 ? 0 : runningOn(week.days[today]!, running, now);

  const views: WeekRowView[] = rows.map((row, rowIndex) => {
    const open = row.target === null;
    const cells = week.days.map((day, index) =>
      cellView(day, index, row.cells[index], open, rowIndex === runningRow && index === today ? extra : 0),
    );
    return { target: row.target, title: row.title, cells, total: cells.reduce((sum, c) => sum + c.tracked, 0) };
  });

  const dayTotals = week.days.map((_, index) => views.reduce((sum, row) => sum + (row.cells[index]?.tracked ?? 0), 0));
  const add = (pick: (c: WeekCellView) => number, keep: (row: WeekRowView) => boolean = () => true) =>
    views.filter(keep).reduce((sum, row) => sum + row.cells.reduce((n, c) => n + pick(c), 0), 0);
  return {
    rows: views,
    dayTotals,
    figures: {
      tracked: add((c) => c.tracked),
      logged: add((c) => c.logged),
      held: add((c) => c.held),
      toLog: add((c) => c.toLog, (row) => row.target !== null),
    },
  };
}

/** `"Thursday"`: the weekday `dayLabel` names, so there is one list of them. */
const weekday = (key: string) => dayLabel(key).split(" ")[0]!;

/** Why a worklog waits on a person: a send never answered first (#1966), then the source's own words, else the queue's state. */
function reasonOf(write: QueuedWrite): string {
  const unanswered = unconfirmedSend(write);
  if (unanswered !== null)
    return `${unanswered.source} did not answer, so it may have arrived. Check ${unanswered.source}, then send it again or discard it.`;
  const said = write.detail?.trim();
  if (said) return /[.!?]$/.test(said) ? said : `${said}.`;
  if (write.state === "refused") return "the source refused it.";
  return write.source_enabled ? "the ticket changed after you logged it." : "its source is turned off.";
}

/**
 * One sentence per worklog in this week that waits on a person, held or
 * refused: how much, of which ticket, on which day, and why.
 *
 * `days` is the week's `YYYY-MM-DD`s, Monday first. A worklog is on the day
 * its logged span began, by the reader's clock.
 */
export function heldNotes(writes: QueuedWrite[], days: string[]): string[] {
  const notes: string[] = [];
  for (const write of writes) {
    if (write.state !== "held" && write.state !== "refused") continue;
    const worklog = withdrawnWorklog(write);
    if (worklog === null) continue;
    const index = days.indexOf(dayKey(new Date(worklog.started)));
    if (index < 0) continue;
    const key = targetReading({ kind: "entity", entity_id: worklog.entity });
    notes.push(`${minutesLabel(minutesOf(worklog.seconds))} of ${key} on ${weekday(days[index]!)} is held: ${reasonOf(write)}`);
  }
  return notes;
}
