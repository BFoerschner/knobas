/**
 * **The ticket page card's fit** (spec #2396, #2402): a double click on the
 * card's edge, or Enter on it, sets the card's column so nothing on the card
 * wraps. The column itself is a sidebar like any other (`side-width.svelte.ts`,
 * `ticket-card`): 240 to 720 px, kept in `localStorage` as `knobas.ticket.side`.
 */
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
