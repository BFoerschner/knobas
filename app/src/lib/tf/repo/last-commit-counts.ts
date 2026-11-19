/**
 * The count each place's commits were last read with (the ux21 pass): a repository's commits answered from the mirror
 * carry its tip and no count, the source's read behind them bringing it, so the last-commit strip draws the count it
 * learned before with such an answer rather than *History* first.
 */
const counts = new Map<string, number>();

/** Keeps `count` for `at`, and gives it back. */
export function keepCount(at: string, count: number): number {
  counts.set(at, count);
  return count;
}

/** The count `at` was last read with, `null` for none. */
export function keptCount(at: string): number | null {
  return counts.get(at) ?? null;
}

/** Every kept count forgotten: a test's fresh window. */
export function forgetCounts(): void {
  counts.clear();
}
