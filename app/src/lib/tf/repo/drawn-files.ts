/**
 * The files Files drew, by the entity they were read at and the listing's section that opens them (the inner-pages
 * pass, 2026-11-12): a file a deep link's guess read by its path, then named by its listing at a commit, was read from
 * the forge a second time on every return, and again once the page was mounted afresh. Only a section that names
 * where the file stands is kept (`TfRepoFiles`' `keepFile`), so what is kept cannot change under it; a refresh forgets
 * the entity's files. Kept for the window's life, the newest {@link KEPT} first.
 */
import type { ViewSection } from "../../ipc/entity";

export const KEPT = 64;

const kept = new Map<string, ViewSection | undefined>();
const keyOf = (at: string, section: string) => `${at}\u0000${section}`;

export function drawnFile(at: string, section: string): ViewSection | undefined {
  const key = keyOf(at, section);
  const read = kept.get(key);
  if (read === undefined) return undefined;
  kept.delete(key);
  kept.set(key, read);
  return read;
}

export function hasDrawnFile(at: string, section: string): boolean {
  return kept.has(keyOf(at, section));
}

export function keepDrawnFile(at: string, section: string, read: ViewSection | undefined): void {
  if (read === undefined) return;
  const key = keyOf(at, section);
  kept.delete(key);
  kept.set(key, read);
  while (kept.size > KEPT) kept.delete(kept.keys().next().value!);
}

/** Every file kept for `at`: the page was refreshed, and reads them again. */
export function forgetDrawnFiles(at: string): void {
  const prefix = `${at}\u0000`;
  for (const key of [...kept.keys()]) if (key.startsWith(prefix)) kept.delete(key);
}

/** Everything: a test's fresh window. */
export function forgetAllDrawnFiles(): void {
  kept.clear();
}
