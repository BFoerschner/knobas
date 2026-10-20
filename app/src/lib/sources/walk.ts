/**
 * A run's outer walk (#1379), as the two surfaces that draw one read it: the
 * first-run wizard and a source's settings page (#1384).
 */
import type { SyncProgress } from "../ipc/sources";

/** The latest reading: *120 of 300 repositories*. */
export interface Walk {
  walked: number;
  /** Absent when the adapter does not know the total. */
  of: number | undefined;
  unit: string;
}

/**
 * The reading after `message`. A message without one keeps the last: only the
 * engine's progress decorator sends one, and the scheduler's own messages
 * (the start, the ending) carry none, so a failure still shows how far the
 * walk got.
 */
export function walkAfter(last: Walk | null, message: SyncProgress): Walk | null {
  if (message.walked === undefined) return last;
  return { walked: message.walked, of: message.of, unit: message.unit ?? "" };
}

/** The bar's maximum: the walk's total, when there is one to draw. */
export function walkMax(walk: Walk | null): number | undefined {
  return walk?.of !== undefined && walk.of > 0 ? walk.of : undefined;
}
