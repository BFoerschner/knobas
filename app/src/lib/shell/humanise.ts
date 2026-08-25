/**
 * A machine key as a heading — `story_points` → *Story points*.
 *
 * One home rather than two, because both callers mean exactly the same thing
 * by it: `kinds.ts` turns an undeclared entity kind into a tile heading, and
 * `detail/payload.ts` turns an undeclared payload key into a field label.
 * Both are the §3a fallback — what knobas can say about a word no adapter has
 * declared anything about.
 *
 * Sentence case, not Title Case: these are field labels beside their values,
 * and *Affected services* reads as a label where *Affected Services* reads as
 * a proper noun.
 */
export function humanise(key: string): string {
  const words = key.replace(/[_-]+/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
