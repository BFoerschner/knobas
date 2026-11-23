/**
 * The XML view's keys (its design, 2026-11-23, section 29; Björn's answer 9: the keys as designed), in one place: a
 * later answer about a key is a change here. Bare keys, on the Code page, from anywhere in the view outside a field.
 * The arrows (`←` `→` fold and walk, `Shift` for everything under; `↑` `↓` move the pick) and `0` to `9` (exactly that
 * many levels) are the design's own and are not listed here.
 */
export const DOC_KEYS = {
  /** One level fewer: `-`. */
  fewer: ["-"],
  /** One level more: `+`, and `=`, which is `+` unshifted on some keyboards. */
  more: ["+", "="],
  /** Focuses the query's field and selects what it holds. */
  query: ["q", "Q"],
  /** The next and the one before of what the rail lists. */
  next: ["n"],
  previous: ["p"],
  /** Looks at the picked line's value, as Quick Look does. */
  look: [" "],
  /** Opens the picked line's address; with `Shift`, in the browser. */
  web: ["w", "W"],
} as const satisfies Record<string, readonly string[]>;

/** Whether `key` is one of `keys`. */
export const isKey = (keys: readonly string[], key: string): boolean => keys.includes(key);

/**
 * Björn's answer 5 (2026-11-23): an image from a host a person kept loads when its dialog opens, without asking again;
 * still only on the press that opens the dialog. `false` here offers it every time.
 */
export const KEPT_HOSTS_LOAD_BY_ITSELF = true;
