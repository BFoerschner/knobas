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
 * - Heights and room are kept on the device's layout grid (`onGrid`,
 *   `gridLength` in `windowed.ts`), so a row swap moves none of the rows that
 *   stay, at any display scale: a row moved by a fraction of a pixel is
 *   painted again, and at 125 % every swap repainted the whole table (#2508).
 *   A change of that scale (a zoom step, `tf/zoom.svelte.ts`, or a screen of
 *   another scale) re-reads the drawn rows once, puts the heights held for
 *   undrawn rows on the new grid and writes the room again (`device-scale`).
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
 * - A list reads where it sits only once it windows, and in the same task as
 *   every other list attached in that update (`placeWindowsNow` reads them at
 *   once, as `reveal` and `topOf` do first), so a page's first open lays itself
 *   out once for all its lists. Until then a list draws its overscan's rows.
 * - The read draws the rows on screen, the overscan above them and a quarter
 *   of the view below; the rest of the overscan below waits until two frames
 *   have drawn after the last list was placed (`growSoon`), so a page's first
 *   open builds about what the screen shows (the sizes pass, 2026-11-15: the
 *   Inbox's and Monitors' first frames built 600 px of rows below the fold).
 *   Rows drawn below the view move nothing on it.
 *
 * A folding list windows its flattened visible rows (headers of open folds
 * and their rows) with one key per row.
 */
import { untrack } from "svelte";
import { onScaleChange } from "./device-scale.svelte";
import { Heights, gridLength, onGrid, STEPS, type Span } from "./windowed";

export interface WindowedOptions<K> {
  /** How many rows the list holds. */
  count: () => number;
  /** Row `index`'s key: unique, stable while the row is the same row. */
  key: (index: number) => K;
  /** A row's height before it is drawn, in pixels. */
  estimate: number;
  /**
   * Row `key`'s own height before it is drawn, where rows differ by far (a comment of a line and one of a page, a
   * file of three changed lines and one of three hundred): the room an undrawn row holds is nearer its own, so the
   * scrollbar and a jump to it land closer. `estimate` stands where it is not given.
   */
  estimateOf?: ((key: K) => number) | null;
  /** The space between rows (a flex column's `gap`), in pixels. */
  gap?: number;
  /** How many pixels beyond the viewport to draw each side. */
  overscan?: number;
  /**
   * How near the view may come to the edge of the drawn rows before they change (default half the overscan). The
   * drawn rows hold still while the view moves inside them, then the window moves to the view and overscan again,
   * so a scroll changes rows a few at a time every `overscan - margin` pixels, not on most frames (PR #2479: a row
   * change on two frames of three cost WKWebView a third of its frames at 120 Hz, and each a main-thread frame in
   * Chromium).
   */
  margin?: number;
  /** Lists shorter than this draw every row. */
  from?: number;
  /**
   * The list scrolls itself (an asset column), so its rows move by its own
   * scroll. Not read from `overflow-y`: a list that clips only sideways (the
   * run log's `overflow: auto`) computes `auto` too, yet grows with its rows.
   */
  scrolls?: boolean;
  /**
   * Each new set of rows draws the rows on screen and one past them first, and the overscan below two frames later,
   * as a list's first read does, but without its quarter of a view more or the rows drawn before it (the ux14 pass,
   * 2026-11-16). A set is new when the list starts to window or a row it drew is no longer where it was (more rows
   * after them, as *Show more* brings, are the same set): a board column's letter, whose
   * first frame drew the whole overscan below the view, 32 cards at 5120 x 1440 where 26 were on screen. Only for a
   * list whose `estimateOf` never reckons a row taller than it draws, and whose rows below the view are clipped from
   * sight: a tall reckoning would leave the view's foot empty for a frame.
   */
  onScreen?: boolean;
  /**
   * For a list whose rows are lines of text (a run's log, a file's source, a diff's lines): hears that text's size
   * change (Settings › Fonts, `fonts.svelte.ts`'s `onCodeSize`), with the new size over the old. The list then takes
   * every height it holds and its estimate times that ratio, onto the device's grid, and reads its drawn rows again, as
   * for a zoom step: a row is a line box, so the ratio is exact, and the scrollbar and a jump land where they will.
   * Other lists keep what they hold: their single-line rows did not change, and a wrapped row is put right when drawn.
   */
  textSize?: ((heard: (ratio: number) => void) => () => void) | null;
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
  /**
   * The viewport, in the list's pixels: its top (negative while the list starts below it), and its height. None until
   * `list` reads it: a list half way down the screen (a pipeline's runs under its chart) that first drew a screenful
   * from the top held fifteen rows more than it shows, since rows that cover the view stay (the Windows pass,
   * 2026-11-14). The first draw is the overscan's rows, and the read in the same flush draws the rest.
   */
  #viewTop = $state(0);
  #viewHeight = $state(0);
  /** Bumped when a measurement moves a row, so the span is worked out again. */
  #remeasured = $state(0);
  /** Bumped when the device's scale changes, so the room is written again on the new grid. */
  #grid = $state(0);
  /** The list's ancestors that clip it, nearest first. */
  #clips: HTMLElement[] = [];
  #synced: readonly K[] | null = null;
  readonly #keyOf = new WeakMap<Element, K>();
  /** The rows in the DOM now. */
  readonly #drawn = new Set<HTMLElement>();
  /** Where row 0 sits on screen, once `list` is attached. */
  #origin = () => 0;
  /** The element `list` is on. */
  #listNode: HTMLElement | null = null;
  /** The row that holds focus, and which of its focusable elements: what a row scrolled away gives focus back to. */
  #focused: { key: K; at: number } | null = null;
  /** Focus the list holds for a row the window took out (`row`), until that row is drawn again. */
  #holding: { key: K; at: number } | null = null;
  readonly #sizes: ResizeObserver | null;

  readonly #keys = $derived.by(() => {
    const n = this.#options.count();
    const keys = new Array<K>(n);
    for (let i = 0; i < n; i++) keys[i] = this.#options.key(i);
    return (this.#workedOut = keys);
  });
  /**
   * The keys `#keys` last worked out, as a plain value, for a row's teardown (`row`), or `null` before it has. A teardown
   * runs while Svelte destroys effects, and there a derived read whose sources changed in the batch runs again over their
   * old values: a focused card dropped in another column read the window's keys as its row went, which ran the column's
   * cards and through them the board's whole read (overlay, bands, held places) once more in the drop's task, and that
   * run, over the old echoes, left the overlay's kept answer stale, so the next pass laid the board over again (the ux30
   * end gate, 2026-11-23: four passes a laned drop in knobas's WebView2 window).
   */
  #workedOut: readonly K[] | null = null;

  /** The rows drawn last, while the view stays inside them with the margin to spare. */
  #held: { keys: readonly K[]; start: number; end: number } | null = null;
  /** Whether `list` has read where the list sits: rows drawn before that are the overscan's, and none are held. */
  #placed = false;
  /**
   * Whether the overscan below the view is drawn too: from two frames after the last list read where it sits, for the
   * set of rows (`#set`) it was drawn for.
   */
  #grownFor = $state(-1);
  /** The set of rows drawn: moved on, for an `onScreen` list, when it starts to window or its first row changes. */
  #set = 0;
  /** The set a grow was asked for. */
  #growAsked = -1;
  /** Whether the list windowed, and the rows it drew where, when the span was last worked out (`onScreen`). */
  #lastOn = false;
  #lastKeys: readonly K[] | null = null;
  #lastDrawn: { start: number; keys: readonly K[] } = { start: 0, keys: [] };
  /** The last row drawn before the read: the read keeps those under the view that the overscan below would hold. */
  #early: { key: K } | null = null;
  /**
   * A jump's first frames (`land`): how many pixels at the view's top a band covers. Until two frames have drawn after
   * it, the list draws the rows on screen below that band and nothing else; then its overscan both ways.
   */
  #landing = $state.raw<{ covered: number; key: K } | null>(null);
  /**
   * The jump this list was attached in, if one was landing (`jumping`): a file's lines, drawn for the first time as the
   * jump brings its card. Until it has read where it sits it draws no row and holds their room; then the rows on screen
   * alone, until the jump's frames have drawn and its overscan grows with the files'.
   */
  #bornIn: object | null = null;

  readonly #span = $derived.by((): Span & { on: boolean } => {
    const keys = this.#keys;
    if (keys.length < this.#options.from) {
      this.#held = null;
      this.#lastOn = false;
      return { ...ALL, end: keys.length, on: false };
    }
    void this.#remeasured;
    if (this.#options.onScreen && this.#placed && keys !== this.#lastKeys) {
      const { start, keys: drew } = this.#lastDrawn;
      if (!this.#lastOn || drew.some((key, at) => keys[start + at] !== key)) this.#set += 1;
    }
    this.#lastOn = true;
    this.#lastKeys = keys;
    // Read before the held rows may answer, so the overscan below is drawn when it may be.
    const grown = this.#grownFor === this.#set;
    this.#sync(keys);
    // A list a jump brought: no row before its read, the rows on screen after it, the overscan once the jump's frames
    // have drawn. Drawn before the read, its overscan's rows from its top were the wrong ones for a card whose foot is
    // what shows, and the read then drew a screen and the overscan above it besides (the compare-jump-speed pass).
    if (this.#bornIn !== null && this.#bornIn === jumping) {
      this.#held = null;
      // The view is read before the list is placed too: its read is what draws its rows.
      const [top, height] = [this.#viewTop, this.#viewHeight];
      const span = this.#placed ? this.#heights.span(top, height, 0, 0) : this.#heights.around(0, 0);
      if (this.#options.onScreen) this.#lastDrawn = { start: span.start, keys: keys.slice(span.start, span.end) };
      return { ...span, on: true };
    }
    // A jump's first frames: the rows on screen under the band alone (`land`), the overscan once they have drawn.
    const landing = this.#landing;
    if (landing !== null) {
      this.#held = null;
      let span = this.#heights.span(this.#viewTop + landing.covered, Math.max(0, this.#viewHeight - landing.covered), 0, 0);
      // The row landed on stays drawn whatever the view reads: the caller holds the page by it.
      const at = keys.indexOf(landing.key);
      // Nor is a row above it drawn while its top stands at the band's foot: those lie under the band. A scroll rounds
      // to whole pixels, so the view's top fell a fraction of a pixel above a row's fractional top, and the row before it
      // was drawn for that sliver: a big file, whose lines then laid out short, put the landed file a file's height up
      // and the jump built a screen of that file before it came back (the ux21 overs pass, 300 to 600 ms at CPU 6x).
      if (at > span.start && this.#heights.offsetOf(at) <= this.#viewTop + landing.covered + 1)
        span = this.#heights.around(at, Math.max(at + 1, span.end));
      if (at >= 0 && (at < span.start || at >= span.end)) span = this.#heights.around(Math.min(at, span.start), Math.max(at + 1, span.end));
      if (this.#options.onScreen) this.#lastDrawn = { start: span.start, keys: keys.slice(span.start, span.end) };
      return { ...span, on: true };
    }
    const { overscan, margin } = this.#options;
    const held = this.#held;
    if (held !== null && held.keys === keys && this.#heights.covers(held.start, held.end, this.#viewTop, this.#viewHeight, margin))
      return { ...this.#heights.around(held.start, held.end), on: true };
    // Before the read, the overscan's rows from the top; after it, the view and the overscan above it, and once two
    // frames have drawn, the overscan below too. Until then a quarter of the view's height below it, and a row past
    // that: rows not yet measured stand at their estimate, which runs a sixth over the Inbox's, so a span of the view
    // alone fell short of the screen's bottom for a frame; and the last drawn row stays off screen, where a list's
    // `:last-child` rule (the Inbox's border) changes no row in view. The read takes away none of the rows drawn before
    // it that the overscan would hold: a row under a clipping footer stays where it drew.
    const fits = this.#options.onScreen;
    // An on-screen list before its read: the most rows that could be on screen, a window's height of them from its top,
    // and one past, not its overscan's. The read then finds them drawn, or takes the few below its view away. Drawn with
    // the overscan, a board's columns built cards the read changed again in the same task, and an asset's column 1800 px
    // of rows where a window shows 900 (the ux30 end gate, Windows: the board of every ticket and a long asset column).
    // Not once something moved its view first (`reveal` of a row past them, a key's walk): that view's rows are drawn.
    if (fits && !this.#placed && this.#viewTop === 0) {
      const screen = this.#heights.span(0, typeof window === "undefined" ? 0 : window.innerHeight, 0, 0);
      const span = this.#heights.around(screen.start, Math.min(keys.length, screen.end + 1));
      this.#lastDrawn = { start: span.start, keys: keys.slice(span.start, span.end) };
      return { ...span, on: true };
    }
    const below = this.#placed && !grown ? (fits ? 0 : Math.min(overscan, this.#viewHeight / 4)) : overscan;
    let span = this.#heights.span(this.#viewTop, this.#viewHeight, overscan, below);
    if (this.#placed && !grown) {
      const full = this.#heights.span(this.#viewTop, this.#viewHeight, overscan);
      const early = this.#early === null || fits ? -1 : keys.indexOf(this.#early.key);
      const end = Math.min(full.end, Math.max(span.end + 1, early + 1));
      if (end > span.end) span = this.#heights.around(span.start, end);
    }
    if (!this.#placed && span.end > span.start) this.#early = { key: keys[span.end - 1]! };
    // Held once the overscan below is in: rows held before would cover the view with the margin to spare, and the grow
    // would draw nothing.
    this.#held = this.#placed && grown ? { keys, start: span.start, end: span.end } : null;
    if (this.#options.onScreen) this.#lastDrawn = { start: span.start, keys: keys.slice(span.start, span.end) };
    return { ...span, on: true };
  });

  /** Whether the list is long enough to window. */
  readonly on = $derived(this.#span.on);
  readonly start = $derived(this.#span.start);
  readonly end = $derived(this.#span.end);
  readonly before = $derived(this.#span.before);
  readonly after = $derived(this.#span.after);

  constructor(options: WindowedOptions<K>) {
    const overscan = options.overscan ?? 600;
    this.#options = {
      gap: 0,
      from: 300,
      scrolls: false,
      estimateOf: null,
      onScreen: false,
      textSize: null,
      margin: overscan / 2,
      ...options,
      overscan,
    };
    this.#heights = new Heights<K>(this.#options.estimate, this.#options.gap, this.#options.estimateOf);
    this.#sizes =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver((entries) => this.#measure(entries));
  }

  /** Draws the overscan below the view for `set` once its frame and the one after it have drawn (`growSoon`). */
  #growFor(set: number) {
    if (untrack(() => this.#grownFor) === set || this.#growAsked === set) return;
    this.#growAsked = set;
    // Dropped with the list: asked again if it is attached again.
    this.#growSoon(
      () => (this.#grownFor = set),
      () => {
        if (this.#growAsked === set) this.#growAsked = -1;
      },
    );
  }

  /**
   * The grows this list asked for that have not come yet, each as what drops it. The list's teardown drops them: the
   * grow waits on two frames and then a task, and a list gone in that time left them asking (under jsdom a frame is a
   * 16 ms interval, residue; the XML-view branch's finding, 2026-11-24, the class of e55fa040).
   */
  #grows = new Set<() => void>();

  /** {@link growSoon} for this list: `grow` once the frames have drawn, or `dropped` if the list goes first. */
  #growSoon(grow: () => void, dropped: () => void) {
    const unask = growSoon(() => {
      this.#grows.delete(drop);
      grow();
    });
    const drop = () => {
      this.#grows.delete(drop);
      unask();
      dropped();
    };
    this.#grows.add(drop);
  }

  /** The rows to draw out of `items` (the list's rows, in the same order). */
  slice<T>(items: readonly T[]): readonly T[] {
    return this.on ? items.slice(this.start, this.end) : items;
  }

  /** On the element that holds the rows. */
  list = (node: HTMLElement) => {
    const own = this.#options.scrolls;
    this.#bornIn = jumping;
    // The ancestors that clip the list, and its own padding (row 0 sits under it, and the room is inside it): read
    // with every other list attached in this flush (`placeSoon`), then the viewport.
    let clips: HTMLElement[] = [];
    const base = $state({ top: 0, bottom: 0, read: false });
    // The browser's own scroll anchoring would correct a row's height a second time.
    $effect(() => {
      node.style.overflowAnchor = this.on ? "none" : "";
    });
    // A flex column's room is its own padding. A table's is its spacer rows
    // (`room`), and so is a list's that scrolls itself: a scroller is never
    // smaller than its padding, so padded it would grow to the whole list.
    if (!(node instanceof HTMLTableSectionElement) && !own)
      $effect(() => {
        // Not before the list's own padding is read, which would then be read with the room in it.
        if (!base.read) return;
        void this.#grid;
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
        const heads = [...(table.tHead?.rows[0]?.cells ?? [])];
        // A table that declares its columns (a `colgroup`) has its widths already, and needs no layout read for them.
        if (heads.length === 0 && table.querySelector(":scope > colgroup")) return;
        // Nor does a fixed table with a header row: its header's cells give its columns, whichever rows are drawn. Read,
        // their widths laid the whole page out as the list began to window, in the task that opens it, and again once
        // written (the ux21 overs pass: Tickets' show-every-ticket list, 105 ms at 1x on Björn's PC).
        if (heads.length > 0 && getComputedStyle(table).tableLayout === "fixed") return;
        const width = table.getBoundingClientRect().width;
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
    /** Whether the list has read a view while its page was shown. */
    let viewed = false;
    const read = (event?: Event) => {
      if (!placed) return;
      // In a page never shown yet (held ahead of its press, `tf/ahead.ts`, so hidden and without a box), a screenful
      // from the list's top: read as the nothing a hidden box measures, the list drew its overscan alone and built its
      // rows again on its first show, about 70 ms of a sixth-speed main thread on Builds (the ux13-overs pass,
      // 2026-11-15). A list shown before still drops to its overscan as its page hides: kept whole, its return laid
      // out every row it had drawn, Builds' at 6x 95 ms where it was 58.
      if (!viewed && node.closest("[hidden]") !== null) {
        this.#viewTop = 0;
        this.#viewHeight = window.innerHeight;
        return;
      }
      viewed = true;
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
      if (event?.type === "scroll" && !this.#selfScroll(event)) {
        // The reader moved on from a jump before its overscan came: the overscan comes now.
        this.#landing = null;
        this.#pin(top, bottom, event);
      }
    };
    // Reading where a list sits lays the page out. A page that opens with several windowed lists (Builds' rail, its
    // days and each day's runs) read each as it was attached, after the one before had drawn its rows: a layout of
    // the whole new page each, six of them and 62 ms of a quarter-speed main thread (the Windows pass, 2026-11-14).
    // Read together, once every list of the flush is attached, they lay it out once.
    const resized =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => read());
    /** Whether the list's place is read (`place`), and whether that read waits for the flush to end. */
    let placed = false;
    let queued = false;
    const place = (phase: Phase) => {
      queued = false;
      if (!node.isConnected) return;
      if (phase === "style") {
        // A list that scrolls itself (an asset column) is its own first clip, and its rows move by its scroll.
        clips = own ? [node, ...clipsOf(node)] : clipsOf(node);
        const style = getComputedStyle(node);
        base.top = parseFloat(style.paddingTop) || 0;
        base.bottom = parseFloat(style.paddingBottom) || 0;
        base.read = true;
        return;
      }
      if (phase === "room") {
        if (!base.read) return;
        // The undrawn rows' room first: a box that clips the list and holds only it (the Inbox's sheet) ends at the rows
        // drawn so far until the room is in, and read before it would take that for the view's bottom.
        if (!(node instanceof HTMLTableSectionElement) && !own) {
          pad(node, "padding-top", this.before && base.top + this.before);
          pad(node, "padding-bottom", this.after && base.bottom + this.after);
        }
        return;
      }
      if (!base.read) return;
      placed = true;
      this.#placed = true;
      this.#clips = clips;
      read();
      this.#growFor(this.#set);
      resized?.observe(node);
      for (const clip of clips) resized?.observe(clip);
    };
    // A new set of rows (`onScreen`) grows its overscan below once its frame has drawn.
    $effect(() => {
      void this.#span;
      if (placed && this.#options.onScreen) this.#growFor(this.#set);
    });
    // A list too short to window draws every row wherever it sits, so it reads nothing until it windows: a first
    // open of a short list lays nothing out for it.
    $effect(() => {
      if (!this.on || placed || queued) return;
      queued = true;
      placeSoon(place);
    });
    // Focus in a row is noted, so a row the window takes out keeps it on the list (`row`) and gets it back when drawn.
    this.#listNode = node;
    const focusIn = (event: FocusEvent) => {
      const row = [...this.#drawn].find((drawn) => drawn.contains(event.target as Node));
      if (row === undefined) return;
      this.#focused = { key: this.#keyOf.get(row)!, at: focusables(row).indexOf(event.target as HTMLElement) };
      this.#holding = null;
      delete node.dataset["heldFocus"];
    };
    const focusOut = (event: FocusEvent) => {
      // Leaving the list (not to the list itself, which is a row taken out): nothing is held for it.
      const to = event.relatedTarget;
      if (to instanceof Node && node.contains(to)) return;
      const from = event.target;
      // Chrome tells a focused row's removal as a `focusout` while the row is still connected: whether it went is
      // asked once this task's DOM work is over.
      queueMicrotask(() => {
        if (from instanceof Node && from.isConnected && document.activeElement !== node) {
          this.#focused = null;
          this.#holding = null;
          delete node.dataset["heldFocus"];
        }
      });
    };
    node.addEventListener("focusin", focusIn);
    node.addEventListener("focusout", focusOut);
    // Scroll events do not bubble; a capturing listener hears every box's.
    document.addEventListener("scroll", read, { capture: true, passive: true });
    window.addEventListener("resize", read, { passive: true });
    const reread = () => read();
    viewReads.add(reread);
    const unscale = onScaleChange(() => this.#rescale());
    const unsize = this.#options.textSize?.((ratio) => this.#resize(ratio)) ?? null;
    return () => {
      unscale();
      unsize?.();
      if (queued) unplaced.delete(place);
      for (const drop of [...this.#grows]) drop();
      node.removeEventListener("focusin", focusIn);
      node.removeEventListener("focusout", focusOut);
      if (this.#listNode === node) this.#listNode = null;
      document.removeEventListener("scroll", read, { capture: true });
      window.removeEventListener("resize", read);
      viewReads.delete(reread);
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
    $effect(() => {
      void this.#grid;
      pad(node, "height", this[side] && this[side] - this.#options.gap);
    });
  };

  /** On each drawn row, with its key. */
  row = (key: K) => (node: HTMLElement) => {
    this.#keyOf.set(node, key);
    this.#drawn.add(node);
    this.#sizes?.observe(node);
    // The row that held focus when the window took it, drawn again while the list holds that focus: it has it back.
    const held = this.#holding;
    if (held !== null && held.key === key && this.#listNode !== null && document.activeElement === this.#listNode) {
      const list = this.#listNode;
      queueMicrotask(() => {
        if (this.#holding !== held || document.activeElement !== list || !node.isConnected) return;
        (focusables(node)[held.at] ?? focusables(node)[0] ?? node).focus({ preventScroll: true });
      });
    }
    return () => {
      this.#drawn.delete(node);
      this.#sizes?.unobserve(node);
      // A row the window takes out while it holds focus (scrolled far off): the list holds focus for it, so keys and
      // the page's place stay, and the row gets it back when it is drawn again. Chrome fixes focus up only at its next
      // rendering update, so the row may still be the active element here; jsdom has dropped it to the body already.
      const focused = this.#focused;
      const list = this.#listNode;
      const active = document.activeElement;
      if (focused === null || focused.key !== key || list === null || !list.isConnected) return;
      if (!(active === document.body || active === null || node.contains(active))) return;
      // Only a row the window took out: one the list no longer holds (a comment box closed, a filter, a card dropped in
      // another column) is gone for good. Asked first of the keys last worked out, which reads nothing reactive.
      if (this.#workedOut !== null && !this.#workedOut.includes(key)) return;
      const at = this.#keys.indexOf(key);
      if (at < 0 || (at >= this.start && at < this.end)) return;
      if (!list.hasAttribute("tabindex")) list.tabIndex = -1;
      list.dataset["heldFocus"] = "";
      this.#holding = focused;
      list.focus({ preventScroll: true });
    };
  };

  /** Brings row `index` into the drawn rows, scrolling as little as it can. */
  reveal(index: number) {
    placeWindowsNow();
    if (!this.on || index < 0 || index >= this.#keys.length) return;
    this.#sync(this.#keys);
    const top = this.#heights.reveal(index, this.#viewTop, this.#viewHeight);
    if (top === this.#viewTop) return;
    // The reader moved on purpose: nothing on screen is held where it was.
    shared.anchor = null;
    this.#scrollBy(top - this.#viewTop);
    this.#viewTop = top;
  }

  /**
   * A jump to row `index` (a diff's file picked in its list, the compare-jump-speed pass, 2026-11-18): the view moves
   * straight to where the row's top stands `covered` pixels below its top (under a band that sticks there), and until two
   * frames have drawn after it the list draws only the rows on screen under that band; the overscan above and below
   * comes in a task of its own after (`growSoon`), held still by the row the caller lands on (`landOn`). `reveal` first
   * brought the row in at the view's foot and drew the overscan around that place, then the landing moved the view a
   * screen down and drew the rows there too: a jump built two screens of files and their lines and threw one away.
   * False where the list draws every row (too short to window), or has no row `index`.
   */
  land(index: number, covered = 0): boolean {
    placeWindowsNow();
    if (!this.on || index < 0 || index >= this.#keys.length) return false;
    this.#sync(this.#keys);
    // The reader moved on purpose: nothing on screen is held where it was.
    shared.anchor = null;
    const box = this.#scroller();
    const want = this.#heights.offsetOf(index) - covered - this.#viewTop;
    if (box === null) this.#viewTop += want;
    else if (Math.abs(want) >= 0.5) {
      const was = box.scrollTop;
      this.#scrollBy(want);
      // As far as the scroll went: the end of the page stops a row near the list's end lower.
      this.#viewTop += box.scrollTop - was;
    }
    const landing = { covered, key: this.#keys[index]! };
    this.#landing = landing;
    const jump = {};
    jumping = jump;
    // Over when its frames have drawn, or with its list: a jump left landing had every list attached later draw as one
    // it brought.
    const landed = () => {
      if (jumping === jump) jumping = null;
      if (this.#landing === landing) this.#landing = null;
    };
    this.#growSoon(landed, landed);
    return true;
  }

  /** Row `index`'s top on screen (client pixels), drawn or not: where a line to an undrawn row points. */
  topOf(index: number): number {
    placeWindowsNow();
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

  /**
   * The device's scale changed (a zoom step): every height held was on the old grid. The undrawn rows' go onto the new
   * one, each drawn row is observed afresh, so the observer reports it as laid out now (and `#measure` holds the row on
   * screen through it, as for any redraw), and the room is written again.
   */
  #rescale() {
    this.#heights.regrid((px) => onGrid(px));
    for (const row of this.#drawn) {
      this.#sizes?.unobserve(row);
      this.#sizes?.observe(row);
    }
    this.#grid++;
    this.#remeasured++;
  }

  /** The rows' text size changed by `ratio` (`textSize`): every height held follows it, and the drawn rows are read again. */
  #resize(ratio: number) {
    this.#heights.rescale(ratio, (px) => onGrid(px));
    for (const row of this.#drawn) {
      this.#sizes?.unobserve(row);
      this.#sizes?.observe(row);
    }
    this.#grid++;
    this.#remeasured++;
  }

  /** `entries` may be missing: a test's stand-in observer calls back with none. */
  #measure(entries: ResizeObserverEntry[] | undefined) {
    let moved = false;
    for (const entry of entries ?? []) {
      const key = this.#keyOf.get(entry.target);
      const px = laidOut(entry);
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
 * cell gives none, and the columns stay sized by the drawn rows. A table with a
 * `colgroup` of its own keeps it: a fixed table whose first row is the spacer
 * would otherwise split its columns evenly (Code's pull requests, 2026-11-10).
 */
function freezeCols(
  table: HTMLTableElement,
  body: HTMLTableSectionElement,
  width: number,
) {
  // A table that declares its columns (a `colgroup`) has its widths already: another would add columns.
  if (table.querySelector(":scope > colgroup")) return;
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

/**
 * The jump landing now (`land`), until its frames have drawn: the lists attached in it draw the rows on screen alone
 * until then (`#bornIn`).
 */
let jumping: object | null = null;

/**
 * A list's place is read in three steps: every list's styles are read, then every list's room is written, then where
 * each sits is read. A list that read its styles after another had written its room asked for a style pass of the page
 * each, and with it a layout: the lines of a jump's cards, 39 ms of a sixth-speed main thread in `clipsOf`
 * (the ux21 overs pass).
 */
type Phase = "style" | "room" | "read";
/** The lists attached in this flush that have not read where they sit yet (`list`). */
const unplaced = new Set<(phase: Phase) => void>();

/**
 * Reads where `place`'s list sits once the flush that attached it is over, with every other list attached in it: the
 * first read lays the page out, and the rest find it laid out. Before that read a list draws its overscan's rows.
 */
function placeSoon(place: (phase: Phase) => void) {
  if (unplaced.size === 0) queueMicrotask(placeWindowsNow);
  unplaced.add(place);
}

/**
 * A jump to `el`, a drawn row or something in one (a diff's file picked in its list): `box` scrolls by `px`, and `el`
 * is the row every list holds where it lands, through the redraws that follow (rows near it measured, lines read in
 * above it), until the reader scrolls. The scroll is the lists' own, so it takes no other row for the reader's.
 *
 * Without it the anchor was the topmost row on screen once the jump's scroll was heard: under a band stuck over the
 * scroller's top (`TfDiffView`), the file before the picked one, whose growth then pushed the picked one down.
 */
export function landOn(el: HTMLElement, box: Element, px: number) {
  if (px !== 0) {
    box.scrollTop += px;
    shared.scrolled = { box, top: box.scrollTop };
    // Every list reads the view it now shows, in this task: one that read a view the page was moved from (a scroll
    // cut back, put right here) would take `el`'s rows out for a frame.
    for (const reread of viewReads) reread();
  }
  shared.pinnedAt = null;
  shared.anchor = { el, y: el.getBoundingClientRect().top, box, scroll: box.scrollTop };
}

/**
 * Lets go of the row the lists hold (Zen mode's flip, Z1): what moves the rows is a header hidden or shown above them,
 * which is the flip's own move, and the flip holds the reader's row itself. Held, a list scrolled the page by the
 * header's height to undo it, so leaving Zen at a page's top scrolled the header it brought back out of sight.
 */
export function releaseAnchor(): void {
  shared.anchor = null;
  shared.pinnedAt = null;
}

/** Each attached list's read of the view, for {@link landOn}. */
const viewReads = new Set<() => void>();

/** Lets go of every list's read of the view: a test's fresh window. */
export function forgetViewReads() {
  viewReads.clear();
}

/** Whether `event` is a scroll a list made to hold its anchor (or a {@link landOn}), not the reader's. */
export function listScrolled(event: Event): boolean {
  return shared.ownEvent === event;
}

/** Reads now where every list attached and not yet placed sits: before a list's place is asked of it, and in tests. */
export function placeWindowsNow() {
  const all = [...unplaced];
  unplaced.clear();
  // Every list's room before any list reads the page (the compare-jump pass, 2026-11-17): a file's lines come in as a
  // windowed list that, unplaced, holds no room for its rows past the overscan; read after another list's read laid the
  // page out, it had made the page 5,000 px shorter for that layout, and a page scrolled near its end was cut back to it
  // and stayed there, a file picked near the list's end landing far from where it was put.
  for (const each of all) each("style");
  for (const each of all) each("room");
  for (const each of all) each("read");
}

/** The lists placed whose overscan below the view waits for the frame that shows their rows (`growSoon`). */
const ungrown = new Set<() => void>();
/** The frame or task the grow waits on, to start the wait again when another list is placed. */
let growWait: { frame: number | null; task: ReturnType<typeof setTimeout> | null } | null = null;

/**
 * Draws `grow`'s list's overscan below the view once the frame that shows its rows, and the one after it, have drawn,
 * counted from the last list placed: the rows nobody sees yet are built after the page's rows on screen are painted,
 * in a task of their own, not in the ones that hold its first useful frame. A page whose lists are placed in turns
 * (Builds' rail, then a pipeline's days once its runs answer) grows them all once the last has drawn.
 */
function growSoon(grow: () => void): () => void {
  ungrown.add(grow);
  stopGrowWait();
  const wait: { frame: number | null; task: ReturnType<typeof setTimeout> | null } = { frame: null, task: null };
  growWait = wait;
  const later = () => (wait.task = setTimeout(growWindowsNow, 0));
  if (typeof requestAnimationFrame === "undefined") later();
  else wait.frame = requestAnimationFrame(() => (wait.frame = requestAnimationFrame(() => ((wait.frame = null), later()))));
  // Takes `grow` back (its list went); the wait goes with the last grow it held, whichever frame or task it is on.
  return () => {
    if (ungrown.delete(grow) && ungrown.size === 0) stopGrowWait();
  };
}

/** Cancels the frame or task the grow waits on. */
function stopGrowWait() {
  if (growWait === null) return;
  if (growWait.frame !== null) cancelAnimationFrame(growWait.frame);
  if (growWait.task !== null) clearTimeout(growWait.task);
  growWait = null;
}

/** Draws now the overscan below the view of every list that waits for it (`growSoon`): when it is asked, and in tests. */
export function growWindowsNow() {
  stopGrowWait();
  const all = [...ungrown];
  ungrown.clear();
  for (const each of all) each();
}

/** A drawn row's border-box height as laid out: its box's, on the grid; the observer's where a transform scales the box. */
export function laidOut(entry: ResizeObserverEntry): number {
  const told = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
  const box = entry.target.getBoundingClientRect().height;
  return onGrid(Math.abs(box - told) < 2 / STEPS ? box : told);
}

/** Sets `px` of room through the CSSOM (CSP refuses inline styles in markup), none when 0. */
function pad(node: HTMLElement, property: string, px: number) {
  if (px > 0) node.style.setProperty(property, gridLength(px));
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

/** What in a row takes focus, in order. */
function focusables(row: HTMLElement): HTMLElement[] {
  const all = [...row.querySelectorAll<HTMLElement>("a[href], button, input, select, textarea, summary, [tabindex]")];
  return (row.matches("a[href], button, summary, [tabindex]") ? [row, ...all] : all).filter((el) => el.tabIndex >= 0 || el === row);
}
