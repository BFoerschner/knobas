/**
 * What a reader typed on a source's page and has not saved (F-617): per source, for
 * the session, so leaving the page by `Esc`, the crumb or the rail loses nothing. Hash history can't be held for a
 * *Discard?* prompt without a guard in the shell, so the page keeps the edit and says *Not saved* beside *Save*.
 *
 * Holds the form's words, never a credential: those go straight to the keychain from their own dialog. A draft can sit
 * unsaved until restart, and that is the trade. The Sources list says which sources hold one.
 */
import { createDrafts } from "../drafts.svelte";

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
  const drafts = createDrafts<SourceDraft>();
  return {
    get: (id) => drafts.held(id),
    has: (id) => drafts.has(id),
    put(id, part, value) {
      const next: SourceDraft = { ...drafts.held(id) };
      if (value === null) delete next[part];
      else next[part] = value;
      drafts.put(id, next.connection === undefined && next.config === undefined ? null : next);
    },
    forget: (id) => drafts.forget(id),
    reset: () => drafts.reset(),
  };
}

/** The window's one store. */
export const sourceDrafts = createSourceDrafts();

/**
 * What `Esc` or × leaves of *Add a source* (F-647): which adapter, the address and the name, for the session. *Cancel* and a
 * source added end it. **Nothing else, and never a secret:** the token, the password and the account stay in the dialog and
 * go with it, and so does a test's answer, so a reopened dialog asks for the token and tests again.
 */
export interface NewSourceDraft {
  /** The adapter's descriptor id. */
  adapter: string;
  address: string;
  name: string;
}

const newSource = createDrafts<NewSourceDraft>();
const NEW_SOURCE = "new-source";

/**
 * An address as it may be held. An address can carry a sign-in, so a query and a fragment go, and one with an `@` in it or a
 * password half typed (`me:hun`) is not held at all. `host:8080` is a port and stays.
 */
export function addressToHold(typed: string): string {
  const bare = typed.trim().split(/[?#]/, 1)[0] ?? "";
  const authority = bare.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").split("/", 1)[0] ?? "";
  return bare.includes("@") || /:\D/.test(authority) ? "" : bare;
}

export const newSourceDraft = {
  held: (): NewSourceDraft | undefined => newSource.held(NEW_SOURCE),
  /** Hold what the form says now. The address goes through {@link addressToHold}, and nothing else can be passed. */
  hold(adapter: string, address: string, name: string): void {
    newSource.put(NEW_SOURCE, { adapter, address: addressToHold(address), name });
  },
  forget: (): void => newSource.forget(NEW_SOURCE),
};
