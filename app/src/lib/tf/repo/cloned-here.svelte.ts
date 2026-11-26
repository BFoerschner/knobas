/**
 * Which repositories have a checkout on this computer, as the pages that read checkouts last heard (ux26): Code's list
 * (`entity_checkouts`) and a repository's page (`entity_checkout`). The right-click menu offers a commit's *Compare with
 * working tree* only where it is known here, so an item never comes in after the menu opened. Nothing is read for it.
 */
import { SvelteMap } from "svelte/reactivity";

const known = new SvelteMap<string, boolean>();

/** What a read of `repoId`'s checkout answered: a path, or none. */
export function heardCheckout(repoId: string, path: string | null): void {
  if (known.get(repoId) !== (path !== null)) known.set(repoId, path !== null);
}

/** Whether `repoId` is known to be cloned here. */
export function clonedHere(repoId: string): boolean {
  return known.get(repoId) === true;
}

/** Tests only: nothing heard. */
export function forgetCheckouts(): void {
  known.clear();
}
