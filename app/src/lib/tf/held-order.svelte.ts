/**
 * A list's rows stay where they first drew (the still-rows pass of PR #2479, 2026-11-11; Björn: "another repo just pops
 * in where my cursor was"). After a list first draws, nothing a person could be pointing at moves unless they did
 * something: a read that lands later fills rows in, and never puts a row above another, reorders them, or takes one
 * out from under the pointer.
 *
 * - **A row already drawn keeps its place**, whatever a later read says of its order.
 * - **A row new to the list** joins at the end when that is where it sorts anyway, as a list's next page does. One
 *   that sorts above a drawn row waits: it is counted, not drawn, and the page offers it behind *Show N new*
 *   (`TfNewRows`), which is the person's own press.
 * - **A row that left** the list stays drawn, as it was, until the person acts: taking it out would close the list up
 *   under the pointer. A list whose rows leave by the person's own press (the Inbox's *Done*, which holds its row under
 *   the pointer itself) passes `{ keepLeft: false }`: the row goes, and its place is kept, so one brought back (*Undo*)
 *   stands where it stood.
 * - **The person's act sorts again**: a changed filter, scope or tab (the `basis`), or *Show N new* / *Sort again*
 *   ({@link HeldOrder.release}).
 * - **Nobody pointing**: a list whose pointer is off it lets the waiting rows in where they sort, and the rows that left
 *   go, while the drawn rows keep their order ({@link HeldOrder.settle}, #2574: a queued run that starts keeps its row).
 *
 * The order of first draw is what the page sorted then, so a page whose order needs a late read waits for that read
 * before it draws (Code waits for the open pull requests that make a repository yours).
 */

export interface Placed<T> {
  /** The rows to draw, in their held places. */
  rows: T[];
  /** Rows the read brought that sort above a drawn one: waiting behind *Show N new*. */
  waiting: number;
  /** Whether the read differs from what is drawn in order, or by a row that left: *Sort again* would move rows. */
  stale: boolean;
  /** Rows that left the read and are still drawn ({@link HeldOrder.settle} takes them out). */
  left?: number;
}

export class HeldOrder<T> {
  #key: (row: T) => string;
  #tag: (row: T) => string;
  /** Each drawn row's place, by key, the row as last read (kept for a row that left), and its group as it drew. */
  #held: Map<string, { rank: number; row: T; tag: string }> | null = null;
  #basis: string | null = null;
  #next = 0;
  /** Moves on a release, so a `$derived` that placed rows places them again. */
  #released = $state(0);
  #keepLeft: boolean;
  #byTag: boolean;
  /** Rows the person's own press brings in (*Wake*): they take their place where they sort, at once. */
  #admitted = new Set<string>();
  /** The next {@link HeldOrder.place} admits every waiting row and drops every row that left ({@link HeldOrder.settle}). */
  #settling = false;

  /**
   * `key` names a row across reads. `tag`, where given, is the group a row draws under (Code's *Yours*, *Merged or
   * closed*): a drawn row keeps the group it drew in, read by {@link HeldOrder.tagOf}, until the person acts.
   *
   * `byTag`: the groups draw side by side, as a board's columns do, so a new row that sorts after every drawn row of its
   * own group joins that group's end and moves nothing, wherever the other groups' rows sort (a board that shows every
   * ticket, ux18: its whole read fills each column below the fifty it drew first). A row of a group with no drawn row
   * takes the stacked rule: a new column's card waits behind *Show N new* with its column.
   */
  constructor(
    key: (row: T) => string,
    tag: (row: T) => string = () => "",
    { keepLeft = true, byTag = false }: { keepLeft?: boolean; byTag?: boolean } = {},
  ) {
    this.#key = key;
    this.#tag = tag;
    this.#keepLeft = keepLeft;
    this.#byTag = byTag;
  }

  /** The group `row` drew in, or its own where it has not drawn. */
  tagOf(row: T): string {
    return this.#held?.get(this.#key(row))?.tag ?? this.#tag(row);
  }

  /**
   * `fresh` in held order. `basis` names what the person chose (filter, scope, tab): a change to it is their act, and
   * the list sorts again. Nothing is held until `fresh` has a row, so an empty first answer holds nothing.
   */
  place(fresh: readonly T[], basis: string): Placed<T> {
    void this.#released;
    if (this.#held === null || this.#basis !== basis) {
      this.#basis = basis;
      this.#held = null;
      this.#settling = false;
      if (fresh.length === 0) return { rows: [], waiting: 0, stale: false };
      this.#held = new Map();
      this.#next = 0;
      for (const row of fresh) this.#held.set(this.#key(row), { rank: this.#next++, row, tag: this.#tag(row) });
      return { rows: [...fresh], waiting: 0, stale: false };
    }
    const held = this.#held;
    const keys = fresh.map((row) => this.#key(row));
    // Spent by this read: every row it holds that waits comes in, and every row that left goes.
    const settling = this.#settling;
    this.#settling = false;
    if (settling) {
      const present = new Set(keys);
      for (const key of [...held.keys()]) if (!present.has(key)) held.delete(key);
    }
    let lastHeld = -1;
    /** By group, where `byTag`: the place in `fresh` of its last drawn row. */
    const lastOf = new Map<string, number>();
    keys.forEach((key, at) => {
      const place = held.get(key);
      if (place === undefined) return;
      lastHeld = at;
      if (this.#byTag) lastOf.set(place.tag, at);
    });
    // A group with no drawn row is new to the list (a board's new column): it waits as any row would, so it is counted.
    const joinsEnd = (row: T, at: number) => {
      const last = this.#byTag ? lastOf.get(this.#tag(row)) : undefined;
      return last !== undefined ? at > last : at > lastHeld;
    };
    let waiting = 0;
    let previousRank = -1;
    let reordered = false;
    const present = new Set<string>();
    fresh.forEach((row, at) => {
      const key = keys[at]!;
      present.add(key);
      const place = held.get(key);
      if (place !== undefined) {
        // Filled in where it stands: the newest read of a drawn row.
        place.row = row;
        if (place.rank < previousRank) reordered = true;
        previousRank = place.rank;
      } else if (joinsEnd(row, at)) {
        // Where it sorts is after every drawn row: it joins the end and moves nothing.
        held.set(key, { rank: this.#next++, row, tag: this.#tag(row) });
      } else if (settling || this.#admitted.has(key)) {
        // The person brought it: between the drawn rows it sorts between. In a read whose drawn rows sort otherwise than
        // they stand (a queued run that started), above every drawn row it sorts above.
        const ranks = (part: readonly string[]) => part.filter((other) => held.has(other)).map((other) => held.get(other)!.rank);
        const above = ranks(keys.slice(at + 1));
        const high = above.length === 0 ? this.#next : Math.min(...above);
        const below = ranks(keys.slice(0, at));
        const low = below.length === 0 ? high - 1 : Math.max(...below);
        held.set(key, { rank: low < high ? (low + high) / 2 : high - 0.5, row, tag: this.#tag(row) });
        this.#admitted.delete(key);
      } else {
        waiting += 1;
      }
    });
    // A row that left stays drawn until the person acts, and *Sort again* takes it out; or, with `keepLeft` off, it goes
    // and keeps its place for a return.
    const rows = [...held]
      .filter(([key]) => this.#keepLeft || present.has(key))
      .sort(([, a], [, b]) => a.rank - b.rank)
      .map(([, place]) => place.row);
    const left = this.#keepLeft ? [...held.keys()].filter((key) => !present.has(key)).length : 0;
    return { rows, waiting, stale: reordered || left > 0, left };
  }

  /** A row the person's press brings into the list (*Wake*): when a read holds it, it stands where it sorts. */
  admit(key: string): void {
    this.#admitted.add(key);
    this.#released += 1;
  }

  /**
   * The person moved `key` to group `tag`, before the drawn row `before` (or at the group's end): it stands there at
   * once, and a later read keeps it there until the person sorts again (ticket-edit decisions, section 8: a card lands
   * where it was dropped). Answers where it stood, for {@link HeldOrder.restore}; `null` for a key that is not held.
   */
  move(key: string, tag: string, before: string | null): { tag: string; rank: number } | null {
    const held = this.#held;
    const place = held?.get(key);
    if (held == null || place === undefined) return null;
    const was = { tag: place.tag, rank: place.rank };
    const group = [...held].filter(([other, row]) => other !== key && row.tag === tag).sort(([, a], [, b]) => a.rank - b.rank);
    const at = before === null ? -1 : group.findIndex(([other]) => other === before);
    let rank: number;
    if (at < 0) rank = (group.at(-1)?.[1].rank ?? this.#next) + 1;
    else if (at === 0) rank = group[0]![1].rank - 1;
    else rank = (group[at - 1]![1].rank + group[at]![1].rank) / 2;
    place.tag = tag;
    place.rank = rank;
    this.#next = Math.max(this.#next, Math.ceil(rank) + 1);
    this.#released += 1;
    return was;
  }

  /** Put `key` back where {@link HeldOrder.move} said it stood: a refused move's card, or an undone one. */
  restore(key: string, was: { tag: string; rank: number }): void {
    const place = this.#held?.get(key);
    if (place === undefined) return;
    place.tag = was.tag;
    place.rank = was.rank;
    this.#released += 1;
  }

  /**
   * Nobody points at the list (#2574): the rows waiting take their places where they sort (a new run above every drawn
   * one takes the top), and the rows that left go. The drawn rows keep their order, so a row whose sort moved (a queued
   * run that started) stays where it stood; only the person's act ({@link HeldOrder.release}, a new basis) sorts it.
   */
  settle(): void {
    if (this.#held === null) return;
    this.#settling = true;
    this.#released += 1;
  }

  /** The person asked for the list as it reads now: *Show N new* or *Sort again*. */
  release(): void {
    this.#held = null;
    this.#released += 1;
  }
}
