/**
 * What knobas looks at again after a branch moved or a write landed (ADR-0064 rule 2, #2585, #2595): the Rust side's
 * bounded schedules, one per scope, each with the sides it still looks for. Kept for the repository's Builds tab, which
 * says *Looking for new builds…* while its builds are looked for, and for an item's page, which says *Looking for
 * changes…* while the item is read by id (#2615).
 *
 * Started once for the window, by the app at start (`App.svelte`) or by the first page that asks
 * ({@link startLooking}): it reads what is looked at now (`looking_again`), so a page opened during a schedule says so,
 * then follows `looking:again` until it is stopped. An event heard before that first read answers wins over it: the
 * event is newer.
 */
import { SvelteMap } from "svelte/reactivity";

import { EVENTS } from "../../ipc";
import { lookingAgain, type LookingAgain } from "../../ipc/sources";
import type { EchoListen } from "../queued.svelte";

/** Each scope looked at now, by its entity id: a repository, or the item a write made or touched. */
const held = new SvelteMap<string, LookingAgain>();

let started = false;

function hear(looking: LookingAgain): void {
  if (looking.code || looking.builds || looking.items.length > 0) held.set(looking.scope, looking);
  else held.delete(looking.scope);
}

/** Whether `scope`'s builds are looked for now. */
export function buildsLooked(scope: string): boolean {
  return held.get(scope)?.builds ?? false;
}

/**
 * Whether `entityId` is read by id now, for what its source does after a write: in its own scope (the written item and
 * its linked tickets) or in a repository's (a branch write's pull request). A scope itself is not an item read by id.
 */
export function itemLooked(entityId: string): boolean {
  for (const looking of held.values()) if (looking.items.includes(entityId)) return true;
  return false;
}

/**
 * Listen for `looking:again`, once, on `listen`; and read what is looked at now with `read`. Answers what stops the
 * listening, which the app calls when its window goes; a second start answers a stop that does nothing.
 */
export function startLooking(listen: EchoListen, read: () => Promise<LookingAgain[]> = lookingAgain): () => void {
  if (started) return () => {};
  started = true;
  const heard = new Set<string>();
  let stopped = false;
  let off: (() => void) | null = null;
  void listen(EVENTS.lookingAgain, ({ payload }) => {
    const looking = payload as LookingAgain;
    heard.add(looking.scope);
    hear(looking);
  })
    .then((unlisten) => {
      if (stopped) unlisten();
      else off = unlisten;
    })
    .catch(() => {});
  void read()
    .then((now) => {
      for (const looking of now) if (!stopped && !heard.has(looking.scope)) hear(looking);
    })
    .catch(() => {});
  return () => {
    stopped = true;
    started = false;
    off?.();
  };
}

/** Tests only. */
export function forgetLooking(): void {
  held.clear();
  started = false;
}
