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
  /** Rows the person's own press brings in (*Wake*): they take their place where they sort, at once. */
  #admitted = new Set<string>();

  /**
   * `key` names a row across reads. `tag`, where given, is the group a row draws under (Code's *Yours*, *Merged or
   * closed*): a drawn row keeps the group it drew in, read by {@link HeldOrder.tagOf}, until the person acts.
   */
  constructor(key: (row: T) => string, tag: (row: T) => string = () => "", { keepLeft = true }: { keepLeft?: boolean } = {}) {
    this.#key = key;
    this.#tag = tag;
    this.#keepLeft = keepLeft;
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
      if (fresh.length === 0) return { rows: [], waiting: 0, stale: false };
      this.#held = new Map();
      this.#next = 0;
      for (const row of fresh) this.#held.set(this.#key(row), { rank: this.#next++, row, tag: this.#tag(row) });
      return { rows: [...fresh], waiting: 0, stale: false };
    }
    const held = this.#held;
    const keys = fresh.map((row) => this.#key(row));
    let lastHeld = -1;
    keys.forEach((key, at) => {
      if (held.has(key)) lastHeld = at;
    });
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
      } else if (at > lastHeld) {
        // Where it sorts is after every drawn row: it joins the end and moves nothing.
        held.set(key, { rank: this.#next++, row, tag: this.#tag(row) });
      } else if (this.#admitted.has(key)) {
        // The person brought it: between the drawn rows it sorts between.
        const before = keys.slice(0, at).reverse().find((other) => held.has(other));
        const after = keys.slice(at + 1).find((other) => held.has(other));
        const low = before === undefined ? -1 : held.get(before)!.rank;
        const high = after === undefined ? this.#next : held.get(after)!.rank;
        held.set(key, { rank: (low + high) / 2, row, tag: this.#tag(row) });
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
    const left = this.#keepLeft && [...held.keys()].some((key) => !present.has(key));
    return { rows, waiting, stale: reordered || left };
  }

  /** A row the person's press brings into the list (*Wake*): when a read holds it, it stands where it sorts. */
  admit(key: string): void {
    this.#admitted.add(key);
    this.#released += 1;
  }

  /** The person asked for the list as it reads now: *Show N new* or *Sort again*. */
  release(): void {
    this.#held = null;
    this.#released += 1;
  }
}
