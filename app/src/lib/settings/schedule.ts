/**
 * How the backup section talks about its own schedule.
 *
 * Functions rather than sentences inlined in the markup, because the wording
 * is the part of this feature that is easiest to get quietly wrong and the
 * only part a test can hold still. `schedule.test.ts` is where the rule is
 * written down.
 *
 * ## "Nightly" is a boundary, not a moment
 *
 * `knobas_app::backup::policy` decides a backup is due when the last one is
 * *older than the most recent occurrence* of the scheduled local time — not
 * by firing a timer at that time. The difference is the whole feature on a
 * laptop: a machine asleep at 03:00 is the ordinary case, and an alarm-clock
 * schedule simply does not happen on those nights. Ratified 2026-08-28 with
 * the defaults themselves (issue #38), and issue #69 makes the wording part
 * of the acceptance: the UI "should not promise a fixed time".
 *
 * So every sentence here says *after* the hour and says what a missed one
 * does. Editing them to read "every night at 03:00" would be shorter, and
 * would be a promise the engine does not make.
 */
import type { BackupSchedule } from "../ipc/backup";

/** The scheduled local time, zero-padded: `03:00`. */
export function clockTime(schedule: BackupSchedule): string {
  const pad = (value: number) => String(Math.trunc(value)).padStart(2, "0");
  return `${pad(schedule.hour)}:${pad(schedule.minute)}`;
}

/**
 * What the schedule actually does, in one sentence a reader can act on.
 *
 * The `enabled: false` branch names *Export now* deliberately: turning the
 * nightly run off is the moment a reader most needs to know that the manual
 * export is untouched by it (`backup::export_now` never consults the
 * schedule), and the button is a few pixels away from the sentence.
 */
export function nightlySentence(schedule: BackupSchedule): string {
  if (!schedule.enabled) {
    return "The nightly export is off. Export now still takes one whenever you ask.";
  }
  return (
    `knobas exports after ${clockTime(schedule)}, once the last export is older than that — ` +
    "so a machine that was asleep or shut down when that time passed backs up the next time " +
    "knobas runs, rather than skipping the night."
  );
}

/**
 * What retention deletes, and how much history survives it.
 *
 * A count on its own ("keep 7") does not say that the eighth is *deleted*, and
 * this is the only control on the screen that destroys anything without being
 * pressed.
 *
 * "its own" is load-bearing: `policy::expired` filters by `is_archive_name`
 * before it deletes anything, so a file the user put in the backup directory
 * themselves is never a candidate. "Anything older" would have promised a
 * sweep of the whole directory.
 */
export function retentionSentence(keep: number): string {
  const archives = keep === 1 ? "1 archive" : `${keep} archives`;
  return `knobas keeps its ${archives} and deletes its older ones after each export.`;
}
