/**
 * The two things the shell has to work out before it can ask for a worklog
 * draft — the pure half of the worklog, so both can be driven without a bridge
 * or a mounted component (#280).
 *
 * Both are about **the reader's own reckoning of time**, which is the one fact
 * the backend cannot supply: `worklog_draft` takes a day and an offset, and
 * this machine is the only thing that knows which day the reader means.
 */

/**
 * The local calendar day a moment falls on, as `YYYY-MM-DD`.
 *
 * Deliberately **not** `toISOString().slice(0, 10)`: that is the day in UTC,
 * and for a reader in Berlin an afternoon that ends at 23:10 would be filed
 * under tomorrow — the very block they just closed would then be missing from
 * the draft, with nothing on screen to say why. The local parts are what a
 * person means by "today".
 *
 * Zero-padded by hand rather than through `toLocaleDateString`, whose output
 * is locale-shaped (`03/09/2026`, `2026/9/3`) and would reach the backend as
 * something `NaiveDate` refuses.
 */
export function localDay(at: Date): string {
  const year = String(at.getFullYear()).padStart(4, "0");
  const month = String(at.getMonth() + 1).padStart(2, "0");
  const day = String(at.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/**
 * The reader's offset from UTC, in minutes, in the sign the backend expects:
 * **east of UTC is positive**.
 *
 * `Date.prototype.getTimezoneOffset` answers the opposite sign — Berlin in
 * summer is `-120` there — because it is defined as the offset to *add* to
 * local time to reach UTC. Every caller negating it inline is one caller
 * eventually forgetting to, and a sign error here is a day boundary silently
 * four hours out.
 */
export function offsetMinutes(at: Date = new Date()): number {
  // `|| 0` folds `-0` -- which is what negating UTC's own zero produces -- back
  // to `0`. Both cross the bridge as `0`, so this is not a correctness fix; it
  // is so that a test comparing the two is comparing numbers rather than
  // discovering `Object.is(-0, 0)`.
  return -at.getTimezoneOffset() || 0;
}

/**
 * The local clock reading of an instant, as `HH:MM` — what a time field shows.
 *
 * Zero-padded and 24-hour, because that is what `<input type="time">` takes
 * and gives back; `toLocaleTimeString` is locale-shaped and would hand the
 * field a value it refuses.
 */
export function clockOf(iso: string): string {
  const at = new Date(iso);
  return `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
}

/**
 * `iso` moved to the local clock reading `hhmm`, as an RFC 3339 instant.
 *
 * **The calendar day does not move**, which is the whole of what this is for:
 * the draft's interval is editable, and a reader correcting *when the work
 * began* is saying 08:30 rather than 09:00 on the day they logged — not on
 * some other day. Seconds and milliseconds are cleared, because a time field
 * does not carry them and leaving the original's would make an edited start
 * read `08:30:47`.
 *
 * A malformed reading — which `<input type="time">` produces while it is being
 * typed into, and when it is cleared — leaves the instant alone. The
 * alternative is `Invalid Date` reaching the backend as the moment a worklog
 * began.
 */
export function atClock(iso: string, hhmm: string): string {
  const match = /^(\d{1,2}):(\d{2})$/.exec(hhmm);
  const at = new Date(iso);
  if (!match || Number.isNaN(at.getTime())) return iso;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 23 || minutes > 59) return iso;
  at.setHours(hours, minutes, 0, 0);
  return at.toISOString();
}
