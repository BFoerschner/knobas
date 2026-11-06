/**
 * **Typed words outlive what holds them** (design system, the key table): `Esc` on an editor closes it and keeps the
 * draft, and leaving the page keeps it too. Each draft has a key of its own, so words typed on one thing never land on
 * another: a page's comment under `comment:<page>`, a section's under `section:<page>:<section>`.
 *
 * For the session: a restart forgets them, and that is the trade. *Cancel* is the explicit discard, and sending is the
 * other way a draft ends, so both {@link Drafts.forget} it.
 *
 *     let words = $derived(drafts.get(key));                    // read it
 *     <textarea value={words} oninput={(e) => drafts.put(key, e.currentTarget.value)}>
 */
import { SvelteMap } from "svelte/reactivity";

export interface Drafts<T> {
  /** What is held under `key`, or `undefined`. */
  held(key: string): T | undefined;
  has(key: string): boolean;
  /** Hold `value`, or drop the draft when it is `null`. */
  put(key: string, value: T | null): void;
  forget(key: string): void;
  /** Tests only: forget every draft. */
  reset(): void;
}

export function createDrafts<T>(): Drafts<T> {
  const held = new SvelteMap<string, T>();
  return {
    held: (key) => held.get(key),
    has: (key) => held.has(key),
    put(key, value) {
      if (value === null) held.delete(key);
      else held.set(key, value);
    },
    forget: (key) => void held.delete(key),
    reset: () => held.clear(),
  };
}

const typed = createDrafts<string>();

/** The window's typed words. Blank words are no draft, so a box emptied by hand holds nothing back. */
export const drafts = {
  /** The words under `key`, or an empty string. */
  get: (key: string): string => typed.held(key) ?? "",
  has: (key: string): boolean => typed.has(key),
  put: (key: string, words: string): void => typed.put(key, words.trim() === "" ? null : words),
  forget: (key: string): void => typed.forget(key),
  reset: (): void => typed.reset(),
};
