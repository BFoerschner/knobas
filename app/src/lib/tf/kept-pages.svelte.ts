/**
 * **Keep-alive** (#2247, spec #2234 step 3): the pages the page host
 * (`TfPage.svelte`) keeps mounted and hidden, so going back to one is instant.
 *
 * - {@link hold} is the cache: the pages held, least recently used first and
 *   the shown one last. It keeps the shown page plus up to *Pages kept open*
 *   hidden ones, and drops the least recently used past that.
 * - {@link keptPages} is *Pages kept open* (Settings › Performance), 0 to 15,
 *   5 until set. 0 holds only the shown page, which is the shell before
 *   keep-alive. A change is read at once: the host trims to it on the spot.
 * - {@link pageVisible} is how a component inside a page asks whether its page
 *   is the shown one, without the page threading `visible` down to it.
 */
import { getContext, setContext } from "svelte";

export const KEPT_PAGES_MIN = 0;
export const KEPT_PAGES_MAX = 15;
export const KEPT_PAGES_DEFAULT = 5;

/** One held page: its key, what the host keeps for it, and how often it was shown again. */
export interface Held<T> {
  /** The page's address key: two keys are two pages, mounted apart. */
  key: string;
  value: T;
  /** How many times the page was shown again after being hidden: moves on each return. */
  shows: number;
  /** Whether this is the page in front. At most one is, and it is the last. */
  visible: boolean;
}

/**
 * The held pages after showing `next` (or no page: an address only the shell's
 * own fallbacks draw), with at most `cap` hidden pages behind it.
 *
 * `next` with the key of a held page is that page: it keeps its instance, takes
 * `next.value` (the address it is at now) and moves last. If it was hidden its
 * `shows` moves, which is what a page re-reads on.
 */
export function hold<T>(held: readonly Held<T>[], next: { key: string; value: T } | null, cap: number): Held<T>[] {
  const hidden = held
    .filter((page) => page.key !== next?.key)
    .map((page) => (page.visible ? { ...page, visible: false } : page));
  const kept = hidden.slice(Math.max(0, hidden.length - cap));
  if (next === null) return kept;
  const before = held.find((page) => page.key === next.key);
  const shown: Held<T> = {
    key: next.key,
    value: next.value,
    shows: before === undefined ? 0 : before.shows + (before.visible ? 0 : 1),
    visible: true,
  };
  return [...kept, shown];
}

/** *Pages kept open*, the one the window's page host reads. */
export interface KeptPagesSetting {
  readonly value: number;
  /** Clamped to 0–15 and rounded; a value that is not a number changes nothing. */
  set(next: number): void;
  /** Tests only: back to the default. */
  reset(): void;
}

export function createKeptPagesSetting(): KeptPagesSetting {
  let value = $state(KEPT_PAGES_DEFAULT);
  return {
    get value() {
      return value;
    },
    set(next) {
      if (!Number.isFinite(next)) return;
      value = Math.min(KEPT_PAGES_MAX, Math.max(KEPT_PAGES_MIN, Math.round(next)));
    },
    reset() {
      value = KEPT_PAGES_DEFAULT;
    },
  };
}

/** The window's one: the shell loads the stored value into it, Settings › Performance writes it. */
export const keptPages = createKeptPagesSetting();

const VISIBLE = Symbol("page visible");

/** Called by the page host for each page it draws, during that page's set-up. */
export function providePageVisible(visible: () => boolean): void {
  setContext(VISIBLE, visible);
}

/**
 * Whether the page this component is drawn in is the shown one. Outside a page
 * host (a dialog, a test mounting a component alone) it is always shown.
 * Call during component set-up; read the answer where it is needed.
 */
export function pageVisible(): () => boolean {
  return getContext<(() => boolean) | undefined>(VISIBLE) ?? (() => true);
}

/**
 * The context of a page that `visible` says is shown or hidden, for a test that
 * mounts one component alone (`mount(C, { context })`) and hides its page.
 */
export function pageContext(visible: () => boolean): Map<unknown, unknown> {
  return new Map<unknown, unknown>([[VISIBLE, visible]]);
}
