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
 *   sized by whichever rows are drawn would re-wrap the rows on screen.
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
}

const ALL: Span = { start: 0, end: 0, before: 0, after: 0 };

export class Windowed<K = string> {
  readonly #options: Required<WindowedOptions<K>>;
  readonly #heights: Heights<K>;
  /** The viewport, in the list's pixels: its top, and its height. */
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
  /** A drawn row on screen and its top, taken at each scroll, held still through the redraw it causes. */
  #anchor: { el: HTMLElement; y: number } | null = null;
  readonly #sizes: ResizeObserver | null;

  readonly #keys = $derived.by(() => {
    const n = this.#options.count();
    const keys = new Array<K>(n);
    for (let i = 0; i < n; i++) keys[i] = this.#options.key(i);
    return keys;
  });

  readonly #span = $derived.by((): Span & { on: boolean } => {
    const keys = this.#keys;
    if (keys.length < this.#options.from) return { ...ALL, end: keys.length, on: false };
    void this.#remeasured;
    this.#sync(keys);
    return { ...this.#heights.span(this.#viewTop, this.#viewHeight, this.#options.overscan), on: true };
  });

  /** Whether the list is long enough to window. */
  readonly on = $derived(this.#span.on);
  readonly start = $derived(this.#span.start);
  readonly end = $derived(this.#span.end);
  readonly before = $derived(this.#span.before);
  readonly after = $derived(this.#span.after);

  constructor(options: WindowedOptions<K>) {
    this.#options = { gap: 0, overscan: 600, from: 300, ...options };
    this.#heights = new Heights<K>(this.#options.estimate, this.#options.gap);
    this.#sizes = typeof ResizeObserver === "undefined" ? null : new ResizeObserver((entries) => this.#measure(entries));
  }

  /** The rows to draw out of `items` (the list's rows, in the same order). */
  slice<T>(items: readonly T[]): readonly T[] {
    return this.on ? items.slice(this.start, this.end) : items;
  }

  /** On the element that holds the rows. */
  list = (node: HTMLElement) => {
    const clips = clipsOf(node);
    this.#clips = clips;
    // The browser's own scroll anchoring would correct a row's height a second time.
    $effect(() => {
      node.style.overflowAnchor = this.on ? "none" : "";
    });
    // A flex column's room is its own padding; a table's is its spacer rows (`room`).
    if (!(node instanceof HTMLTableSectionElement))
      $effect(() => {
        pad(node, "padding-top", this.before);
        pad(node, "padding-bottom", this.after);
      });
    // A table sizes its columns from the rows it draws, so drawing others
    // would re-wrap the rows on screen, and that changes which rows draw.
    // While windowed, its columns keep the widths the first drawn rows gave.
    const table = node instanceof HTMLTableSectionElement ? node.closest("table") : null;
    if (table !== null)
      $effect(() => {
        if (!this.on) return;
        const width = table.getBoundingClientRect().width;
        const heads = [...(table.tHead?.rows[0]?.cells ?? [])];
        if (width === 0 || heads.length === 0) return;
        const shares = heads.map((head) => (head.getBoundingClientRect().width / width) * 100);
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
    const read = () => {
      let top = 0;
      let bottom = window.innerHeight;
      for (const clip of clips) {
        const box = clip.getBoundingClientRect();
        top = Math.max(top, box.top + clip.clientTop);
        bottom = Math.min(bottom, box.top + clip.clientTop + clip.clientHeight);
      }
      this.#viewTop = Math.max(0, top - node.getBoundingClientRect().top);
      this.#viewHeight = Math.max(0, bottom - top);
      this.#pin(top);
    };
    read();
    // Scroll events do not bubble; a capturing listener hears every box's.
    document.addEventListener("scroll", read, { capture: true, passive: true });
    window.addEventListener("resize", read, { passive: true });
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(read);
    resized?.observe(node);
    for (const clip of clips) resized?.observe(clip);
    return () => {
      document.removeEventListener("scroll", read, { capture: true });
      window.removeEventListener("resize", read);
      resized?.disconnect();
      if (this.#clips === clips) this.#clips = [];
    };
  };

  /** On a table's spacer cell: the room of the undrawn rows above (`before`) or below (`after`). */
  room = (side: "before" | "after") => (node: HTMLElement) => {
    $effect(() => pad(node, "height", this[side]));
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
    this.#anchor = null;
    this.#scrollBy(top - this.#viewTop);
    this.#viewTop = top;
  }

  /** Holds the first drawn row on screen at `top` (client pixels) and where it is. */
  #pin(top: number) {
    let best: { el: HTMLElement; y: number } | null = null;
    for (const el of this.#drawn) {
      const box = el.getBoundingClientRect();
      if (box.bottom > top && (best === null || box.top < best.y)) best = { el, y: box.top };
    }
    this.#anchor = best;
  }

  /** Scrolls the nearest box that has something to scroll, else the page. */
  #scrollBy(px: number) {
    const box = this.#clips.find((clip) => clip.scrollHeight > clip.clientHeight);
    if (box !== undefined) box.scrollTop += px;
    // jsdom has no `scrollingElement`.
    else if (document.scrollingElement) document.scrollingElement.scrollTop += px;
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
      const px = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      if (key !== undefined && px !== 0 && this.#heights.set(key, px)) moved = true;
    }
    // A row drawn above the screen at other than its estimate, or a table
    // whose columns widen for a new row, moves what the reader is looking
    // at; scroll it back to where it was before the rows changed. WebKit
    // has no CSS scroll anchoring to do this.
    const anchor = this.#anchor;
    if (this.on && anchor !== null && anchor.el.isConnected) {
      const off = anchor.el.getBoundingClientRect().top - anchor.y;
      if (Math.abs(off) >= 0.5) {
        this.#scrollBy(off);
        this.#viewTop += off;
      }
    }
    if (moved) this.#remeasured++;
  }
}

/** Sets `px` of room through the CSSOM (CSP refuses inline styles in markup), none when 0. */
function pad(node: HTMLElement, property: string, px: number) {
  if (px > 0) node.style.setProperty(property, `${px}px`);
  else node.style.removeProperty(property);
}

/** The ancestors whose overflow clips `node`, nearest first. */
function clipsOf(node: HTMLElement): HTMLElement[] {
  const clips: HTMLElement[] = [];
  for (let at = node.parentElement; at !== null && at !== document.body; at = at.parentElement)
    if (getComputedStyle(at).overflowY !== "visible") clips.push(at);
  return clips;
}
