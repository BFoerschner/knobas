/**
 * **The ticket page card's width** (spec #2396, #2402): the card's column is
 * the reader's to drag, between {@link SIDE_MIN} and {@link SIDE_MAX}, and a
 * double click on its edge fits it so nothing on the card wraps.
 *
 * Kept in the webview's `localStorage` (`knobas.ticket.side`), as the chosen
 * job is: read when the shell mounts, never through IPC. A kept width out of
 * bounds reads clamped; one that is no number, or a read that fails, reads as
 * the default. A write that fails keeps the width for this run only.
 */
import { keptValue, windowStorage, type KeptStorage } from "../kept-value";

export const SIDE_KEY = "knobas.ticket.side";
export const SIDE_MIN = 240;
export const SIDE_MAX = 720;
export const SIDE_DEFAULT = 320;

/** A width in whole pixels, within the bounds. */
export function clampSide(px: number): number {
  return Math.round(Math.min(SIDE_MAX, Math.max(SIDE_MIN, px)));
}

export interface CardWidth {
  /** The card's column, in pixels. */
  readonly px: number;
  /** Set the column to `px`, clamped, and keep it. */
  set(px: number): void;
  /** Read the kept width again: the shell's mount, as a restart does. */
  load(): void;
  /** Tests only: the default width, nothing kept. */
  reset(): void;
}

export function createCardWidth(storage: () => KeptStorage | null): CardWidth {
  const kept = keptValue(storage, SIDE_KEY);
  const read = (): number => {
    const px = Number.parseFloat(kept.read() ?? "");
    return Number.isFinite(px) ? clampSide(px) : SIDE_DEFAULT;
  };
  let px = $state(SIDE_DEFAULT);
  return {
    get px() {
      return px;
    },
    set(next) {
      px = clampSide(next);
      kept.write(String(px));
    },
    load() {
      px = read();
    },
    reset() {
      px = SIDE_DEFAULT;
      kept.clear();
    },
  };
}

/** The window's one card width, over this viewer's `localStorage` where there is one. */
export const cardWidth: CardWidth = createCardWidth(windowStorage);

/**
 * The width at which nothing on `card` wraps: a clone laid out at its
 * content's width, every line kept whole and the facts' values on one line,
 * measured out of sight beside the card and removed. One pixel more than the
 * measure, so a fraction does not wrap the widest line; `set` caps it.
 */
export function naturalWidth(card: HTMLElement): number {
  const clone = card.cloneNode(true) as HTMLElement;
  clone.style.cssText = "position:absolute;left:-99999px;top:0;width:max-content;max-width:none;overflow:visible;visibility:hidden";
  // The width itself last: jsdom's `cssText` parser keeps no `max-content`.
  clone.style.width = "max-content";
  clone.setAttribute("aria-hidden", "true");
  for (const el of clone.querySelectorAll<HTMLElement>("*")) el.style.whiteSpace = "nowrap";
  for (const el of clone.querySelectorAll<HTMLElement>(".tf-facts dd")) el.style.flexWrap = "nowrap";
  card.after(clone);
  try {
    return Math.ceil(clone.getBoundingClientRect().width) + 1;
  } finally {
    clone.remove();
  }
}
