/**
 * *Text search* in a repository's Files tab (ADR-0062): what is typed, the two switches beside it, and the answer.
 *
 * Typing asks once the keys rest ({@link REST} ms), so a word typed fast is one search; the backend stops the scan
 * before a newer one of the same place, and an answer that lands after a newer question was asked is dropped here, so
 * the list never shows words the field no longer holds. *Search in repository* on a selection hands its words to the
 * repository's tab through {@link askText}, read once by the tab that shows that repository ({@link takeAsked}).
 */
import { ipcErrorMessage } from "../../ipc";
import { entityTextSearch, type TextFound, type TextQuery } from "../../ipc/entity";

/** How long the keys rest before a search starts, in ms. */
export const REST = 150;

/** The tree the tab shows: what the search reads. */
export interface TextWhere {
  /** The repository, or the branch picked. */
  at: string;
  /** The commit the tab reads at, `null` for the mirror's tip. */
  commit: string | null;
  /** The working tree is chosen in the branch box. */
  working: boolean;
  /** The branch's name. */
  branch: string | null;
}

export type TextPort = (at: string, commit: string | null, working: boolean, branch: string | null, query: TextQuery) => Promise<TextFound>;

export const ipcTextPort: TextPort = (...asked) => entityTextSearch(...asked);

let asked = $state<{ repoId: string; text: string } | null>(null);

/** *Search in repository*: the words for `repoId`'s Files tab, which reads them when it shows. */
export function askText(repoId: string, text: string): void {
  asked = { repoId, text };
}

/** No words wait: for tests. */
export function forgetAsked(): void {
  asked = null;
}

/** The words handed to `repoId`'s tab, once; `null` where none wait. Reactive: a tab's effect re-runs on a hand-off. */
export function takeAsked(repoId: string): string | null {
  const now = asked;
  if (now === null || now.repoId !== repoId) return null;
  asked = null;
  return now.text;
}

export class TextSearch {
  typed = $state("");
  /** *Aa*: the case typed. Off, the case is smart. */
  caseSensitive = $state(false);
  /** *.\**: the words are a pattern. */
  regex = $state(false);
  /** File names are answered too (*Search in repository*). */
  names = $state(false);
  found = $state.raw<TextFound | null>(null);
  reading = $state(false);
  failed = $state<string | null>(null);

  #asked = 0;
  #timer: ReturnType<typeof setTimeout> | null = null;
  readonly #port: TextPort;
  readonly #where: () => TextWhere;

  constructor(port: TextPort, where: () => TextWhere) {
    this.#port = port;
    this.#where = where;
  }

  /** Whether words are typed. */
  get asking(): boolean {
    return this.typed.trim() !== "";
  }

  /** Search for what is typed: once the keys rest, or `now`. Nothing typed clears the answer. */
  ask(now = false): void {
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = null;
    this.#asked += 1;
    if (!this.asking) {
      this.found = null;
      this.failed = null;
      this.reading = false;
      return;
    }
    this.reading = true;
    this.#timer = setTimeout(() => this.#run(), now ? 0 : REST);
  }

  /** Stop: no answer still on its way is drawn. */
  stop(): void {
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = null;
    this.#asked += 1;
    this.reading = false;
  }

  #run() {
    this.#timer = null;
    const mine = this.#asked;
    const where = this.#where();
    const query: TextQuery = { text: this.typed, case_sensitive: this.caseSensitive, regex: this.regex, names: this.names };
    this.#port(where.at, where.commit, where.working, where.branch, query).then(
      (found) => {
        if (mine !== this.#asked || found.stale) return;
        this.found = found;
        this.failed = null;
        this.reading = false;
      },
      (error: unknown) => {
        if (mine !== this.#asked) return;
        this.found = null;
        this.failed = ipcErrorMessage(error);
        this.reading = false;
      },
    );
  }
}
