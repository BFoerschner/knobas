/**
 * What the schema explorer's parts share (the XSD design, 2026-11-23, sections 7 to 12): the set's index, the place
 * shown and its page, the rail's mode, folds and cursor, the rows opened in place, the way back. Kept for the set's
 * life; nothing of it is saved.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";

import type { Checked, PageAnswer, Row, SetIndex, TreeRow } from "../../../render/xsd-answer";
import { localeOf } from "../../../render/example-fakes";
import type { Example, ExampleOptions } from "../../../render/xsd-example";
import type { XsdPort } from "../../../render/xsd-port";
import type { RailMode } from "../../../render/xsd-view";

/** An example's first seed (`xsd-example.ts`' `SEED`, which the page does not load: the maker runs in the worker). */
const SEED = 4711;

/** No place: the overview. */
export const OVERVIEW = "";

export class XsdState {
  readonly port: XsdPort;
  index = $state.raw<SetIndex | null>(null);
  mode = $state<RailMode>("tree");
  typed = $state("");
  cursor = $state<string | null>(null);
  /** Open tree rows by id, file folds by `F:<path>` (and `F:<entry>:shut` for the opened file's). */
  folds = new SvelteSet<string>();
  /** The tree's children read so far, by the row that holds them (`""` the roots). */
  tree = new SvelteMap<string, readonly TreeRow[]>();
  /** The place shown (`el/ORDER`), `""` the overview. */
  current = $state(OVERVIEW);
  /** The page answered for it; `null` while asked, or for the overview. */
  page = $state.raw<PageAnswer | null>(null);
  /** The page about to be drawn goes back to a scroll: it draws whole at once, not a screenful first. */
  drawWhole = false;
  /** A place asked for while the set's files are still being read. */
  waiting: string | null = null;
  /** A place the set does not have, said once on the overview. */
  lost = $state<string | null>(null);
  /** Rows opened in place, and what each holds once answered. */
  expand = new SvelteSet<string>();
  kids = new SvelteMap<string, readonly Row[]>();
  /** *Show all of it*, *Show the other …*: what is shown whole. */
  more = new SvelteSet<string>();
  /** Each followed link's way back: the place left, its label and its scroll. */
  backs = $state<{ place: string; label: string; scroll: number }[]>([]);
  /** The folds and scroll the filter's first word put aside. */
  beforeFilter: { folds: string[]; scroll: number } | null = null;

  // ---- The example (sections 19 to 23) ----
  /** How examples are made: kept while the set is open, never saved. The language starts as the window's. */
  ex = $state.raw<ExampleOptions>({ seed: SEED, optional: "required", repeats: 1, lang: localeOf(globalThis.navigator?.language), picks: {} });
  /** The example of the element shown, with the element and settings it was made for; `null` before it is made. */
  made = $state.raw<{ key: string; options: ExampleOptions; example: Example } | null>(null);
  /** What the check said of it: `"checking"` while libxml2 reads it. */
  verdict = $state.raw<Checked | "checking" | null>(null);
  /** Under 1,480 px the example takes the page's place: which one the page shows. */
  tab = $state<"ref" | "example">("ref");
  #making = 0;

  constructor(port: XsdPort) {
    this.port = port;
  }

  /** Takes a round's index; the tree's roots are read once, the first root open one level. */
  take(index: SetIndex): void {
    const first = this.index === null;
    this.index = index;
    if (first || !this.tree.has("")) void this.loadRoots(first);
    // A place asked for before its file was read: asked again with each round, until it is found or all is read.
    if (!first && this.waiting !== null) void this.show(this.waiting);
    // The tree's folds read so far are read again: a name from a file just read now opens.
    if (!first) void this.refreshTree();
  }

  async loadRoots(openFirst: boolean): Promise<void> {
    const roots = await this.port.tree(null);
    this.tree.set("", roots);
    const r = roots[0];
    if (openFirst && r !== undefined && r.kids) await this.fold(r.id, true);
  }

  /** Opens the tree down to an element (a page the rail did not pick) and puts the keys on its row. */
  async reveal(key: string): Promise<string | null> {
    const way = await this.port.way(key);
    if (way.length === 0) return null;
    if (!this.tree.has("")) this.tree.set("", await this.port.tree(null));
    for (const id of way.slice(0, -1)) await this.fold(id, true);
    return way.at(-1)!;
  }

  /** Reads every fold of the tree read so far again, after a round of files landed. */
  async refreshTree(): Promise<void> {
    for (const id of [...this.tree.keys()]) this.tree.set(id, await this.port.tree(id === "" ? null : id));
  }

  /** Opens or shuts a fold: a tree row reads its children the first time. */
  async fold(id: string, open?: boolean): Promise<void> {
    const want = open ?? !this.folds.has(id);
    if (!want) {
      this.folds.delete(id);
      return;
    }
    if (!id.startsWith("F:") && !this.tree.has(id)) this.tree.set(id, await this.port.tree(id));
    this.folds.add(id);
  }

  /** The opened file's fold: open until it is shut. */
  foldFile(path: string, depth: number, open?: boolean): void {
    if (depth === 0) {
      const shut = `F:${path}:shut`;
      const now = !this.folds.has(shut);
      if ((open ?? !now) === now) return;
      if (now) this.folds.add(shut);
      else this.folds.delete(shut);
      return;
    }
    void this.fold(`F:${path}`, open);
  }

  /** Shows a place: its page is asked of the worker, the overview needs nothing. */
  async show(place: string): Promise<boolean> {
    this.current = place;
    if (place === OVERVIEW) {
      this.page = null;
      return true;
    }
    const answer = await this.port.page(place);
    if (this.current !== place) return false;
    if ("missing" in answer) {
      // The set is still being read: the place may be in a file not read yet, so it is asked again as each lands.
      if (answer.reading > 0) {
        this.waiting = place;
        // A round landed while the answer was on its way: ask again now.
        if ((this.index?.reading ?? 0) === 0) return this.show(place);
        return false;
      }
      this.lost = place;
      this.current = OVERVIEW;
      this.page = null;
      return true;
    }
    this.lost = null;
    this.waiting = null;
    this.page = answer;
    return true;
  }

  /** Opens or shuts a row in place; with `all`, every row under it too. */
  async toggle(id: string, all = false): Promise<void> {
    if (this.expand.has(id)) {
      for (const k of [...this.expand]) if (k === id || (all && k.startsWith(`${id}/`))) this.expand.delete(k);
      return;
    }
    await this.open(id, all ? 6 : 0);
  }

  /** Opens a row, reading what it holds the first time; `depth` more levels under it too. */
  async open(id: string, depth: number): Promise<void> {
    if (!this.kids.has(id)) this.kids.set(id, await this.port.rows(id));
    this.expand.add(id);
    if (depth > 0) for (const r of openable(this.kids.get(id) ?? [])) await this.open(r, depth - 1);
  }

  /** *Expand all* at a section's label: every row of the page, a few levels down. */
  async openAll(rows: readonly Row[]): Promise<void> {
    for (const id of openable(rows)) await this.open(id, 3);
  }

  /** Makes the example of element `key` with the settings as they stand; a newer ask wins. */
  async makeExample(key: string): Promise<void> {
    const run = ++this.#making;
    const options = this.ex;
    const example = await this.port.example(key, options);
    if (run !== this.#making) return;
    this.verdict = null;
    this.made = example === null ? null : { key, options, example };
  }

  /** A control changed: the settings, and the example made again by whoever shows it. */
  setExample(change: Partial<ExampleOptions>): void {
    this.ex = { ...this.ex, ...change };
  }

  /** *Another* and `A`: a new seed, never the one shown. */
  another(): void {
    let seed = this.ex.seed;
    while (seed === this.ex.seed) seed = 1 + Math.floor(Math.random() * 99_999);
    this.setExample({ seed });
  }

  /** A choice's branch picked on its line: it holds wherever the choice stands, for the set's life. */
  pick(choice: string, branch: number): void {
    this.setExample({ picks: { ...this.ex.picks, [choice]: branch } });
  }

  /** *Check*: libxml2's verdict on the example shown. */
  async checkExample(): Promise<void> {
    const made = this.made;
    if (made === null) return;
    this.verdict = "checking";
    const said = await this.port.check(made.example.xml);
    if (this.made === made) this.verdict = said;
  }
}

/** The rows among these (and inside their groups) that open in place. */
export function openable(rows: readonly Row[]): string[] {
  const out: string[] = [];
  for (const r of rows) {
    if (r.k === "el" && r.can) out.push(r.id);
    else if (r.k === "group") out.push(...openable(r.items));
  }
  return out;
}

/** A place's label, for *Back to*. */
export const labelOfPlace = (place: string): string => (place === OVERVIEW ? "the overview" : place.slice(place.indexOf("/") + 1));
