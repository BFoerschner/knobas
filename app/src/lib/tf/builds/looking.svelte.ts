/**
 * What knobas looks at again after a branch moved (ADR-0064 rule 2, #2585): the Rust side's bounded schedules, one per
 * repository, each with the sides it still looks for. Kept for the repository's Builds tab, which says *Looking for new
 * builds…* while its builds are looked for.
 *
 * Started once, by the first page that asks ({@link startLooking}): it reads what is looked at now (`looking_again`),
 * so a page opened during a schedule says so, then follows `looking:again` until the window goes. An event heard
 * before that first read answers wins over it: the event is newer.
 */
import { SvelteMap } from "svelte/reactivity";

import { EVENTS } from "../../ipc";
import { lookingAgain, type LookingAgain } from "../../ipc/sources";
import type { EchoListen } from "../queued.svelte";

/** Each repository looked at now, by its entity id. */
const held = new SvelteMap<string, LookingAgain>();

let started = false;

function hear(looking: LookingAgain): void {
  if (looking.code || looking.builds) held.set(looking.scope, looking);
  else held.delete(looking.scope);
}

/** Whether `scope`'s builds are looked for now. */
export function buildsLooked(scope: string): boolean {
  return held.get(scope)?.builds ?? false;
}

/** Listen for `looking:again`, once, on `listen`; and read what is looked at now with `read`. */
export function startLooking(listen: EchoListen, read: () => Promise<LookingAgain[]> = lookingAgain): void {
  if (started) return;
  started = true;
  const heard = new Set<string>();
  void listen(EVENTS.lookingAgain, ({ payload }) => {
    const looking = payload as LookingAgain;
    heard.add(looking.scope);
    hear(looking);
  }).catch(() => {});
  void read()
    .then((now) => {
      for (const looking of now) if (!heard.has(looking.scope)) hear(looking);
    })
    .catch(() => {});
}

/** Tests only. */
export function forgetLooking(): void {
  held.clear();
  started = false;
}
