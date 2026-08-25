/**
 * How long ago something happened — the mockup's `ago()`
 * (`signal-miller.html:778`), extended to the range a real corpus spans.
 *
 * Every synced row carries one of these (spec §4: "per-row provenance — where
 * it came from and when it was last seen"), which is why it is a tested
 * function and not five copies of a `Math.round` in five components.
 */

/**
 * `"just now" | "4 min ago" | "3 h ago" | "2026-08-19"`.
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
 * The mirror stores UTC, the launcher and the sync log quote UTC, and this
 * string is a provenance stamp that ends up in bug reports — one that means
 * the same thing on two machines is worth more here than one that agrees with
 * the reader's calendar at the midnight boundary. Anything recent enough for
 * that distinction to matter is still being rendered as a duration.
 */
export function ago(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "—";
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return "—";

  const minutes = Math.round((now.getTime() - then.getTime()) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;

  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;

  return then.toISOString().slice(0, 10);
}
