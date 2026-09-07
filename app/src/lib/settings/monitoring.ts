/**
 * How the monitoring section talks about its own two numbers.
 *
 * Functions rather than sentences inlined in the markup, the rule
 * `schedule.ts` records: the wording is the part of this feature easiest to
 * get quietly wrong and the only part a test can hold still.
 *
 * Both sentences exist because a bare number lies by omission. "90" does not
 * say that the ninety-first day is *deleted*, and retention is the only
 * control on this screen that destroys anything without being pressed.
 * "1500" does not say that *warn* is knobas' own reading rather than
 * something the monitoring source reports, nor that changing it leaves
 * everything already recorded alone.
 */

/** What retention keeps, and what it takes away. */
export function retentionDaysSentence(days: number): string {
  const span = days === 1 ? "one day" : `${days} days`;
  return `knobas keeps ${span} of readings and deletes the older ones each night.`;
}

/**
 * What the threshold does, and — the half a spinner cannot say — what it does
 * not do to what is already recorded.
 */
export function thresholdSentence(ms: number): string {
  return (
    `A monitor that is up but answers slower than ${ms} ms is recorded as ` +
    `warn. Changing this decides the next check; readings already taken stay ` +
    `as they were recorded.`
  );
}
