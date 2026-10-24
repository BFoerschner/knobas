/**
 * The one place a duration, a clock reading or an entity key is spelled
 * (#1678).
 *
 * Before this module five functions each wrote a length their own way and two
 * wrote "when": two of them were both called `hm` and took different units
 * (seconds in one, minutes in the other), so a page importing the wrong one
 * was off by sixty and the type checker could not tell. The names here carry
 * the unit instead.
 *
 * Minutes, never decimal hours, and never days: the timesheet is
 * minute-granular with no rounding (CONTEXT.md, *Timesheet*), and how long a
 * working day is belongs to the source's settings.
 */

/**
 * The key half of an entity id, past its first colon only: `jira:PAY-231`
 * reads `PAY-231`, `confluence:ENG:SEPA design` reads `ENG:SEPA design`. The
 * one copy: `house-rules.test.ts` refuses the slice written in place (#1986).
 */
export function keyOf(entityId: string): string {
  return entityId.slice(entityId.indexOf(":") + 1);
}

/** A length given in **minutes**: `2h 10m`, `45m`, `3h`. */
export function minutesLabel(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest}m`;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** A length given in **seconds**, as a run's: `38 s`, `4 m 12 s`. */
export function secondsLabel(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  return `${Math.floor(seconds / 60)} m ${String(seconds % 60).padStart(2, "0")} s`;
}

const pad = (n: number) => String(n).padStart(2, "0");
const dateOf = (at: Date) => `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;

const clockOf = (at: Date) => `${pad(at.getHours())}:${pad(at.getMinutes())}`;

/** `today`, `yesterday` or `null` for an older (or later) day, in the reader's calendar. */
function dayWord(at: Date, now: Date): "today" | "yesterday" | null {
  if (dateOf(at) === dateOf(now)) return "today";
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  return dateOf(at) === dateOf(yesterday) ? "yesterday" : null;
}

/**
 * An instant against `now`: `today 11:48`, `yesterday 16:05`, else the date.
 * `never` for a source that never said; `""` for an instant that is not one.
 *
 * `bare` drops the word on today (`11:48`) for a column that is only ever
 * read as recent. Yesterday is worded the same either way, so two surfaces
 * showing one instant never disagree about which day it was.
 */
export function clockLabel(iso: string | null, now: Date = new Date(), bare = false): string {
  if (iso === null) return "never";
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  const word = dayWord(at, now);
  if (word === null) return dateOf(at);
  return bare && word === "today" ? clockOf(at) : `${word} ${clockOf(at)}`;
}

/**
 * {@link clockLabel} for a line that says when something was written or
 * taken (a comment, a backup, a sync run), where an older day keeps its
 * clock: `today 09:35`, `yesterday 17:41`, `2026-08-12 09:14`. `bare` drops
 * the word on today as there. `""` for an instant that is not one (#1986).
 */
export function stampLabel(iso: string, now: Date = new Date(), bare = false): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  return dayWord(at, now) === null ? `${dateOf(at)} ${clockOf(at)}` : clockLabel(iso, now, bare);
}

/**
 * How long ago an instant was, without the *ago*: `38 min`, `2 h`, `3 days`,
 * never under `1 min`. The stale line's *data is 38 min old* and the status
 * bar's *synced 4 min ago* (`shell/time.ts`, `ago`) both say it (#1986).
 *
 * An age, not a length: a timesheet length is {@link minutesLabel}'s `38m`.
 */
export function ageLabel(iso: string, now: Date = new Date()): string {
  const minutes = Math.max(1, Math.round((now.getTime() - new Date(iso).getTime()) / 60_000));
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours} h`;
  return `${Math.round(hours / 24)} days`;
}
