/**
 * Filter state kept per view for the session (R-2.6, #810).
 *
 * A module map from a {@link FilterConfig.id} to that view's reactive
 * {@link FilterState}: leaving a view and coming back finds its words, chips
 * and dropdowns as they were. Nothing is written anywhere; a reload is a new
 * session.
 */
import { emptyFilter, type FilterState } from "./filter";

const kept = new Map<string, FilterState>();

/** The view's filter, the same object every time it is asked for this session. */
export function sessionFilter(id: string): FilterState {
  const held = kept.get(id);
  if (held) return held;
  const fresh = $state(emptyFilter());
  kept.set(id, fresh);
  return fresh;
}

/** Forget every view's filter: a new session, which is what each test is. */
export function forgetFilters(): void {
  kept.clear();
}
