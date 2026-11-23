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

/** What the page drew last, or `null` before its first read. */
export function heldNow(): Held | null {
  return kept;
}

/**
 * Read again; answers the new read, or the kept one when nothing changed. The first read compares nothing: it is on
 * the first open's path, and a text of 20,000 rows made only to compare a later read cost 5 ms at 1x there (the ux30
 * end gate, 2026-11-23). A later read compares value by value, stopping at the first difference.
 */
export async function readHeld(read: () => Promise<Held> = passwordsHeld): Promise<Held> {
  const next = await read();
  if (kept !== null && same(kept, next)) return kept;
  kept = next;
  return next;
}

/** Whether two values read from JSON are equal, deeply. */
export function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!same(a[i], b[i])) return false;
    return true;
  }
  if (Array.isArray(b)) return false;
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  for (const k of ka) if (!Object.hasOwn(b, k) || !same((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k])) return false;
  return true;
}

/** Tests only. */
export function forgetHeld(): void {
  kept = null;
  folds.clear();
}
