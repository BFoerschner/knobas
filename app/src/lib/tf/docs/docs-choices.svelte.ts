/**
 * **Where Docs opens, and the spaces it hides** (PR #2479, 2026-11-17,
 * `docs/decisions/2026-11-17-where-docs-opens-and-the-spaces-it-hides.md`).
 *
 * Björn: "add the ability to change in the docs area the default open page",
 * and "hide areas that I'm not using? I still want it to be searchable, just
 * not visible".
 *
 * - **Docs' home** is a space's own page or one page. Docs opens there when
 *   nothing in it is chosen: the nav entry's first press (after a restart
 *   too), the entry pressed inside Docs, and the launcher's *Docs* row. A
 *   place the reader stood on in Docs still comes back first (*Section's
 *   place*). With none, Docs opens on `#/docs`, as before. A home the mirror
 *   no longer holds opens `#/docs` (`TfDocs.svelte`).
 * - **A hidden space** leaves Docs' browsing: the rail's tree, the home's
 *   tiles and the Space picker's list, for a fold at their end. Search still
 *   finds its pages, a link into it still opens, and the sync keeps it.
 *
 * Both are `knobas.setting` rows (`crate::docs::choices`), read once the
 * database answers (`TfShell.svelte`), so a restart keeps them and the nav's
 * first press knows where to go. Every change is drawn at once and written
 * behind it; a write that fails puts the choice back and says so.
 */
import { SvelteSet } from "svelte/reactivity";

import {
  docsChoices as realDocsChoices,
  setDocsHome as realSetDocsHome,
  setDocsSpaceHidden as realSetDocsSpaceHidden,
  type DocsChoices,
  type DocsHome,
} from "../../ipc/entity";
import { ipcErrorMessage } from "../../ipc";
import { push } from "../../shell/toasts.svelte";
import { pageHash } from "./docs";

/** What the choices are read and written by. */
export interface DocsChoicesPorts {
  read(): Promise<DocsChoices>;
  setHome(home: DocsHome | null): Promise<DocsChoices>;
  setHidden(space: string, hidden: boolean): Promise<DocsChoices>;
}

/** The bridge's, each named only when called, so a suite that mocks the bridge without them still loads this. */
const REAL: DocsChoicesPorts = {
  read: async () => realDocsChoices(),
  setHome: async (home) => realSetDocsHome(home),
  setHidden: async (space, hidden) => realSetDocsSpaceHidden(space, hidden),
};

/** Docs' own page, which opens no page: where Docs opens with no home chosen. */
export const DOCS_HOME = "#/docs";

/**
 * The address of a space by its `spaceId` (`<source>/<key>`), always naming its source (`#/docs/space/<source>:<key>`),
 * which `spaceAt` reads whether or not another source holds the key.
 */
export function spaceIdHash(id: string): string {
  const at = id.indexOf("/");
  return `#/docs/space/${encodeURIComponent(`${id.slice(0, at)}:${id.slice(at + 1)}`)}`;
}

/** The address `home` opens. */
export function homeHash(home: DocsHome | null): string {
  if (home === null) return DOCS_HOME;
  return home.kind === "space" ? spaceIdHash(home.id) : pageHash(home.id);
}

export class Choices {
  home = $state<DocsHome | null>(null);
  readonly hidden = new SvelteSet<string>();
  ports: DocsChoicesPorts = REAL;
  /** Bumped by every write: an answer to an older one than the last is not held, so a quick second press stands. */
  #asked = 0;

  /** Hold what the database answered. */
  hold(choices: DocsChoices): void {
    this.home = choices.home;
    for (const id of [...this.hidden]) if (!choices.hidden.includes(id)) this.hidden.delete(id);
    for (const id of choices.hidden) this.hidden.add(id);
  }

  /** Read them, once the database answers; a read a write overtook holds nothing. */
  async load(): Promise<void> {
    const mine = this.#asked;
    const read = await this.ports.read();
    if (mine === this.#asked) this.hold(read);
  }

  /** Where Docs opens: the home's address, else Docs' own page. */
  get start(): string {
    return homeHash(this.home);
  }

  /** Whether `kind` `id` is Docs' home. */
  isHome(kind: DocsHome["kind"], id: string): boolean {
    return this.home?.kind === kind && this.home.id === id;
  }

  /** Make `home` Docs' home, `null` for Docs' own page, at once; written behind, and put back if the write fails. */
  async setHome(home: DocsHome | null, undo = true): Promise<void> {
    const before = this.home;
    const mine = ++this.#asked;
    this.home = home;
    if (undo) {
      push({
        text: home === null ? "Docs opens on its home again." : `Docs opens on ${home.title} now.`,
        action: { label: "Undo", run: () => void this.setHome(before, false) },
      });
    }
    try {
      const answer = await this.ports.setHome(home);
      if (mine === this.#asked) this.hold(answer);
    } catch (cause) {
      if (mine === this.#asked) this.home = before;
      push({ text: `Could not change where Docs opens: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** Hide space `id` (`<source>/<key>`, called `name`) from Docs' browsing, or show it again, at once. */
  async setHidden(id: string, name: string, hidden: boolean, undo = true): Promise<void> {
    const mine = ++this.#asked;
    if (hidden) this.hidden.add(id);
    else this.hidden.delete(id);
    if (undo) {
      push({
        text: hidden ? `Hid ${name} from Docs. Search still finds its pages.` : `${name} is back in Docs.`,
        action: { label: "Undo", run: () => void this.setHidden(id, name, !hidden, false) },
      });
    }
    try {
      const answer = await this.ports.setHidden(id, hidden);
      if (mine === this.#asked) this.hold(answer);
    } catch (cause) {
      if (mine === this.#asked) {
        if (hidden) this.hidden.delete(id);
        else this.hidden.add(id);
      }
      push({ text: `Could not ${hidden ? "hide" : "unhide"} ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** Back to nothing chosen and the real IPC, for a test. */
  forget(): void {
    this.home = null;
    this.hidden.clear();
    this.ports = REAL;
    this.#asked = 0;
  }
}

/** A fresh set of choices, nothing chosen, over the real IPC. */
export function createDocsChoices(): Choices {
  return new Choices();
}

/** The window's one. */
export const docsChoices = createDocsChoices();
