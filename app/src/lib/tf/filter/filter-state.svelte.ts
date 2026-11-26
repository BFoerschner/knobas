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
const folds = new Map<string, { open: boolean | null }>();
const values = new Map<string, { value: string | null }>();

/** The view's filter, the same object every time it is asked for this session. */
export function sessionFilter(id: string): FilterState {
  const held = kept.get(id);
  if (held) return held;
  const fresh = $state(emptyFilter());
  kept.set(id, fresh);
  return fresh;
}

/**
 * A fold a view draws beside its filter (Monitors' hidden monitors), as the
 * reader last left it this session: `open` is `null` until they touch it, so
 * the view decides the default.
 */
export function sessionFold(id: string): { open: boolean | null } {
  const held = folds.get(id);
  if (held) return held;
  const fresh = $state<{ open: boolean | null }>({ open: null });
  folds.set(id, fresh);
  return fresh;
}

/** A value a view keeps for the session (the pipeline `#/builds` reopens), `null` until it says one. */
export function sessionValue(id: string): { value: string | null } {
  const held = values.get(id);
  if (held) return held;
  const fresh = $state<{ value: string | null }>({ value: null });
  values.set(id, fresh);
  return fresh;
}

/** Forget every view's filter and fold: a new session, which is what each test is. */
export function forgetFilters(): void {
  kept.clear();
  folds.clear();
  values.clear();
}
