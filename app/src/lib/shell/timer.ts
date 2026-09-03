/**
 * What the timer *reads* as, and what may be a target — the pure half of the
 * shell's timer (issue #278).
 *
 * Separate from `timer.svelte.ts` so the two rules with real content — the
 * elapsed reading and the refusal — can be driven without a bridge, a clock or
 * a mounted component.
 */
import type { EntityRow } from "../ipc/entity";
import type { TimerTarget } from "../ipc/time";

/**
 * The one word this module refuses on — both the entity namespace stored
 * contexts live in and the kind their `knobas.entity` row carries, which are
 * the same word because `knobas_core::context::insert` writes both from it.
 *
 * Pinned to the backend's own copy, `knobas_app::time::CONTEXT_NAMESPACE`, by
 * `the_shells_context_namespace_is_the_one_the_backend_refuses` in
 * `crates/knobas-app/src/commands/time.rs`, which reads this file. Without
 * that, a rename in Rust would leave this filtering for a spelling nothing
 * produces any more — green, and refusing nothing.
 *
 * A context is a real row in `knobas.entity`, so it turns up in recents and in
 * search results like anything else. That is exactly why {@link canBeTarget}
 * has to be a rule and not an absence.
 */
const CONTEXT_NAMESPACE = "ctx";

/**
 * Whether this candidate may be a timer target.
 *
 * **The rule, and the one thing it is for** (`CONTEXT.md`, *timer target*;
 * spec #272 story 15): a stored context is never a target. A context is a
 * *set*, and time on a set has nowhere to go — the ad-hoc label is what covers
 * "worked across the SEPA context". The picker draws its candidates from the
 * launcher's recents, which carry contexts, so a picker that did not refuse
 * would offer one.
 *
 * Both spellings are checked and either alone is enough. The `kind` is what a
 * row carries and the namespace is what the id itself says; a caller that has
 * one and not the other still gets the right answer, and a context reached by
 * a route that lost its kind is still refused.
 *
 * Everything else is legal, including a kind knobas has never heard of. §3a is
 * the constraint: an adapter's new kind is browsable on day one, and a timer
 * that only ran on a table of known kinds would be the table §3a forbids. That
 * is why this refuses one word rather than allowing a list.
 */
export function canBeTarget(candidate: { entityId: string; kind?: string }): boolean {
  if (candidate.kind?.toLowerCase() === CONTEXT_NAMESPACE) return false;
  return namespaceOf(candidate.entityId) !== null &&
    namespaceOf(candidate.entityId) !== CONTEXT_NAMESPACE;
}

/**
 * The namespace half of `"<namespace>:<key>"`, or `null` when the string is
 * not an entity id.
 *
 * The first `:` only, matching `knobas_core::entity::EntityRef::parse`: a key
 * is free to contain further colons (`confluence:ENG:SEPA design`).
 */
function namespaceOf(entityId: string): string | null {
  const at = entityId.indexOf(":");
  if (at <= 0) return null;
  if (entityId.slice(at + 1).trim() === "") return null;
  return entityId.slice(0, at).toLowerCase();
}

/**
 * What the strip calls the thing the clock is on.
 *
 * An entity reads as its **key** — the half after the first `:` — which is the
 * same half `Detail.svelte`'s header and `App.svelte`'s `openEntity` use, so
 * the strip and the slide-over say the same word about the same ticket. A
 * label reads as itself: it is already what a person typed.
 */
export function targetReading(target: TimerTarget): string {
  return target.kind === "label" ? target.label : target.entity_id.slice(target.entity_id.indexOf(":") + 1);
}

/**
 * How long the timer has been running — `"0:07"`, `"45:12"`, `"3:20:07"`.
 *
 * The shape the sync countdown uses (`sources/diagnostics.ts`), with one
 * difference: **the seconds keep ticking past an hour.** A countdown to a sync
 * rounds them away because nobody watches it; this is the value story 16 asks
 * to "read as live", and a reading that stops moving after sixty minutes stops
 * saying the clock is running.
 *
 * **A start in the future reads `0:00`**, the decision `time.ts`'s `ago` makes
 * for the same reason: `started_at` is the database's clock and `now` is the
 * webview's, the two disagree by fractions of a second routinely, and
 * `"-0:01"` is a fact about two clocks that looks like a bug in knobas.
 */
export function elapsedReading(startedAt: string, now: Date = new Date()): string {
  const from = new Date(startedAt).getTime();
  if (Number.isNaN(from)) return "0:00";

  const seconds = Math.max(0, Math.floor((now.getTime() - from) / 1000));
  const ss = String(seconds % 60).padStart(2, "0");
  if (seconds < 3600) return `${Math.floor(seconds / 60)}:${ss}`;
  const mm = String(Math.floor((seconds % 3600) / 60)).padStart(2, "0");
  return `${Math.floor(seconds / 3600)}:${mm}:${ss}`;
}

/** One thing the picker can offer, whatever list it came from. */
export interface TargetCandidate {
  entityId: string;
  title: string;
  kind: string;
}

/** A recent entity, as a candidate. */
export function candidateOf(row: EntityRow): TargetCandidate {
  return { entityId: row.entity_id, title: row.title, kind: row.kind };
}

/** The candidates that may actually be started on, in the order given. */
export function legalCandidates(candidates: TargetCandidate[]): TargetCandidate[] {
  return candidates.filter(canBeTarget);
}
