/**
 * The hints *Not now* put away (#870, R-7.10), for the rest of the session:
 * module state, so leaving `#/sets` and coming back does not offer the same
 * item to the same set again. Nothing is stored; a restart offers it again.
 */
import { SvelteSet } from "svelte/reactivity";

import type { LinkSetHint } from "../../ipc/entity";

const putAway = new SvelteSet<string>();

const keyOf = (hint: Pick<LinkSetHint, "set_id" | "member">) => `${hint.set_id}\u0000${hint.member.entity_id}`;

/** *Not now* on one hint. */
export function notNow(hint: Pick<LinkSetHint, "set_id" | "member">): void {
  putAway.add(keyOf(hint));
}

/** The first hint not put away, or `null`. */
export function firstHint(hints: readonly LinkSetHint[]): LinkSetHint | null {
  return hints.find((hint) => !putAway.has(keyOf(hint))) ?? null;
}

/** For tests: every hint offered again. */
export function forgetHints(): void {
  putAway.clear();
}
