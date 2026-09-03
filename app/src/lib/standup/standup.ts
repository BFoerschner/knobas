/**
 * The days a digest is asked for, and how one of its lines reads — the pure
 * half of the standup view (issue #288).
 *
 * Separate from `StandupView.svelte` for the reason `lib/time/week.ts` is
 * separate from `WeekTimesheet.svelte`: the rule with real content here is
 * arithmetic over local midnights, and it is wrong twice a year in a way no
 * screenshot shows.
 *
 * ## The days are the reader's, and this side is the only thing that knows it
 *
 * `standup_digest` takes {@link DayWindow}s, each carrying a date and the two
 * instants it spans, because the machine's timezone is a fact only the webview
 * holds — the rule `lib/time/day.ts` records in full. {@link digestWindows}
 * asks `Date` for eight consecutive local midnights rather than adding 24
 * hours seven times, so a week containing a daylight-saving change is still
 * eight correct windows.
 *
 * ## What this side does *not* decide
 *
 * How far back *yesterday* may look. {@link LOOKBACK_DAYS} is what this side
 * sends and the backend consults at most its own seven however many arrive —
 * `knobas_app::standup::LOOKBACK_DAYS` is where the rule lives, because a rule
 * a caller could widen is not a rule.
 */
import type { DayWindow } from "../ipc/time";

import { dayBounds, shiftDay } from "../time/day";

/**
 * How many earlier days the view sends.
 *
 * Seven, matching the backend's cap, so an ordinary read never asks it to
 * ignore a window. It is a *courtesy* to the backend and not the rule itself:
 * sending six would narrow the digest, and sending eight would not widen it.
 */
export const LOOKBACK_DAYS = 7;

/** The day being asked about, and the seven before it, oldest first. */
export function digestWindows(key: string): { today: DayWindow; earlier: DayWindow[] } {
  return {
    today: windowOf(key),
    earlier: Array.from({ length: LOOKBACK_DAYS }, (_, index) =>
      windowOf(shiftDay(key, index - LOOKBACK_DAYS)),
    ),
  };
}

/** One date as the backend asks for it: the date, and the instants it spans. */
function windowOf(day: string): DayWindow {
  const { from, to } = dayBounds(day);
  return { day, from: from.toISOString(), to: to.toISOString() };
}
