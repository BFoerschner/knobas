/**
 * The sentences the backup section says about its own schedule.
 *
 * These are tested as a unit and not as markup because the *wording* is the
 * deliverable. Issue #69 spells the rule out: "'Nightly' is a **boundary**,
 * not a moment — a laptop asleep at 03:00 backs up on wake; the UI wording
 * should not promise a fixed time." A settings screen that reads "backs up
 * every night at 03:00" is a screen that will be wrong on most nights of a
 * laptop's life, and the person reading it has no way to find that out except
 * by noticing an archive that is not there.
 *
 * The Rust these sentences describe is `knobas_app::backup::policy`: a backup
 * is due when the last one is older than the most recent occurrence of the
 * scheduled local time.
 */
import { expect, test } from "vitest";

import { clockTime, nightlySentence, retentionSentence } from "./schedule";

const NIGHTLY = { enabled: true, hour: 3, minute: 0, keep: 7 };

test("the clock reads as a zero-padded local time", () => {
  expect(clockTime({ ...NIGHTLY })).toBe("03:00");
  expect(clockTime({ ...NIGHTLY, hour: 22, minute: 30 })).toBe("22:30");
  expect(clockTime({ ...NIGHTLY, hour: 0, minute: 5 })).toBe("00:05");
});

/**
 * The whole point. `after 03:00` is the boundary reading; `at 03:00` is the
 * alarm-clock reading the engine deliberately does not implement.
 */
test("the nightly sentence says after the hour, never at it", () => {
  const sentence = nightlySentence(NIGHTLY);
  expect(sentence).toContain("after 03:00");
  expect(sentence).not.toContain("at 03:00");
});

/**
 * Naming the miss is the half that makes the boundary comprehensible: without
 * it "after 03:00" reads as pedantry rather than as the reason an archive is
 * stamped 09:14.
 */
test("the nightly sentence says what happens when the machine misses the hour", () => {
  const sentence = nightlySentence(NIGHTLY);
  expect(sentence).toMatch(/asleep|shut down|off/);
  expect(sentence).toMatch(/\bnext\b/);
});

/** The hour is read from the schedule, not baked into the sentence. */
test("the nightly sentence follows the schedule it is given", () => {
  const sentence = nightlySentence({ ...NIGHTLY, hour: 22, minute: 30 });
  expect(sentence).toContain("after 22:30");
  expect(sentence).not.toContain("03:00");
});

/**
 * *Export now* works whatever the schedule says (`backup::export_now` never
 * consults it), and a reader who has just switched the nightly run off is
 * exactly the reader who needs telling.
 */
test("a schedule that is off says so, and says the manual export still works", () => {
  const sentence = nightlySentence({ ...NIGHTLY, enabled: false });
  expect(sentence).toMatch(/off|not running|no automatic/i);
  expect(sentence).toContain("Export now");
  expect(sentence).not.toContain("03:00");
});

/** Retention is a promise about deletion, so it names the number and the loss. */
test("the retention sentence names how many are kept and that the rest go", () => {
  expect(retentionSentence(7)).toContain("7");
  expect(retentionSentence(7)).toMatch(/delet/i);
  expect(retentionSentence(1)).toContain("1 archive");
  expect(retentionSentence(7)).toContain("7 archives");
});

/**
 * ...and deletes nothing else. `policy::expired` filters by `is_archive_name`
 * before it names a candidate, and its own test "spares what is not ours", so
 * a file the reader dropped in the backup directory is safe. The shorter
 * "deletes anything older" would have said knobas sweeps the directory, which
 * is a claim about their files rather than ours.
 */
test("the retention sentence claims only knobas's own archives", () => {
  expect(retentionSentence(7)).not.toMatch(/anything older/i);
  expect(retentionSentence(7)).toMatch(/its \d+ archives/);
  expect(retentionSentence(7)).toMatch(/its older ones/);
});
