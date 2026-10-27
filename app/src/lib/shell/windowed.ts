/**
 * The arithmetic of a windowed list (#2209): where each row sits, from the
 * heights measured so far and an estimate for the rest, and which rows a
 * viewport needs drawn. No DOM here; `windowed.svelte.ts` feeds it.
 *
 * A row's slot runs from its top to the next row's top, so the gap after a
 * row belongs to it. Measurements are kept by the row's key, not its place,
 * so a filter that narrows or reorders the list keeps them.
 */

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
  readonly #gap: number;
  readonly #measured = new Map<K, number>();
  #keys: readonly K[] = [];
  /** `offsets[i]` is row i's top; `offsets[n]` is the end of the last slot (its gap included). */
  #offsets = new Float64Array(1);
  #stale = false;

  constructor(estimate: number, gap = 0) {
    this.#estimate = estimate;
    this.#gap = gap;
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

  /** A row's measured height, else the estimate. */
  heightOf(key: K): number {
    return this.#measured.get(key) ?? this.#estimate;
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

  /** The rows a viewport `top` to `top + height` (list pixels) needs, `overscan` pixels more each side. */
  span(top: number, height: number, overscan: number): Span {
    const n = this.#keys.length;
    if (n === 0) return { start: 0, end: 0, before: 0, after: 0 };
    const at = this.#layout();
    const lo = Math.max(0, top - overscan);
    const hi = top + height + overscan;
    const start = Math.min(n - 1, Math.max(0, firstAbove(at, lo) - 1));
    const end = Math.min(n, Math.max(start + 1, firstAtLeast(at, hi)));
    return { start, end, before: at[start]!, after: at[n]! - at[end]! };
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
