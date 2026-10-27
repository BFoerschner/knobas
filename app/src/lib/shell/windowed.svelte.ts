/**
 * A windowed list (#2209, ruling on #2203: window, don't cap): a long list
 * keeps every row it holds, in order, but only the rows in or near the
 * viewport are in the DOM; the rest are room above and below.
 *
 * ```svelte
 * const win = new Windowed({ count: () => rows.length, key: (i) => rows[i].id, estimate: 63, gap: 6 });
 *
 * <div class="list" {@attach win.list} style:padding-top="{win.before}px" style:padding-bottom="{win.after}px">
 *   {#each win.slice(rows) as row (row.id)}<div {@attach win.row(row.id)}>…</div>{/each}
 * </div>
 * ```
 *
 * - `list` goes on the element that holds the rows (a `tbody` too). It finds
 *   the nearest scrolling ancestor itself, so the view's layout decides.
 * - `row(key)` goes on each drawn row: its height is measured, the estimate
 *   stands for rows never drawn, and a row measured above the viewport moves
 *   the scroll by its error, so scrolling up never jumps.
 * - `before`/`after` are the undrawn rows' room: padding on a flex column
 *   (`gap` is its gap), or spacer rows in a table (`gap` 0; draw a spacer
 *   only when its room is above 0, so `:last-child` rules hold).
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
  readonly #o: Required<WindowedOptions<K>>;
  readonly #h: Heights<K>;
  /** The viewport, in the list's pixels: its top, and its height. */
  #top = $state(0);
  #height = $state(typeof window === "undefined" ? 0 : window.innerHeight);
  /** Bumped when a measurement moves a row. */
  #measured = $state(0);
  /** The list's ancestors that clip it, nearest first. */
  #clips: HTMLElement[] = [];
  #synced: readonly K[] | null = null;
  readonly #keyOf = new WeakMap<Element, K>();
  readonly #rows: ResizeObserver | null;

  readonly #keys = $derived.by(() => {
    const n = this.#o.count();
    const keys = new Array<K>(n);
    for (let i = 0; i < n; i++) keys[i] = this.#o.key(i);
    return keys;
  });

  readonly #span = $derived.by((): Span & { on: boolean } => {
    const keys = this.#keys;
    if (keys.length < this.#o.from) return { ...ALL, end: keys.length, on: false };
    void this.#measured;
    this.#sync(keys);
    return { ...this.#h.span(this.#top, this.#height, this.#o.overscan), on: true };
  });

  /** Whether the list is long enough to window. */
  readonly on = $derived(this.#span.on);
  readonly start = $derived(this.#span.start);
  readonly end = $derived(this.#span.end);
  readonly before = $derived(this.#span.before);
  readonly after = $derived(this.#span.after);

  constructor(options: WindowedOptions<K>) {
    this.#o = { gap: 0, overscan: 600, from: 300, ...options };
    this.#h = new Heights<K>(this.#o.estimate, this.#o.gap);
    this.#rows = typeof ResizeObserver === "undefined" ? null : new ResizeObserver((entries) => this.#measure(entries));
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
      this.#top = Math.max(0, top - node.getBoundingClientRect().top);
      this.#height = Math.max(0, bottom - top);
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

  /** On each drawn row, with its key. */
  row = (key: K) => (node: HTMLElement) => {
    this.#keyOf.set(node, key);
    this.#rows?.observe(node);
    return () => this.#rows?.unobserve(node);
  };

  /** Brings row `index` into the drawn rows, scrolling as little as it can. */
  reveal(index: number) {
    if (!this.on || index < 0 || index >= this.#keys.length) return;
    this.#sync(this.#keys);
    const top = this.#h.reveal(index, this.#top, this.#height);
    if (top === this.#top) return;
    this.#scrollBy(top - this.#top);
    this.#top = top;
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
    this.#h.keys(keys);
  }

  #measure(entries: ResizeObserverEntry[]) {
    let shift = 0;
    let moved = false;
    for (const entry of entries) {
      const key = this.#keyOf.get(entry.target);
      const px = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      if (key === undefined || px === 0) continue;
      const was = this.#h.heightOf(key);
      const at = this.on ? this.#h.indexOf(key) : -1;
      const above = at >= 0 && this.#h.offsetOf(at) < this.#top;
      if (!this.#h.set(key, px)) continue;
      moved = true;
      if (above) shift += px - was;
    }
    if (!moved) return;
    // A row above the viewport drawn taller or shorter than its estimate
    // would push what is on screen; scroll by the difference instead.
    if (shift !== 0) {
      this.#scrollBy(shift);
      this.#top += shift;
    }
    this.#measured++;
  }
}

/** The ancestors whose overflow clips `node`, nearest first. */
function clipsOf(node: HTMLElement): HTMLElement[] {
  const clips: HTMLElement[] = [];
  for (let at = node.parentElement; at !== null && at !== document.body; at = at.parentElement)
    if (getComputedStyle(at).overflowY !== "visible") clips.push(at);
  return clips;
}
