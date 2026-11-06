/**
 * What a reader typed on a source's page and has not saved (F-617): per source, for
 * the session, so leaving the page by `Esc`, the crumb or the rail loses nothing. Hash history can't be held for a
 * *Discard?* prompt without a guard in the shell, so the page keeps the edit and says *Not saved* beside *Save*.
 *
 * Holds the form's words, never a credential: those go straight to the keychain from their own dialog. A draft can sit
 * unsaved until restart, and that is the trade. The Sources list says which sources hold one.
 */
import { SvelteMap } from "svelte/reactivity";

export interface SourceDraft {
  /** The address and user as typed, while either differs from what is stored. */
  connection?: { address: string; user: string } | undefined;
  /** The config fields the reader changed, as the form holds them. */
  config?: Record<string, unknown> | undefined;
}

export interface SourceDrafts {
  get(id: string): SourceDraft | undefined;
  has(id: string): boolean;
  /** Set one half of a source's draft, or `null` to drop it. */
  put<K extends keyof SourceDraft>(id: string, part: K, value: NonNullable<SourceDraft[K]> | null): void;
  forget(id: string): void;
  /** Tests only: forget every draft. */
  reset(): void;
}

export function createSourceDrafts(): SourceDrafts {
  const drafts = new SvelteMap<string, SourceDraft>();
  return {
    get: (id) => drafts.get(id),
    has: (id) => drafts.has(id),
    put(id, part, value) {
      const next: SourceDraft = { ...drafts.get(id) };
      if (value === null) delete next[part];
      else next[part] = value;
      if (next.connection === undefined && next.config === undefined) drafts.delete(id);
      else drafts.set(id, next);
    },
    forget: (id) => void drafts.delete(id),
    reset: () => drafts.clear(),
  };
}

/** The window's one store. */
export const sourceDrafts = createSourceDrafts();
