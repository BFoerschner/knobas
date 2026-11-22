/**
 * The Passwords page's one read, kept outside the page (ADR-0056), as the Docs tree keeps its tree
 * (`docs/tree-store.ts`): a return to the page draws from it in the first frame and reads again behind. Read again when
 * a run of a filler source ends and when a direct write lands. A read that changed nothing replaces nothing.
 */
import { SvelteSet } from "svelte/reactivity";

import { passwordsHeld, type Held } from "../../ipc/passwords";

/** The folds the reader opened in the tree, kept for the session. */
export const folds = new SvelteSet<string>();

let kept: Held | null = null;
let keptJson = "";

/** What the page drew last, or `null` before its first read. */
export function heldNow(): Held | null {
  return kept;
}

/** Read again; answers the new read, or the kept one when nothing changed. */
export async function readHeld(read: () => Promise<Held> = passwordsHeld): Promise<Held> {
  const next = await read();
  const json = JSON.stringify(next);
  if (kept !== null && json === keptJson) return kept;
  kept = next;
  keptJson = json;
  return next;
}

/** Tests only. */
export function forgetHeld(): void {
  kept = null;
  keptJson = "";
  folds.clear();
}
