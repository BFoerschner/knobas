/**
 * The two sentences the monitoring section says about its numbers (#443).
 *
 * Held here rather than asserted through the rendered section, because the
 * wording is the deliverable: a spinner reading `90` is not a promise, and
 * "deletes the older ones" is.
 */
import { expect, test } from "vitest";

import { retentionDaysSentence, thresholdSentence } from "./monitoring";

/**
 * Retention is the only control on this screen that destroys anything without
 * being pressed, so the sentence has to name the deleting.
 */
test("the retention sentence says what is deleted and not only what is kept", () => {
  expect(retentionDaysSentence(90)).toContain("90 days");
  expect(retentionDaysSentence(90)).toContain("deletes");
});

test("one day is not `1 days`", () => {
  expect(retentionDaysSentence(1)).toContain("one day");
  expect(retentionDaysSentence(1)).not.toContain("1 days");
});

/**
 * The half a spinner cannot say: a new threshold decides the next check and
 * does not rewrite what is already recorded. *Warn* is derived at sample time
 * and stored with the sample, so a bar redrawn under a new threshold would be
 * a picture of the setting rather than of what happened.
 */
test("the threshold sentence says warn is knobas' own and history is left alone", () => {
  const sentence = thresholdSentence(1500);
  expect(sentence).toContain("1500 ms");
  expect(sentence).toContain("warn");
  expect(sentence).toContain("already taken");
});
