/**
 * Waiting states (#2246, spec #2234 step 2): no page shows a blank pane
 * while it reads.
 *
 * - A read that **replaces** what is drawn keeps it, dimmed and not
 *   clickable, until the answer arrives.
 * - A **first** read, with nothing drawn yet, draws skeleton rows shaped like
 *   what is coming: table rows for a list, lines for a file.
 * - A read that **fails** draws its error in place of either.
 *
 * The page keeps its own state and says three things about it; this module
 * decides what to draw, and `TfWaiting.svelte` draws it.
 */

/** What a waiting surface draws. */
export type WaitPhase = "skeleton" | "dimmed" | "failed" | "shown";

export interface WaitFacts {
  /** Whether the surface holds content to draw: an answer, even an empty one. */
  drawn: boolean;
  /** Whether a read whose answer will replace that content is under way. */
  reading: boolean;
  /** Whether the latest read failed; its error is drawn instead. */
  failed: boolean;
}

export function waitPhase({ drawn, reading, failed }: WaitFacts): WaitPhase {
  if (failed) return "failed";
  if (!drawn) return "skeleton";
  return reading ? "dimmed" : "shown";
}
