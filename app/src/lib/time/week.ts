/**
 * What a *week* is, which of its columns are drawn, and how a cell reads —
 * the pure half of the timesheet (issue #283).
 *
 * Separate from `WeekTimesheet.svelte` for the reason `day.ts` is separate
 * from `DayReview.svelte`: the rules with real content here — which seven
 * intervals a week is, and which of them a reader is shown — are worth driving
 * without a mounted component, and one of them (the intervals) is arithmetic
 * that is wrong twice a year in a way no screenshot shows.
 *
 * ## The week is the reader's, and this side is the only thing that knows it
 *
 * `week_timesheet` takes seven {@link DayWindow}s, each carrying a date and
 * the two instants it spans, because the machine's timezone is a fact only the
 * webview holds — the rule `day.ts` records in full. A week is where that
 * matters most: the week containing a daylight-saving change has one day of 23
 * or 25 hours in it, so seven *equal* windows built by adding 24 hours would
 * put one day's evening in the next day's column, in the one week of the year
 * a reader is most likely to check. {@link weekWindows} asks `Date` for seven
 * consecutive local midnights instead, which is right by construction in every
 * zone.
 *
 * ## Monday to Sunday
 *
 * The spec's week, and `Date.getDay()`'s is Sunday-first — so the offset back
 * to Monday is `(getDay() + 6) % 7` rather than `getDay() - 1`, which is off
 * by seven days on every Sunday.
 */
import type { DayWindow, Week, WeekCell } from "../ipc/time";

import { MONTHS, dayBounds, shiftDay } from "./day";

/** How many days a week has. Named because it is also an index bound. */
export const DAYS = 7;

/**
 * The column indices the weekend occupies — Saturday and Sunday.
 *
 * Indices into a Monday-first week, so a rename of the weekend (a Sunday-first
 * calendar, a Gulf working week) is one constant rather than a scan for `5`.
 */
const WEEKEND = [5, 6];

/** The `YYYY-MM-DD` of the Monday of the week `key` falls in. */
export function mondayOf(key: string): string {
  const [year, month, day] = key.split("-").map(Number);
  const at = new Date(year ?? 1970, (month ?? 1) - 1, day ?? 1);
  return shiftDay(key, -((at.getDay() + 6) % 7));
}

/**
 * The seven windows the week containing `key` is, Monday first.
 *
 * Each from `dayBounds`, which asks `Date` for that day's own midnight and the
 * next one — so a 23- or 25-hour day is 23 or 25 hours here too.
 */
export function weekWindows(key: string): DayWindow[] {
  const monday = mondayOf(key);
  return Array.from({ length: DAYS }, (_, index) => {
    const day = shiftDay(monday, index);
    const { from, to } = dayBounds(day);
    return { day, from: from.toISOString(), to: to.toISOString() };
  });
}

/** Whether the column at `index` is a weekend day. */
export function isWeekend(index: number): boolean {
  return WEEKEND.includes(index);
}

/**
 * Whether a cell says nothing at all.
 *
 * **Every number, not just *tracked***: a Saturday whose only content is a
 * held worklog is a Saturday a person needs to see, and a cell judged empty on
 * `tracked === 0` would hide exactly the day that needs acting on. The same
 * holds for a passive block knobas is offering — it is on the strip above, so
 * it is not nothing.
 *
 * `unlogged_seconds` is deliberately absent: it is derived from the other
 * three, so a cell where it is the only non-zero number cannot exist, and
 * naming it here would suggest it could.
 *
 * One predicate, exported, because the timesheet asks this question twice —
 * {@link dayIsEmpty} for the weekend collapse, and the view for the em dash it
 * draws in a cell with nothing in it.
 */
export function cellIsEmpty(cell: WeekCell | undefined): boolean {
  return (
    cell === undefined ||
    (cell.tracked_seconds === 0 &&
      cell.offered_seconds === 0 &&
      cell.logged_seconds === 0 &&
      cell.held_seconds === 0)
  );
}

/** Whether a column has nothing on it at all. */
export function dayIsEmpty(week: Week, index: number): boolean {
  return week.rows.every((row) => cellIsEmpty(row.cells[index]));
}

/**
 * The columns to draw: every weekday, a weekend day with something on it, and
 * — whatever else is true of it — **the day the strip is standing on**.
 *
 * **Weekdays are never collapsed, empty or not.** An empty Wednesday is a fact
 * about the week — the gap a person is looking for — while an empty Saturday
 * is the ordinary case and a column of dashes for it is noise. That asymmetry
 * is the whole of "empty weekends collapsed".
 *
 * **`current` is the exception that keeps the highlight honest.** The strip's
 * date drives the highlighted column; a reader who steps the strip onto an
 * empty Saturday would otherwise be looking at a week with the one day they
 * are on missing from it, and nothing highlighted. So that column stays,
 * empty or not, for exactly as long as the strip is on it. Omitting `current`
 * asks the plain question, which is what a test of the collapse rule wants.
 */
export function visibleColumns(week: Week, current?: string): number[] {
  return week.days
    .map((_, index) => index)
    .filter(
      (index) =>
        week.days[index] === current || !isWeekend(index) || !dayIsEmpty(week, index),
    );
}

/**
 * Whole minutes, **rounded down and never up**.
 *
 * `CONTEXT.md`'s timesheet is minute-granular with no rounding, because knobas
 * must never invent a rounding policy a person's Jira may not have. Down
 * rather than nearest for the same reason the day review's `minutesBetween`
 * floors: the number a timesheet shows may be quoted to somebody, and the only
 * safe direction to be wrong in is the one that never claims time that was not
 * worked.
 */
export function minutesOf(seconds: number): number {
  return Math.max(0, Math.floor(seconds / 60));
}

/** `"Mon 24"` — a column heading a person can check against a calendar. */
const WEEKDAY_ABBREVIATIONS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function columnHeading(key: string, index: number): string {
  const [, , day] = key.split("-").map(Number);
  return `${WEEKDAY_ABBREVIATIONS[index] ?? ""} ${day ?? ""}`.trim();
}

/**
 * `"24–30 August 2026"` — the week's own heading.
 *
 * Built from the two ends rather than from a locale formatter, the rule
 * `day.ts`'s `dayLabel` records: a heading that read differently on two
 * machines would be one more thing a reader cannot check against anything.
 */
export function weekLabel(days: string[]): string {
  const first = days[0];
  const last = days[days.length - 1];
  if (first === undefined || last === undefined) return "";
  const [fromYear, fromMonth, fromDay] = first.split("-").map(Number);
  const [toYear, toMonth, toDay] = last.split("-").map(Number);
  const fromName = MONTHS[(fromMonth ?? 1) - 1];
  const toName = MONTHS[(toMonth ?? 1) - 1];
  if (fromYear !== toYear) {
    return `${fromDay} ${fromName} ${fromYear} – ${toDay} ${toName} ${toYear}`;
  }
  if (fromMonth !== toMonth) {
    return `${fromDay} ${fromName} – ${toDay} ${toName} ${toYear}`;
  }
  return `${fromDay}–${toDay} ${fromName} ${fromYear}`;
}
