/**
 * How long ago something happened — the mockup's `ago()`
 * (`signal-miller.html:778`), extended to the range a real corpus spans.
 *
 * Every synced row carries one of these (spec §4: "per-row provenance — where
 * it came from and when it was last seen"), which is why it is a tested
 * function and not five copies of a `Math.round` in five components.
 *
 * One copy lives outside the shell: the adapters' view rows carry their age in
 * a view's text (`knobas_source::view::ago`), because a view row's meta is a
 * string. Under a day the two say the same. Past a day the crate still writes an
 * ISO date, and the read turns it into this one's form (`viewDate`,
 * `lib/time/format.ts`) until the crate follows.
 *
 * The words under a day are `ageLabel`'s (`lib/time/format.ts`), which the
 * stale line's *data is 38 min old* says too (#1986).
 */

import { ageLabel, utcDateWords } from "../time/format";

/**
 * `"just now" | "4 min ago" | "3 h ago" | "19 Aug"`, with the year when it isn't `now`'s.
 *
 * `now` is injectable so the boundaries can be tested rather than waited for.
 *
 * ## Two decisions worth knowing about
 *
 * **A stamp in the future reads as "just now."** `synced_at` is knobas' own
 * clock and a source's `updated_at` is a remote one; the two disagree by
 * seconds routinely and by minutes when a server's clock drifts. "-2 min ago"
 * is not a fact about the item, it is a fact about two clocks, and it looks
 * like a bug in knobas.
 *
 * **Past a day the date is rendered in UTC**, not in the reader's timezone.
 * The mirror stores UTC, the launcher and the sync log quote UTC, and a stamp
 * that means the same thing on two machines is worth more here than one that
 * agrees with the reader's calendar at the midnight boundary. Anything recent
 * enough for that distinction to matter is still being rendered as a duration.
 * It reads as the one date form (F-049); {@link agoStamp} is the ISO stamp for
 * a title, to paste into a bug report.
 */
export function ago(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "—";
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return "—";

  const minutes = Math.round((now.getTime() - then.getTime()) / 60_000);
  if (minutes < 1) return "just now";
  if (Math.round(minutes / 60) < 24) return `${ageLabel(iso, now)} ago`;

  return utcDateWords(then, now);
}

/** The ISO date behind an {@link ago} that reads as a date, for its title; `undefined` while it reads as a duration or there is no date. */
export function agoStamp(iso: string | null | undefined, now: Date = new Date()): string | undefined {
  if (!iso) return undefined;
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return undefined;
  return Math.round((now.getTime() - then.getTime()) / 3_600_000) < 24 ? undefined : then.toISOString().slice(0, 10);
}
