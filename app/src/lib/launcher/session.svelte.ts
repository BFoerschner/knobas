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
import {
  type LauncherHome,
  type SearchQuery,
  type SearchResponse,
  type UrlMatch,
  ipcErrorMessage,
  noFilters,
} from "../ipc";
import type { LauncherAction } from "./actions";
import { type LauncherMode, type LauncherRow, flatten, modeOf } from "./rows";
import { SYNTAX } from "./syntax";
import { pastedUrl } from "./url";

/** How long the box waits after the last keystroke. */
export const DEBOUNCE_MS = 90;

/** How many rows one answer may carry. The engine clamps to 200 anyway. */
export const SEARCH_LIMIT = 40;

/** The IPC the session drives. Injected so tests need no bridge. */
export interface SessionPorts {
  search(query: SearchQuery): Promise<SearchResponse>;
  launcherHome(): Promise<LauncherHome>;
  /**
   * What a pasted link names — `resolve_url` (#496).
   *
   * **Optional, and its absence is the switch.** A session with no resolver
   * never leaves the search path: an absolute URL typed into it is a query
   * like any other, which is what the two link pickers built on this class
   * want (`detail/LinkDialog.svelte`, `notes/NoteView.svelte`) — they are
   * choosing a target from the corpus, and a paste there is not a request to
   * navigate anywhere. The launcher passes one, and it is the launcher that
   * draws the two outcomes.
   */
  resolveUrl?(url: string): Promise<UrlMatch | null>;
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
  /**
   * The absolute URL the box currently holds, or `null` when it holds a query
   * (#496).
   *
   * Set the moment the box is read, before anything is sent, because it
   * decides *which* backend read happens — the box is either a search or a
   * link, never both. It is what {@link mode} answers `"url"` on.
   */
  url = $state<string | null>(null);
  /**
   * What the resolver said about {@link url}, or `null` while it is still
   * being asked.
   *
   * Two levels of `null` and they are different answers, which is why the
   * outer one is a wrapper rather than the match itself: `urlAnswer === null`
   * is *still asking*, and `urlAnswer.match === null` is *the mirror does not
   * hold this link* — the miss, and the only state that offers *Open in
   * browser*. Collapsed into one field, a paste would flash the miss for as
   * long as the round trip takes.
   */
  urlAnswer = $state<{ match: UrlMatch | null } | null>(null);
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
    if (this.url !== null) return "url";
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
      this.#clear();
      this.pending = false;
      return;
    }
    // Before the query is sent, and never as a branch of the grammar (spec
    // #491): a link is not a search, and handing one to the FTS engine would
    // answer with whatever words happen to be in the host.
    const resolveUrl = this.#ports.resolveUrl;
    if (resolveUrl) {
      const url = pastedUrl(this.raw);
      if (url !== null) {
        await this.#resolve(url, resolveUrl);
        return;
      }
    }
    this.url = null;
    this.urlAnswer = null;
    const id = ++this.#issued;
    this.pending = true;
    try {
      const response = await this.#ports.search({
        raw: this.raw,
        limit: SEARCH_LIMIT,
        // `noFilters()` rather than the literal: the shape is the bridge's, so
        // a dimension added to `SearchFilters` is one edit there and none here.
        filters: noFilters(),
      });
      // The whole point of the counter. A stale answer is dropped *silently*:
      // a newer one is already on screen and there is nothing to report.
      // The second half is what a counter cannot see: a paste issued after
      // this query has already put a link in the box, and an answer applied
      // over it would put results behind a panel that is not showing them.
      if (id <= this.#applied || this.url !== null) return;
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

  /**
   * Stop the pending timer and forget the paste. Called when the overlay
   * closes.
   *
   * **The box is emptied too, and only for a paste.** A query and its answer
   * survive a close on purpose — reopening the launcher shows what was last
   * searched for. A paste has no such state to come back to: it has already
   * been opened or handed to the browser, and what makes the difference
   * structural rather than cosmetic is that the answer is what the launcher
   * *navigates on*. Left behind, it would either re-navigate on the next
   * opening or sit in a box that says it is searching with nothing in flight.
   */
  dispose(): void {
    this.#cancel();
    if (this.url !== null) {
      this.raw = "";
      this.#clear();
    }
  }

  /**
   * Ask the resolver what a pasted link names.
   *
   * Sequenced by the same two counters the search is, and for the same reason:
   * a reader who pastes a second link over the first must not be navigated to
   * whichever answer the network returned last.
   */
  async #resolve(
    url: string,
    resolveUrl: (url: string) => Promise<UrlMatch | null>,
  ): Promise<void> {
    const id = ++this.#issued;
    this.url = url;
    this.urlAnswer = null;
    this.response = null;
    this.selected = 0;
    this.pending = true;
    try {
      const match = await resolveUrl(url);
      if (this.#stale(id, url)) return;
      this.#applied = id;
      this.urlAnswer = { match };
      this.error = null;
    } catch (cause) {
      if (this.#stale(id, url)) return;
      this.#applied = id;
      this.error = ipcErrorMessage(cause);
      this.urlAnswer = null;
    } finally {
      if (id === this.#issued) this.pending = false;
    }
  }

  /**
   * Whether the answer to request `id` about `url` is worth applying.
   *
   * **Two questions, and the counter answers only one of them.** `#applied`
   * orders the answers, which is enough for a read whose result is *drawn* --
   * a stale one is replaced by the newer one already on screen. A resolved
   * paste is not drawn: the launcher **navigates** on it. So an answer that
   * arrives after the reader has typed the link back into a query would open
   * an entity nobody asked for and close the box over it, and the counter
   * cannot see that: the query bumps `#issued` but does not touch `#applied`
   * until its *own* answer lands, so a resolver that beats a slow search sails
   * through the ordering check. The box itself is what closes it -- the answer
   * is good only while the box still holds the URL it was asked about.
   */
  #stale(id: number, url: string): boolean {
    return id <= this.#applied || this.url !== url;
  }

  /** Everything an empty box has nothing to say about. */
  #clear(): void {
    this.response = null;
    this.url = null;
    this.urlAnswer = null;
    this.selected = 0;
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
