/**
 * A windowed list (#2209, ruling on #2203: window, don't cap): a long list
 * keeps every row it holds, in order, but only the rows in or near the
 * viewport are in the DOM; the rest are room above and below.
 *
 * ```svelte
 * const win = new Windowed({ count: () => rows.length, key: (i) => rows[i].id, estimate: 63, gap: 6 });
 *
 * <div class="list" {@attach win.list}>
 *   {#each win.slice(rows) as row (row.id)}<div {@attach win.row(row.id)}>…</div>{/each}
 * </div>
 *
 * <tbody {@attach win.list}>
 *   {#if win.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach win.room("before")}></td></tr>{/if}
 *   {#each win.slice(rows) as row (row.id)}<tr {@attach win.row(row.id)}>…</tr>{/each}
 *   {#if win.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach win.room("after")}></td></tr>{/if}
 * </tbody>
 * ```
 *
 * - `list` goes on the element that holds the rows (a `tbody` too). It reads
 *   what of the list is on screen from the boxes around it, so the view's
 *   layout decides. On a `tbody`, while windowed, the table's columns keep
 *   the widths its first drawn rows gave (`table-layout: fixed`): columns
 *   sized by whichever rows are drawn would re-wrap the rows on screen. With
 *   no header row, the first drawn row's cells give them, held by a `colgroup`.
 * - `row(key)` goes on each drawn row: its height is measured, the estimate
 *   stands for rows never drawn, and the row on screen is held where it was
 *   through each redraw (a row above drawn at other than its estimate, a
 *   table's columns widening), so scrolling never jumps.
 * - `before`/`after` are the undrawn rows' room. `list` pads a flex column
 *   with them itself (`gap` is the column's gap). A table draws spacer rows
 *   (`tr.tf-wgap`, `gap` 0) only while their room is above 0, so
 *   `:last-child` rules hold, and `room(side)` sizes each one's cell. Both
 *   go through the CSSOM: CSP refuses inline styles in markup.
 * - `slice(items)` is what to draw; `start` is its first index.
 * - `topOf(index)` is where row `index` sits on screen, drawn or not.
 * - A list that scrolls itself (`scrolls: true`) reads the
 *   viewport from its own scroll and, as a table does, draws spacers
 *   (`.tf-wgap`, `room(side)`) while their room is above 0: a scroller is
 *   never smaller than its own padding.
 * - `reveal(index)` scrolls an undrawn row near enough to draw; `flushSync`,
 *   then focus or `scrollIntoView` it as for any row (`j`/`k` past the window).
 * - Under `from` rows (300) everything draws and `before`/`after` are 0, so a
 *   short list is the DOM it always was.
 *
 * A folding list windows its flattened visible rows (headers of open folds
 * and their rows) with one key per row.
 */
import { Heights, type Span } from "./windowed";

export interface WindowedOptions<K> {
  /** How many rows the list holds. */
  count: () => number;
  /** Row `index`'s key: unique, stable while the row is the same row. */
  key: (index: number) => K;
  /** A row's height before it is drawn, in pixels. */
  estimate: number;
  /** The space between rows (a flex column's `gap`), in pixels. */
  gap?: number;
  /** How many pixels beyond the viewport to draw each side. */
  overscan?: number;
  /** Lists shorter than this draw every row. */
  from?: number;
  /**
   * The list scrolls itself (an asset column), so its rows move by its own
   * scroll. Not read from `overflow-y`: a list that clips only sideways (the
   * run log's `overflow: auto`) computes `auto` too, yet grows with its rows.
   */
  scrolls?: boolean;
}

const ALL: Span = { start: 0, end: 0, before: 0, after: 0 };

/**
 * The scroll anchor every windowed list shares (#2269): a diff draws one list
 * per file card, all in one scroller. Each list holding a row of its own and
 * scrolling to keep it still would move the page once per list; one anchor,
 * the topmost row on screen across them, moves it once. So is the scroll a
 * list made to hold it: every list hears that scroll event, and none takes it
 * for the reader's.
 */
const shared: {
  /** The row, its top, and the box that scrolls it with its scroll then. */
  anchor: { el: HTMLElement; y: number; box: Element | null; scroll: number } | null;
  /** The reader's scroll the anchor was taken at. */
  pinnedAt: Event | null;
  scrolled: { box: Element; top: number } | null;
  /** The scroll event a list's own scroll raised, once one list has heard it. */
  ownEvent: Event | null;
} = { anchor: null, pinnedAt: null, scrolled: null, ownEvent: null };

export class Windowed<K = string> {
  readonly #options: Required<WindowedOptions<K>>;
  readonly #heights: Heights<K>;
  /** The viewport, in the list's pixels: its top (negative while the list starts below it), and its height. */
  #viewTop = $state(0);
  #viewHeight = $state(typeof window === "undefined" ? 0 : window.innerHeight);
  /** Bumped when a measurement moves a row, so the span is worked out again. */
  #remeasured = $state(0);
  /** The list's ancestors that clip it, nearest first. */
  #clips: HTMLElement[] = [];
  #synced: readonly K[] | null = null;
  readonly #keyOf = new WeakMap<Element, K>();
  /** The rows in the DOM now. */
  readonly #drawn = new Set<HTMLElement>();
  /** Where row 0 sits on screen, once `list` is attached. */
  #origin = () => 0;
  readonly #sizes: ResizeObserver | null;

  readonly #keys = $derived.by(() => {
    const n = this.#options.count();
    const keys = new Array<K>(n);
    for (let i = 0; i < n; i++) keys[i] = this.#options.key(i);
    return keys;
  });

  readonly #span = $derived.by((): Span & { on: boolean } => {
    const keys = this.#keys;
    if (keys.length < this.#options.from)
      return { ...ALL, end: keys.length, on: false };
    void this.#remeasured;
    this.#sync(keys);
    return {
      ...this.#heights.span(
        this.#viewTop,
        this.#viewHeight,
        this.#options.overscan,
      ),
      on: true,
    };
  });

  /** Whether the list is long enough to window. */
  readonly on = $derived(this.#span.on);
  readonly start = $derived(this.#span.start);
  readonly end = $derived(this.#span.end);
  readonly before = $derived(this.#span.before);
  readonly after = $derived(this.#span.after);

  constructor(options: WindowedOptions<K>) {
    this.#options = {
      gap: 0,
      overscan: 600,
      from: 300,
      scrolls: false,
      ...options,
    };
    this.#heights = new Heights<K>(this.#options.estimate, this.#options.gap);
    this.#sizes =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver((entries) => this.#measure(entries));
  }

  /** The rows to draw out of `items` (the list's rows, in the same order). */
  slice<T>(items: readonly T[]): readonly T[] {
    return this.on ? items.slice(this.start, this.end) : items;
  }

  /** On the element that holds the rows. */
  list = (node: HTMLElement) => {
    // A list that scrolls itself (an asset column) is its own first clip, and its rows move by its scroll.
    const style = getComputedStyle(node);
    const own = this.#options.scrolls;
    const clips = own ? [node, ...clipsOf(node)] : clipsOf(node);
    this.#clips = clips;
    // The list's own padding: row 0 sits under it, and the room is inside it.
    const base = {
      top: parseFloat(style.paddingTop) || 0,
      bottom: parseFloat(style.paddingBottom) || 0,
    };
    // The browser's own scroll anchoring would correct a row's height a second time.
    $effect(() => {
      node.style.overflowAnchor = this.on ? "none" : "";
    });
    // A flex column's room is its own padding. A table's is its spacer rows
    // (`room`), and so is a list's that scrolls itself: a scroller is never
    // smaller than its padding, so padded it would grow to the whole list.
    if (!(node instanceof HTMLTableSectionElement) && !own)
      $effect(() => {
        pad(node, "padding-top", this.before && base.top + this.before);
        pad(node, "padding-bottom", this.after && base.bottom + this.after);
      });
    // A table sizes its columns from the rows it draws, so drawing others
    // would re-wrap the rows on screen, and that changes which rows draw.
    // While windowed, its columns keep the widths the first drawn rows gave.
    const body = node instanceof HTMLTableSectionElement ? node : null;
    const table = body?.closest("table") ?? null;
    // A header cell added or removed (the view's columns changed) freezes the widths again.
    let heading = $state(0);
    const headings = table?.tHead
      ? new MutationObserver(() => heading++)
      : null;
    if (table?.tHead)
      headings?.observe(table.tHead, { childList: true, subtree: true });
    if (table !== null && body !== null)
      $effect(() => {
        void heading;
        if (!this.on) return;
        const width = table.getBoundingClientRect().width;
        const heads = [...(table.tHead?.rows[0]?.cells ?? [])];
        if (width === 0) return;
        if (heads.length === 0) return freezeCols(table, body, width);
        const shares = heads.map(
          (head) => (head.getBoundingClientRect().width / width) * 100,
        );
        heads.forEach((head, i) => (head.style.width = `${shares[i]}%`));
        table.style.tableLayout = "fixed";
        return () => {
          for (const head of heads) head.style.width = "";
          table.style.tableLayout = "";
        };
      });
    // What of the list is on screen: the window cut by every ancestor that
    // clips, so a layout whose scrolling box changes with the width (Tickets
    // below 900 px) needs nothing from the view.
    // Where row 0 would sit: under the list's padding, moved up by its own scroll.
    const origin = () =>
      node.getBoundingClientRect().top +
      base.top +
      (own ? node.clientTop - node.scrollTop : 0);
    this.#origin = origin;
    const read = (event?: Event) => {
      let top = 0;
      let bottom = window.innerHeight;
      for (const clip of clips) {
        const box = clip.getBoundingClientRect();
        top = Math.max(top, box.top + clip.clientTop);
        bottom = Math.min(bottom, box.top + clip.clientTop + clip.clientHeight);
      }
      // Below 0 while the list starts lower than the screen's top: a list far
      // under the screen then draws one row, not a screenful (#2269: forty
      // diff cards, each its own list).
      // Clipped away whole (a card of its own, below or above the screen):
      // the screen's edge nearest the list stands for the view, so nothing of
      // a list far off is drawn but its one row.
      this.#viewTop = (bottom < top ? bottom : top) - origin();
      this.#viewHeight = Math.max(0, bottom - top);
      // The reader scrolled: hold the row on screen now. Not on a scroll this
      // list made to hold a row, nor on a resize: those arrive after a redraw
      // may have moved the rows, and pinning them there would keep the move
      // (#2211: a summary 5 px under its estimate shifted the list 5 px).
      if (event?.type === "scroll" && !this.#selfScroll(event))
        this.#pin(top, bottom, event);
    };
    read();
    // Scroll events do not bubble; a capturing listener hears every box's.
    document.addEventListener("scroll", read, { capture: true, passive: true });
    window.addEventListener("resize", read, { passive: true });
    const resized =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => read());
    resized?.observe(node);
    for (const clip of clips) resized?.observe(clip);
    return () => {
      document.removeEventListener("scroll", read, { capture: true });
      window.removeEventListener("resize", read);
      resized?.disconnect();
      headings?.disconnect();
      if (this.#clips === clips) this.#clips = [];
    };
  };

  /**
   * On a spacer (a table's cell, or a row of a list that scrolls itself): the
   * room of the undrawn rows above (`before`) or below (`after`), less the
   * gap the list puts beside the spacer itself.
   */
  room = (side: "before" | "after") => (node: HTMLElement) => {
    $effect(() =>
      pad(node, "height", this[side] && this[side] - this.#options.gap),
    );
  };

  /** On each drawn row, with its key. */
  row = (key: K) => (node: HTMLElement) => {
    this.#keyOf.set(node, key);
    this.#drawn.add(node);
    this.#sizes?.observe(node);
    return () => {
      this.#drawn.delete(node);
      this.#sizes?.unobserve(node);
    };
  };

  /** Brings row `index` into the drawn rows, scrolling as little as it can. */
  reveal(index: number) {
    if (!this.on || index < 0 || index >= this.#keys.length) return;
    this.#sync(this.#keys);
    const top = this.#heights.reveal(index, this.#viewTop, this.#viewHeight);
    if (top === this.#viewTop) return;
    // The reader moved on purpose: nothing on screen is held where it was.
    shared.anchor = null;
    this.#scrollBy(top - this.#viewTop);
    this.#viewTop = top;
  }

  /** Row `index`'s top on screen (client pixels), drawn or not: where a line to an undrawn row points. */
  topOf(index: number): number {
    this.#sync(this.#keys);
    return this.#origin() + this.#heights.offsetOf(index);
  }

  /**
   * Holds the first drawn row on screen (`top` to `bottom`, client pixels)
   * and where it is: this list's, where it is higher than the one another
   * list took at the same scroll.
   */
  #pin(top: number, bottom: number, event: Event) {
    if (shared.pinnedAt !== event) {
      shared.pinnedAt = event;
      shared.anchor = null;
    }
    const scroller = this.#scroller();
    for (const el of this.#drawn) {
      const box = el.getBoundingClientRect();
      if (box.bottom > top && box.top < bottom && (shared.anchor === null || box.top < shared.anchor.y))
        shared.anchor = { el, y: box.top, box: scroller, scroll: scroller?.scrollTop ?? 0 };
    }
  }

  /** The nearest box that has something to scroll, else the page. */
  #scroller(): Element | null {
    // jsdom has no `scrollingElement`.
    return (
      this.#clips.find((clip) => clip.scrollHeight > clip.clientHeight) ??
      document.scrollingElement
    );
  }

  /** Scrolls {@link #scroller}. */
  #scrollBy(px: number) {
    const box = this.#scroller();
    if (!box) return;
    box.scrollTop += px;
    shared.scrolled = { box, top: box.scrollTop };
  }

  /** Whether `event` is the scroll a list raised to hold the anchor ({@link #scrollBy}), and only that. */
  #selfScroll(event: Event): boolean {
    if (shared.ownEvent === event) return true;
    const own = shared.scrolled;
    const box = event.target === document ? document.scrollingElement : event.target;
    if (own === null || box !== own.box) return false;
    shared.scrolled = null;
    if (own.box.scrollTop !== own.top) return false;
    shared.ownEvent = event;
    return true;
  }

  /** Hands the rows to the arithmetic when they changed (a new array), not on every scroll. */
  #sync(keys: readonly K[]) {
    if (keys === this.#synced) return;
    this.#synced = keys;
    this.#heights.keys(keys);
  }

  #measure(entries: ResizeObserverEntry[]) {
    let moved = false;
    for (const entry of entries) {
      const key = this.#keyOf.get(entry.target);
      const px =
        entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      if (key !== undefined && px !== 0 && this.#heights.set(key, px))
        moved = true;
    }
    // A row drawn above the screen at other than its estimate, or a table
    // whose columns widen for a new row, moves what the reader is looking
    // at; scroll it back to where it was before the rows changed. WebKit
    // has no CSS scroll anchoring to do this.
    const anchor = shared.anchor;
    // Only a row this list's scroller moves, and one still laid out: a row
    // held on a page keep-alive hid since (`[hidden]`, no box) or in another
    // scroller is no measure of what this list's redraw moved.
    if (
      this.on &&
      anchor !== null &&
      anchor.el.isConnected &&
      anchor.box === this.#scroller() &&
      anchor.el.closest("[hidden]") === null
    ) {
      // A scroll since the anchor was taken that no scroll event has told it
      // of yet (one set in a frame callback lands after that frame's scroll
      // events) moved the row too, and is not the redraw's to undo.
      const since = (anchor.box?.scrollTop ?? 0) - anchor.scroll;
      const off = anchor.el.getBoundingClientRect().top - (anchor.y - since);
      if (Math.abs(off) >= 0.5) {
        this.#scrollBy(off);
        this.#viewTop += off;
      }
      anchor.y = anchor.el.getBoundingClientRect().top;
      anchor.scroll = anchor.box?.scrollTop ?? 0;
    }
    if (moved) this.#remeasured++;
  }
}

/**
 * A table with no header row: its first drawn row's cells give the widths,
 * held by a `colgroup` while it windows (#2210). A first row with a spanning
 * cell gives none, and the columns stay sized by the drawn rows.
 */
function freezeCols(
  table: HTMLTableElement,
  body: HTMLTableSectionElement,
  width: number,
) {
  const first = [...body.rows].find((tr) => !tr.classList.contains("tf-wgap"));
  const cells = [...(first?.cells ?? [])];
  if (cells.length === 0 || cells.some((cell) => cell.colSpan > 1)) return;
  const cols = document.createElement("colgroup");
  for (const cell of cells)
    cols.appendChild(document.createElement("col")).style.width =
      `${(cell.getBoundingClientRect().width / width) * 100}%`;
  table.insertBefore(cols, table.caption?.nextSibling ?? table.firstChild);
  table.style.tableLayout = "fixed";
  return () => {
    cols.remove();
    table.style.tableLayout = "";
  };
}

/** Sets `px` of room through the CSSOM (CSP refuses inline styles in markup), none when 0. */
function pad(node: HTMLElement, property: string, px: number) {
  if (px > 0) node.style.setProperty(property, `${px}px`);
  else node.style.removeProperty(property);
}

/** The ancestors whose overflow clips `node`, nearest first. */
function clipsOf(node: HTMLElement): HTMLElement[] {
  const clips: HTMLElement[] = [];
  for (
    let at = node.parentElement;
    at !== null && at !== document.body;
    at = at.parentElement
  )
    if (getComputedStyle(at).overflowY !== "visible") clips.push(at);
  return clips;
}
