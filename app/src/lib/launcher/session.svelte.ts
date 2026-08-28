/**
 * The launcher's state machine: debounce, sequencing, selection.
 *
 * Everything here is a rule that a component cannot be trusted to reproduce by
 * hand, and every one of them is a bug somebody has shipped before:
 *
 * - **90 ms debounce**, so a five-letter word is one query and not five.
 * - **A monotonic request id**, so a slow query that lands *after* a faster,
 *   newer one is dropped rather than applied. Without it the rows under the
 *   cursor change to an older answer and the box reads as "jumping" — the
 *   nastiest kind of bug, because it only appears when the corpus is big
 *   enough for two queries to overtake each other.
 * - **The selection resets to the first row on every new answer**, because the
 *   row that was under the cursor is not in the new list.
 *
 * A `.svelte.ts` module: `$state` is compiler syntax, and only files with this
 * suffix are compiled by `vite-plugin-svelte`.
 */
import { type LauncherHome, type SearchQuery, type SearchResponse, ipcErrorMessage } from "../ipc";
import type { LauncherAction } from "./actions";
import { type LauncherMode, type LauncherRow, flatten, modeOf } from "./rows";
import { SYNTAX } from "./syntax";

/** How long the box waits after the last keystroke. */
export const DEBOUNCE_MS = 90;

/** How many rows one answer may carry. The engine clamps to 200 anyway. */
export const SEARCH_LIMIT = 40;

/** The IPC the session drives. Injected so tests need no bridge. */
export interface SessionPorts {
  search(query: SearchQuery): Promise<SearchResponse>;
  launcherHome(): Promise<LauncherHome>;
}

export interface SessionOptions extends SessionPorts {
  actions?: LauncherAction[];
  debounceMs?: number;
}

export class Session {
  #ports: SessionPorts;
  #debounceMs: number;
  #timer: ReturnType<typeof setTimeout> | undefined;

  /**
   * The last id handed out, and the newest id whose answer has been applied.
   *
   * Two counters and not one: `#issued` orders the requests, `#applied` is
   * what makes a late answer to an older request droppable. A single "is this
   * the latest" check against `#issued` would also drop a *correct* answer
   * that merely raced the next keystroke's timer.
   */
  #issued = 0;
  #applied = 0;

  raw = $state("");
  response = $state<SearchResponse | null>(null);
  home = $state<LauncherHome | null>(null);
  error = $state<string | null>(null);
  pending = $state(false);
  selected = $state(0);
  actions = $state<LauncherAction[]>([]);

  constructor(options: SessionOptions) {
    this.#ports = options;
    this.#debounceMs = options.debounceMs ?? DEBOUNCE_MS;
    this.actions = options.actions ?? [];
  }

  get mode(): LauncherMode {
    return modeOf(this.raw, this.response);
  }

  get rows(): LauncherRow[] {
    return flatten({
      mode: this.mode,
      response: this.response,
      home: this.home,
      actions: this.actions,
      syntax: SYNTAX,
    });
  }

  /** The row `Enter` opens, if there is one. */
  get current(): LauncherRow | null {
    return this.rows[this.selected] ?? null;
  }

  /** A keystroke: remember it and schedule the query. */
  type(raw: string): void {
    this.raw = raw;
    this.#schedule();
  }

  /** Replace the box's contents and query immediately — a clicked chip. */
  set(raw: string): void {
    this.raw = raw;
    this.#cancel();
    void this.run();
  }

  /** Move the cursor, clamped. The list does not wrap: a launcher whose
   *  selection jumps from the last row back to the first loses the reader. */
  move(delta: number): void {
    const last = this.rows.length - 1;
    if (last < 0) {
      this.selected = 0;
      return;
    }
    this.selected = Math.min(last, Math.max(0, this.selected + delta));
  }

  /** Load the board. Called once when the overlay opens. */
  async loadHome(): Promise<void> {
    try {
      this.home = await this.#ports.launcherHome();
      this.error = null;
    } catch (cause) {
      this.error = ipcErrorMessage(cause);
    }
  }

  /**
   * Ask the backend, now.
   *
   * An empty box asks nothing: the board is already loaded, and a query with
   * neither text nor a filter is one the engine refuses anyway.
   */
  async run(): Promise<void> {
    if (this.raw.trim() === "") {
      this.response = null;
      this.selected = 0;
      this.pending = false;
      return;
    }
    const id = ++this.#issued;
    this.pending = true;
    try {
      const response = await this.#ports.search({
        raw: this.raw,
        limit: SEARCH_LIMIT,
        filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      });
      // The whole point of the counter. A stale answer is dropped *silently*:
      // a newer one is already on screen and there is nothing to report.
      if (id <= this.#applied) return;
      this.#applied = id;
      this.response = response;
      this.error = null;
      this.selected = 0;
    } catch (cause) {
      if (id <= this.#applied) return;
      this.#applied = id;
      this.error = ipcErrorMessage(cause);
      this.response = null;
    } finally {
      // Only the newest request may clear the spinner, or an overtaken slow
      // answer would say "done" while a newer query is still in flight.
      if (id === this.#issued) this.pending = false;
    }
  }

  /** Stop the pending timer. Called when the overlay closes. */
  dispose(): void {
    this.#cancel();
  }

  #schedule(): void {
    this.#cancel();
    this.#timer = setTimeout(() => {
      this.#timer = undefined;
      void this.run();
    }, this.#debounceMs);
  }

  #cancel(): void {
    if (this.#timer !== undefined) {
      clearTimeout(this.#timer);
      this.#timer = undefined;
    }
  }
}
