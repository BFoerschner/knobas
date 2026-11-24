/**
 * What the XML view's parts share (its design, 2026-11-23, section 5): the outline's answers, the folds, the picked
 * line and the way down to it. One per document drawn; `TfDocView` makes it and hands it to its parts.
 *
 * **Folds stay where they were put** (section 8): kept by repository and path for the window's life, the last 50 files.
 * The same content draws folded in its first frame, from the fold table kept with them, with no outline needed. Changed
 * content opens unfolded; once its outline lands, the folds are found again by their elements and *Fold as before (n)*
 * is offered. Not across a restart of knobas (Björn's answer 7).
 */
import { untrack } from "svelte";

import { foldedTo, foldsUnder, holderOf, levelsOpen, noFolds, opening, stepLevels, visibleLines, rowOfLine, foldDepth, type FoldTable, type Folded } from "../../../render/doc-folds";
import type { JsonQueryAnswer } from "../../../ipc/api";
import type { Crumb, DocOpened, Picked, QueryAnswer, ResultRow, ValueAt } from "../../../render/doc-model";
import type { DocSession } from "../../../render/doc-port";

/** How many files' folds the window keeps (section 8). */
export const KEPT_FILES = 50;

/** One file's folds, as left. */
interface KeptFolds {
  /** What the content was: the file's blob where known, else its length and a sample's hash. */
  content: string;
  table: FoldTable;
  folded: number[];
  /** The same folds named by their elements, once the worker has named them. */
  keys: string[];
}

const kept = new Map<string, KeptFolds>();

/** Forgets every file's folds and what was asked (tests; a restart does it by itself). */
export function forgetFolds(): void {
  kept.clear();
  asked.length = 0;
}

/**
 * What was asked since knobas started (section 23): the last 50 queries, each once, the newest first. Kept when the
 * query answered and the person acted on it. In the window's memory only: no settings row, no database row, no address.
 */
const asked: string[] = [];
/** How many queries are remembered. */
export const ASKED = 50;

/** The queries asked since knobas started, the newest first. */
export function askedBefore(): readonly string[] {
  return asked;
}

/** Remembers a query that answered and was acted on. */
export function remember(source: string): void {
  const s = source.trim();
  if (s === "") return;
  const at = asked.indexOf(s);
  if (at !== -1) asked.splice(at, 1);
  asked.unshift(s);
  if (asked.length > ASKED) asked.length = ASKED;
}

/** The query's state: nothing asked, asked and waiting, or what it answered. */
export type QueryState = { state: "empty" } | { state: "asking" } | QueryAnswer;

/** In a document over this, the query runs when typing rests for {@link REST_MS} (section 22). */
export const REST_FROM = 2 * 1024 * 1024;
export const REST_MS = 120;

/** A name for a text's content: its blob where the source gave one, else its length and a hash of a sample of it. */
export function contentOf(text: string, sha: string | null): string {
  if (sha !== null && sha !== "") return sha;
  let h = 0x811c9dc5;
  const step = Math.max(1, Math.floor(text.length / 65536));
  for (let i = 0; i < text.length; i += step) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return `${text.length}:${h.toString(16)}`;
}

export class DocState {
  readonly session: DocSession;
  readonly key: string;
  readonly content: string;
  /** What the worker answered of the document; `null` until it lands. */
  opened = $state.raw<DocOpened | null>(null);
  /** What folds: nothing until the outline lands, or the table kept with the same content's folds. */
  folds = $state.raw<FoldTable>(noFolds(0));
  folded = $state.raw<Folded>(new Set());
  /** The lines on screen, in order. */
  readonly visible = $derived(visibleLines(this.folds, this.folded));
  /** The picked line (from 1), 0 for none. */
  cursor = $state(0);
  crumbs = $state.raw<Crumb[]>([]);
  picked = $state.raw<Picked | null>(null);
  /** Folds left on another version of this file, found again here: what *Fold as before* applies. */
  before = $state.raw<number[] | null>(null);
  /** What is typed into the query's field. */
  source = $state("");
  /** What the query answered: the last answer stands while a newer one is asked. */
  q = $state.raw<QueryState>({ state: "empty" });
  /** The rows of the results drawn so far (more are asked as the list scrolls). */
  rows = $state.raw<ResultRow[]>([]);
  /** The lines that hold a match, for the marks and the folds' counts. */
  hitLines = $state.raw<Uint32Array>(new Uint32Array(0));
  readonly hits = $derived(new Set(this.hitLines));
  /** The match or finding stepped to, by its place in the rail's list; -1 for none. */
  cur = $state(-1);
  /** The values of the lines read so far, by line: only the lines in view are read (section 17). */
  values = $state.raw<ReadonlyMap<number, ValueAt[]>>(new Map());
  #valuesAsked = new Set<number>();
  /** The lines drawn last: read once the outline lands. */
  #drawn: readonly number[] = [];
  /** The source of the query in flight, and the newest source typed while it ran. */
  #asking: string | null = null;
  #next: string | null = null;
  #rest: ReturnType<typeof setTimeout> | null = null;
  #keysAsked = 0;
  /**
   * jq, where the document is JSON (the XML view's design, section 26): a filter asked of the document's session, kept
   * running in a helper. `null` asks the document worker's XPath.
   */
  jq: ((filter: string) => Promise<JsonQueryAnswer>) | null = null;
  #keysTimer: ReturnType<typeof setTimeout> | null = null;
  #atAsked = 0;

  constructor(session: DocSession, key: string, content: string, lines: number) {
    this.session = session;
    this.key = key;
    this.content = content;
    const left = kept.get(key);
    if (left !== undefined && left.content === content) {
      // The same content: the folds as they were, in the first frame.
      this.folds = left.table;
      this.folded = new Set(left.folded);
    } else this.folds = noFolds(lines);
  }

  /** The outline landed: its fold table, and the folds of another version found again. */
  take(opened: DocOpened): void {
    this.opened = opened;
    const left = kept.get(this.key);
    const same = left !== undefined && left.content === this.content;
    if (!same || this.folds.line.length === 0) this.folds = opened.folds;
    if (left !== undefined && !same && left.keys.length > 0) {
      const asked = this.session;
      void asked.find(left.keys).then((lines) => {
        if (lines.length > 0 && this.folded.size === 0) this.before = lines;
      });
    }
    if (this.cursor > 0) this.ask(this.cursor);
    this.readValues(this.#drawn);
  }

  /** *Fold as before*: the folds found again, shut. */
  foldAsBefore(): void {
    if (this.before === null) return;
    this.setFolded(new Set(this.before));
    this.before = null;
  }

  /** Changes what is folded, and keeps it for the file. */
  setFolded(next: Folded): void {
    this.folded = next;
    this.before = null;
    this.#keep();
  }

  #keep(): void {
    const folded = [...this.folded];
    const left = kept.get(this.key);
    kept.delete(this.key);
    kept.set(this.key, { content: this.content, table: this.folds, folded, keys: left?.content === this.content ? left.keys : [] });
    while (kept.size > KEPT_FILES) kept.delete(kept.keys().next().value!);
    // The folds' names, asked once the folding rests: what finds them again in another version of the file.
    if (this.#keysTimer !== null) clearTimeout(this.#keysTimer);
    const asked = ++this.#keysAsked;
    this.#keysTimer = setTimeout(() => {
      this.#keysTimer = null;
      void this.session.keys(folded).then((keys) => {
        if (asked !== this.#keysAsked) return;
        const now = kept.get(this.key);
        if (now !== undefined && now.content === this.content) now.keys = keys.filter((k): k is string => k !== null);
      });
    }, 250);
  }

  /** Folds or unfolds the fold at `line`; `all` does every fold under it too. */
  toggle(line: number, all = false, to?: boolean): void {
    const f = this.folds.byLine[line] ?? -1;
    if (f === -1) return;
    const shut = to ?? !this.folded.has(line);
    const next = new Set(this.folded);
    for (const l of all ? foldsUnder(this.folds, line) : [line]) {
      if (shut) next.add(l);
      else next.delete(l);
    }
    this.setFolded(next);
  }

  /** Whether the fold at `line` is shut. */
  shut(line: number): boolean {
    return this.folded.has(line) && (this.folds.byLine[line] ?? -1) !== -1;
  }

  /** How many levels are open everywhere, and how deep the document goes. */
  readonly levels = $derived(levelsOpen(this.folds, this.folded));
  readonly depth = $derived(foldDepth(this.folds));

  /** Exactly `n` levels (the digit keys), or every level (0, and a press on the number). */
  toLevel(n: number): void {
    this.setFolded(n === 0 ? new Set() : foldedTo(this.folds, n));
    this.#keepCursor();
  }

  /** One level more or fewer (the stepper's `+` and `−`). */
  stepLevel(by: 1 | -1): void {
    const n = this.levels;
    if (by < 0 ? n <= 1 : n >= this.depth) return;
    this.setFolded(stepLevels(this.folds, this.folded, by));
    this.#keepCursor();
  }

  /** After the levels changed, the picked line stays picked, or the fold that now hides it is. */
  #keepCursor(): void {
    if (this.cursor > 0) this.pick(this.visible[rowOfLine(this.visible, this.cursor)] ?? 1);
  }

  /** `←`: fold the element the picked line opens; on one that opens nothing or is folded, go to what holds it. */
  left(all: boolean): void {
    const line = this.cursor;
    if (line === 0) return;
    const f = this.folds.byLine[line] ?? -1;
    if (f !== -1 && (all || !this.folded.has(line))) {
      this.toggle(line, all, true);
      return;
    }
    const up = holderOf(this.folds, line);
    if (up > 0) this.pick(up);
  }

  /** `→`: unfold the picked line's fold, everything under it with `all`. */
  right(all: boolean): void {
    const line = this.cursor;
    if (line === 0) return;
    if ((this.folds.byLine[line] ?? -1) !== -1) this.toggle(line, all, false);
  }

  /** Opens what hides `line`: a step to a line inside a fold is the person's own press, so what moves, moves for a reason. */
  reach(line: number): void {
    const next = opening(this.folds, this.folded, line);
    if (next !== this.folded) this.setFolded(next);
  }

  /** Picks a line, opening what hides it, and asks for the way down to it. */
  pick(line: number): void {
    if (line < 1) return;
    this.reach(line);
    this.cursor = line;
    this.ask(line);
  }

  /** `↑` and `↓`: the next line on screen. */
  move(by: 1 | -1, from: number): void {
    const vis = this.visible;
    if (vis.length === 0) return;
    if (this.cursor === 0) {
      this.pick(vis[rowOfLine(vis, Math.max(1, from))]!);
      return;
    }
    const row = rowOfLine(vis, this.cursor) + by;
    this.pick(vis[Math.max(0, Math.min(vis.length - 1, row))]!);
  }

  ask(line: number): void {
    const asked = ++this.#atAsked;
    if (untrack(() => this.opened) === null) return;
    void this.session.at(line).then((answer) => {
      if (asked !== this.#atAsked) return;
      this.crumbs = answer.crumbs;
      this.picked = answer.picked;
    });
  }

  /** Asks the query typed: at once, or when typing rests in a large document. A newer key replaces a query that waits. */
  queryTyped(source: string, size: number): void {
    this.source = source;
    if (this.#rest !== null) clearTimeout(this.#rest);
    this.#rest = null;
    if (source.trim() === "") {
      this.#next = null;
      this.q = { state: "empty" };
      this.rows = [];
      this.hitLines = new Uint32Array(0);
      this.cur = -1;
      return;
    }
    if (size > REST_FROM) this.#rest = setTimeout(() => this.#send(source), REST_MS);
    else this.#send(source);
  }

  #send(source: string): void {
    if (this.#asking !== null) {
      this.#next = source;
      return;
    }
    this.#asking = source;
    if (this.q.state === "empty") this.q = { state: "asking" };
    const asked = this.jq !== null ? this.#jqAnswer(this.jq, source) : this.session.query(source);
    void asked.then((answer) => {
      this.#asking = null;
      const waiting = this.#next;
      this.#next = null;
      if (waiting !== null && waiting !== source) {
        this.#send(waiting);
        return;
      }
      if (this.source !== source) return;
      // An expression not yet finished leaves the last results' marks off.
      this.q = answer;
      this.cur = -1;
      if (answer.state === "nodes") {
        this.rows = answer.rows;
        this.hitLines = answer.lines;
      } else {
        this.rows = [];
        this.hitLines = new Uint32Array(0);
      }
    });
  }

  /**
   * A jq answer in the query's words: a filter that picks answers its values on their lines, the worker making its
   * paths rows; one that makes new values answers them alone; jq's errors and stops as they are.
   */
  async #jqAnswer(jq: (filter: string) => Promise<JsonQueryAnswer>, source: string): Promise<QueryAnswer> {
    let answer: JsonQueryAnswer;
    try {
      answer = await jq(source);
    } catch (cause) {
      return { state: "stopped", message: cause instanceof Error ? cause.message : String(cause), ms: 0 };
    }
    if (answer.state === "failed") return { state: "error", message: answer.why, at: -1 };
    if (answer.state === "stopped") return { state: "stopped", message: answer.why, ms: 0 };
    if (answer.paths === null) return { state: "values", count: answer.count, values: answer.values, ms: answer.ms };
    const { rows, lines } = await this.session.jsonRows(answer.paths);
    return { state: "nodes", count: answer.count, rows, lines, ms: answer.ms };
  }

  /** Reads the values of the lines drawn, those not read yet. */
  readValues(lines: readonly number[]): void {
    this.#drawn = lines;
    if (this.opened === null) return;
    const want = lines.filter((l) => !this.#valuesAsked.has(l));
    if (want.length === 0) return;
    for (const l of want) this.#valuesAsked.add(l);
    void this.session.values(want).then((got) => {
      const next = new Map(this.values);
      for (const l of want) next.set(l, got[l] ?? []);
      this.values = next;
    });
  }

  /** More rows of the results, as the list scrolls. */
  more(): void {
    const q = this.q;
    if (q.state !== "nodes" || this.rows.length >= q.count || this.#loading) return;
    this.#loading = true;
    const from = this.rows.length;
    void this.session.results(from, 2000).then((more) => {
      this.#loading = false;
      if (this.q !== q) return;
      this.rows = [...this.rows, ...more];
    });
  }
  #loading = false;

  close(): void {
    if (this.#rest !== null) clearTimeout(this.#rest);
    if (this.#keysTimer === null) {
      this.session.close();
      return;
    }
    // Folded a moment ago: the folds are named first, then the session goes.
    clearTimeout(this.#keysTimer);
    this.#keysTimer = null;
    const folded = [...this.folded];
    void this.session.keys(folded).then((keys) => {
      const now = kept.get(this.key);
      if (now !== undefined && now.content === this.content) now.keys = keys.filter((k): k is string => k !== null);
      this.session.close();
    });
  }
}
