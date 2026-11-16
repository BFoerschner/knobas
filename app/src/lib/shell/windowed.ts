/**
 * The arithmetic of a windowed list (#2209): where each row sits, from the
 * heights measured so far and an estimate for the rest, and which rows a
 * viewport needs drawn. No DOM here; `windowed.svelte.ts` feeds it.
 *
 * A row's slot runs from its top to the next row's top, so the gap after a
 * row belongs to it. Measurements are kept by the row's key, not its place,
 * so a filter that narrows or reorders the list keeps them.
 */

/**
 * Chromium lays out in device pixels, in steps of a 64th (a layout unit). A row's height is a whole number of
 * those steps; in CSS pixels at a scale of 1.25 or 1.5 it is not, and a ResizeObserver reports it cut down to a 64th of
 * a CSS pixel (a 51-device-pixel row at 1.25 is 40.8 px, told as 40.796875). Room summed from such heights came out
 * short by a step or two for every few rows, so each row swap moved every row that stayed by a fraction of a pixel,
 * and a row that moves is painted again: the whole visible table, every swap (#2508). So a height is kept on the
 * device's grid, read from the row's box, which carries it whole, and the room is written so it lands on that grid.
 */
export const STEPS = 64;

/** The display's scale; 1 where there is none (jsdom). */
const scale = () => (typeof devicePixelRatio === "number" && devicePixelRatio > 0 ? devicePixelRatio : 1);

/** `px` (CSS) on the device's layout grid, in CSS pixels. */
export function onGrid(px: number, dpr = scale()): number {
  return Math.round(px * dpr * STEPS) / STEPS / dpr;
}

/** The layout step a CSS length `px` lands on at scale `dpr`, as Chromium cuts it: in single precision, then down. */
export function stepOf(px: number, dpr: number): number {
  return Math.floor(Math.fround(Math.fround(px) * Math.fround(dpr)) * STEPS);
}

/**
 * `px` as a CSS length that lays out at exactly `px` on the device's grid. At a scale that is not a power of two the
 * length times the scale is not the step; Chromium takes it in single precision and cuts it down to a step (measured,
 * 2026-11-16), so the length aims at the middle of the step and, where single precision carries it off, moves by
 * the least amount single precision can. Past about 2^17 device pixels single precision cannot hit every step, and a
 * swap there may move the rows by one (then they are painted again, as before).
 */
export function gridLength(px: number, dpr = scale()): string {
  const device = Math.round(px * dpr * STEPS) / STEPS;
  if (Number.isInteger(Math.log2(dpr))) return `${device / dpr}px`;
  const want = device * STEPS;
  const aim = (device + 1 / (2 * STEPS)) / dpr;
  if (stepOf(aim, dpr) === want) return `${aim}px`;
  const near = Math.fround(aim);
  // Single precision's step at `near`: 23 bits below its leading one.
  const ulp = 2 ** (Math.floor(Math.log2(near)) - 23);
  for (let k = 1; k <= 4; k++)
    for (const at of [near + k * ulp, near - k * ulp]) if (stepOf(at, dpr) === want) return `${at}px`;
  return `${aim}px`;
}

/** The rows to draw, `start` to `end` (exclusive), and the room the undrawn rows take above and below them. */
export interface Span {
  start: number;
  end: number;
  /** Pixels above the first drawn row's top. */
  before: number;
  /** Pixels below the last drawn row's bottom. */
  after: number;
}

export class Heights<K = string> {
  readonly #estimate: number;
  /** A row's own estimate, where rows differ (a comment's length, a file's lines); else `#estimate`. */
  readonly #estimateOf: ((key: K) => number) | null;
  readonly #gap: number;
  readonly #measured = new Map<K, number>();
  #keys: readonly K[] = [];
  /** `offsets[i]` is row i's top; `offsets[n]` is the end of the last slot (its gap included). */
  #offsets = new Float64Array(1);
  #stale = false;

  constructor(estimate: number, gap = 0, estimateOf: ((key: K) => number) | null = null) {
    this.#estimate = estimate;
    this.#gap = gap;
    this.#estimateOf = estimateOf;
  }

  /** The list's rows, in order, by key. */
  keys(keys: readonly K[]) {
    this.#keys = keys;
    this.#stale = true;
  }

  /** Records a drawn row's height; `true` when that moves anything. */
  set(key: K, px: number): boolean {
    const was = this.heightOf(key);
    this.#measured.set(key, px);
    if (was === px) return false;
    this.#stale = true;
    return true;
  }

  /** Every measured height through `to` (onto another scale's grid); the estimates stand as they are. */
  regrid(to: (px: number) => number) {
    for (const [key, px] of this.#measured) this.#measured.set(key, to(px));
    this.#stale = true;
  }

  /** A row's measured height, else its estimate. */
  heightOf(key: K): number {
    return this.#measured.get(key) ?? this.#estimateOf?.(key) ?? this.#estimate;
  }

  /** Row `index`'s top. */
  offsetOf(index: number): number {
    return this.#layout()[Math.max(0, Math.min(index, this.#keys.length))]!;
  }

  /** The whole list's height. */
  total(): number {
    const n = this.#keys.length;
    return n === 0 ? 0 : this.#layout()[n]! - this.#gap;
  }

  /**
   * The rows a viewport `top` to `top + height` (list pixels) needs, `overscan` pixels more above it and `below` pixels
   * more under it (`overscan` too unless given).
   */
  span(top: number, height: number, overscan: number, below = overscan): Span {
    const n = this.#keys.length;
    if (n === 0) return { start: 0, end: 0, before: 0, after: 0 };
    const at = this.#layout();
    const lo = Math.max(0, top - overscan);
    const hi = top + height + below;
    const start = Math.min(n - 1, Math.max(0, firstAbove(at, lo) - 1));
    const end = Math.min(n, Math.max(start + 1, firstAtLeast(at, hi)));
    return { start, end, before: at[start]!, after: at[n]! - at[end]! };
  }

  /**
   * Whether rows `start` to `end` still cover a viewport `top` to `top + height` with `margin` pixels to spare each
   * side (less where the list ends): while they do, the drawn rows need not change.
   */
  covers(start: number, end: number, top: number, height: number, margin: number): boolean {
    const n = this.#keys.length;
    if (start < 0 || end > n || start >= end) return false;
    const at = this.#layout();
    const lo = Math.max(0, top - margin);
    const hi = Math.min(at[n]!, top + height + margin);
    return at[start]! <= lo && at[end]! >= hi;
  }

  /** Rows `start` to `end` as a span, with the room above and below them now. */
  around(start: number, end: number): Span {
    const at = this.#layout();
    return { start, end, before: at[start]!, after: at[this.#keys.length]! - at[end]! };
  }

  /** The viewport top that shows row `index` whole, moving `top` as little as it can. */
  reveal(index: number, top: number, height: number): number {
    const at = this.#layout();
    const rowTop = at[index]!;
    const rowBottom = at[index + 1]! - this.#gap;
    if (rowTop < top) return rowTop;
    if (rowBottom > top + height) return Math.min(rowTop, rowBottom - height);
    return top;
  }

  #layout(): Float64Array {
    if (!this.#stale) return this.#offsets;
    const n = this.#keys.length;
    const at = new Float64Array(n + 1);
    for (let i = 0; i < n; i++) at[i + 1] = at[i]! + this.heightOf(this.#keys[i]!) + this.#gap;
    this.#offsets = at;
    this.#stale = false;
    return at;
  }
}

/** The first index whose value is greater than `x`. */
function firstAbove(at: Float64Array, x: number): number {
  let lo = 0;
  let hi = at.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (at[mid]! > x) hi = mid;
    else lo = mid + 1;
  }
  return lo;
}

/** The first index whose value is `x` or more. */
function firstAtLeast(at: Float64Array, x: number): number {
  let lo = 0;
  let hi = at.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (at[mid]! >= x) hi = mid;
    else lo = mid + 1;
  }
  return lo;
}

/**
 * About how tall `text` draws: `base` pixels and `line` a line, a line every `chars` characters or newline. An
 * undrawn comment's room in a windowed thread (`estimateOf`): near enough that a long one's room is long.
 */
export function textHeight(text: string, { base, line, chars }: { base: number; line: number; chars: number }): number {
  let lines = 0;
  let from = 0;
  while (from <= text.length) {
    let to = text.indexOf("\n", from);
    if (to < 0) to = text.length;
    lines += Math.max(1, Math.ceil((to - from) / chars));
    from = to + 1;
  }
  return base + lines * line;
}
