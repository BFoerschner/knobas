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

/**
 * A length given in **seconds**, as a run's: `38s`, `4m 12s`, `4m`, and from an hour {@link minutesLabel}'s `1h 2m`, where
 * the seconds stop mattering. One spelling with the timesheet's, so a run and a worklog read alike (design system, Words).
 */
export function secondsLabel(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes >= 60) return minutesLabel(minutes);
  const rest = seconds % 60;
  return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`;
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

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY_MS = 86_400_000;

/** The one date form: `4 Nov`, and `4 Nov 2025` in another year than `now`'s. */
function dateWords(at: Date, now: Date): string {
  const date = `${at.getDate()} ${MONTHS[at.getMonth()]}`;
  return at.getFullYear() === now.getFullYear() ? date : `${date} ${at.getFullYear()}`;
}

/** Whether `at` fell in the six days before yesterday's end, so its weekday names one day only. */
function thisPastWeek(at: Date, now: Date): boolean {
  const midnight = (day: Date) => new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(at)) / DAY_MS);
  return days > 0 && days < 7;
}

/**
 * An instant against `now`, on the one ladder a row's date climbs (F-049): the
 * time with its word, `today 11:48`, then `yesterday 16:05`, then the weekday
 * for the week past, `Wed 14:32`, then the date, `4 Nov`.
 * `never` for a source that never said; `""` for an instant that is not one.
 *
 * `bare` drops the word on today (`11:48`) for a column that is only ever
 * read as recent. Yesterday is worded the same either way, so two surfaces
 * showing one instant never disagree about which day it was. A row under a date
 * heading says neither: {@link timeLabel}. An age in a sentence stays an age:
 * {@link ageLabel}.
 */
export function clockLabel(iso: string | null, now: Date = new Date(), bare = false): string {
  if (iso === null) return "never";
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  const word = dayWord(at, now);
  if (word !== null) return bare && word === "today" ? clockOf(at) : `${word} ${clockOf(at)}`;
  return thisPastWeek(at, now) ? `${DAYS[at.getDay()]} ${clockOf(at)}` : dateWords(at, now);
}

/**
 * {@link clockLabel} for a line that says when something was written or
 * taken (a comment, a backup, a sync run), where an older day keeps its
 * clock: `today 09:35`, `yesterday 17:41`, `Wed 14:32`, `12 Aug 09:14`. `bare`
 * drops the word on today as there. `""` for an instant that is not one (#1986).
 */
export function stampLabel(iso: string, now: Date = new Date(), bare = false): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  return dayWord(at, now) === null && !thisPastWeek(at, now) ? `${dateWords(at, now)} ${clockOf(at)}` : clockLabel(iso, now, bare);
}

/** The time alone, for a row under a heading that already names its day (F-049). `""` for an instant that is not one. */
export function timeLabel(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : clockOf(at);
}

/** A date heading over rows of one day: `Today`, `Yesterday`, else `Wed 4 Nov`. */
export function dayHeadingLabel(at: Date, now: Date = new Date()): string {
  const word = dayWord(at, now);
  if (word === "today") return "Today";
  if (word === "yesterday") return "Yesterday";
  return `${DAYS[at.getDay()]} ${dateWords(at, now)}`;
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
