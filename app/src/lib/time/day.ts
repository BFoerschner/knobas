/**
 * What a day *is*, and where its gaps are — the pure half of the day review
 * (issue #279).
 *
 * Separate from `DayReview.svelte` for the reason `shell/timer.ts` is separate
 * from the strip: the two rules here with real content — which instants a day
 * covers, and which stretches of it nobody accounted for — are worth driving
 * without a mounted component.
 *
 * ## A day is local, and it is this side's to know
 *
 * `day_blocks` takes two instants, not a `YYYY-MM-DD`. The machine's timezone
 * is a fact only the webview holds — nothing on this bridge has ever carried
 * one — and a UTC offset sent instead would be the wrong *shape* as well as
 * the wrong owner: a day containing a daylight-saving change is 23 or 25 hours
 * long and has two offsets, so an offset would move one edge of the strip
 * wrongly twice a year. {@link dayBounds} asks `Date` for that day's midnight
 * and the next day's, which is right by construction in every zone.
 */
import type { DayBlock } from "../ipc/time";

/** Weekday and month names, so a date reads the same in every locale. */
const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

/**
 * The `YYYY-MM-DD` an instant falls on **in the reader's own calendar**.
 *
 * Not `toISOString().slice(0, 10)`, which is UTC's answer: at 23:30 in Berlin
 * that is tomorrow, and *Today* would open a day the reader has not lived yet.
 */
export function dayKey(at: Date): string {
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
}

/** The `Date` at local midnight on `key`. */
function midnight(key: string): Date {
  const [year, month, day] = key.split("-").map(Number);
  return new Date(year ?? 1970, (month ?? 1) - 1, day ?? 1, 0, 0, 0, 0);
}

/**
 * The half-open interval `day_blocks` is asked for: this day's local midnight,
 * and the next day's.
 *
 * `getDate() + 1` rather than "plus 24 hours": `Date` rolls the month, the
 * year and the daylight-saving change for us, and 24 hours is wrong twice a
 * year by exactly the hour a person is most likely to have been working.
 */
export function dayBounds(key: string): { from: Date; to: Date } {
  const from = midnight(key);
  const to = new Date(from);
  to.setDate(to.getDate() + 1);
  return { from, to };
}

/** The day `days` away from `key`, month and year ends included. */
export function shiftDay(key: string, days: number): string {
  const at = midnight(key);
  at.setDate(at.getDate() + days);
  return dayKey(at);
}

/** `"Thursday 3 September 2026"` — the heading, checkable against a calendar. */
export function dayLabel(key: string): string {
  const at = midnight(key);
  return `${WEEKDAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]} ${at.getFullYear()}`;
}

/**
 * `"09:05"` — the clock the reader was looking at when this instant passed.
 *
 * With `on` given, an instant that fell on **another day** carries which:
 * `"01:00 (+1)"`, `"23:30 (−1)"`. `day_blocks` returns a block that ran
 * through midnight on *both* days it touched, deliberately, so without this
 * the strip for the second day opens `23:30 → 01:00` and reads as a day drawn
 * out of order rather than as work that started the night before.
 *
 * The offset is in days and is computed from the two midnights, so a day
 * containing a daylight-saving change — 23 or 25 hours long — still counts as
 * one day.
 */
export function clockReading(iso: string, on?: string): string {
  const at = new Date(iso);
  const clock = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  if (on === undefined) return clock;

  const days = Math.round(
    (midnight(dayKey(at)).getTime() - midnight(on).getTime()) / 86_400_000,
  );
  if (days === 0) return clock;
  return `${clock} (${days > 0 ? "+" : "−"}${Math.abs(days)})`;
}

/** Whole minutes between two instants, rounded down. */
export function minutesBetween(from: string, to: string): number {
  return Math.max(0, Math.floor((new Date(to).getTime() - new Date(from).getTime()) / 60_000));
}

/**
 * `"45 min"`, `"1 h"`, `"2 h 15 min"`.
 *
 * Minutes, never decimal hours: `CONTEXT.md`'s timesheet is minute-granular
 * with no rounding, because knobas must never invent a rounding policy a
 * person's Jira may not have. `"1.25 h"` is that invention in miniature.
 */
export function durationReading(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

/**
 * One thing on the strip: a block, or the unaccounted time before it.
 *
 * A tagged union rather than blocks with an optional gap hanging off each,
 * because the strip draws the two **differently and deliberately**: a gap is
 * time nobody claimed, and a person has to be able to see one without reading
 * a number. It is also what keeps a gap distinguishable from a block of no
 * length, which is a real thing (a timer started and stopped inside a second;
 * a stranded timer closed before its first heartbeat) and the opposite of a
 * gap.
 */
export type DayStripSegment =
  | { kind: "block"; key: string; block: DayBlock }
  | { kind: "gap"; key: string; from: string; to: string; minutes: number };

/**
 * The day, laid out: every block in time order with the gaps between them.
 *
 * Three rules, and each of them is a thing that would otherwise be drawn
 * wrong:
 *
 * * **A gap of no length is not a gap.** Two blocks that butt onto each other
 *   have nothing between them, and a `>=` here would put an empty marker
 *   between every pair.
 * * **A block of no length is still a block.** It is what a timer stopped
 *   inside one second leaves, and what the relaunch sweep writes for a timer
 *   that died before its first heartbeat. Dropping it would be knobas deciding
 *   the block did not happen.
 * * **The gap is measured from the furthest end so far**, not from the
 *   previous block's end. Blocks can overlap once they are editable — moving
 *   one start back over the block before it is a single edit — and a gap
 *   measured from the previous block would come out negative.
 *
 * Nothing is drawn before the first block or after the last. That time is not
 * unaccounted work, it is the rest of a person's life, and a strip that opened
 * with an eight-hour hole would say something false about every day.
 *
 * The order is this function's, not the backend's: `day_blocks` already sorts,
 * and a layout that depended on it would be a rule stated in only one of the
 * two places that has to hold it.
 */
export function segmentsOf(blocks: DayBlock[]): DayStripSegment[] {
  // Compared as instants, never as strings. Both sides of this bridge produce
  // RFC 3339 in UTC, but chrono omits a zero fraction and `Date.toISOString`
  // writes `.000` — so `"…:00Z"` sorts *after* `"…:00.000Z"` even though they
  // are the same moment, and a lexicographic sort would put a block after a
  // gap it starts at.
  const instant = (iso: string) => new Date(iso).getTime();
  const ordered = [...blocks].sort((left, right) => {
    const by = instant(left.block.started_at) - instant(right.block.started_at);
    return by === 0 ? left.block.id - right.block.id : by;
  });

  const segments: DayStripSegment[] = [];
  let reached: string | null = null;
  for (const entry of ordered) {
    if (reached !== null && instant(entry.block.started_at) > instant(reached)) {
      segments.push({
        kind: "gap",
        key: `gap-${reached}`,
        from: reached,
        to: entry.block.started_at,
        minutes: minutesBetween(reached, entry.block.started_at),
      });
    }
    segments.push({ kind: "block", key: `block-${entry.block.id}`, block: entry });
    if (reached === null || instant(entry.block.ended_at) > instant(reached)) {
      reached = entry.block.ended_at;
    }
  }
  return segments;
}
