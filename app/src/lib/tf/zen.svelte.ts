/**
 * Zen mode (ux31; Björn, 2026-11-22: "if z, call it zen mode too"; 2026-11-24: "I'd like zen mode to work everywhere in
 * code"): one switch for the whole of Code (step Z1, Fable's ruling). On every Code page it hides the page's crumbs, its
 * header, its tab bar and the rail on its left (the Files tree, a diff's file list, the Builds tab's branches); what the
 * page reads and works with stays: a file's own header, a diff's toolbar, the compare form, filters, *Levels*. The top
 * bar and the status bar stay: they are the way out of Code and what says a sync failed.
 *
 * - `Z` (outside a field, with no dialog open) or the top bar's *Zen* turns it on and off, on any Code page; `Esc` leaves
 *   it first (an inline panel's rung). The shell owns all three ({@link flipZen}).
 * - It holds while the reader moves between Code pages, and ends on leaving Code. It is a way of reading, not a setting:
 *   never kept.
 * - Nothing moves but what it hides and shows: the topmost row the reader sees keeps its place on the screen, and the
 *   panes that fit the window fit again in the frame of the flip.
 */

import { releaseAnchor } from "../shell/windowed.svelte";

/** The key that turns Zen mode on and off, anywhere in Code. Free there: `T` is *Go to file*. */
export const ZEN_KEY = "z";

/** How the key is drawn: the tooltip, Settings › Keyboard. */
export const ZEN_KEY_LABEL = ZEN_KEY.toUpperCase();

/**
 * What Zen mode hides, under `.tf-main.tf-zen` (`css/code.css` says the same list): a page's crumbs, header and tabs,
 * and the rails on a page's left with their side edges.
 */
export const ZEN_HIDES = [
  ".tf-full > .tf-crumbs",
  ".tf-full > .tf-fhead",
  ".tf-full > .tf-tabs",
  ".tf-repo > .tf-ftreebox",
  ".tf-repo > .tf-sedge",
  ".tf-dgrid > .tf-dnav",
  ".tf-dgrid > .tf-sedge",
  ".tf-dgrid > .tf-railbar",
  ".tf-rbv > .tf-rbrail",
  ".tf-rbv > .tf-sedge",
].join(", ");

/** Whether a key press is Zen mode's: `z` with no ⌘, Ctrl or ⌥, not answered already. A field is the caller's to ask. */
export function isZenKey(event: KeyboardEvent): boolean {
  return event.key.toLowerCase() === ZEN_KEY && !event.metaKey && !event.ctrlKey && !event.altKey && !event.defaultPrevented;
}

class Zen {
  /** Whether the reader asked for it. */
  on = $state(false);
  /** Whether the page shown is one of Code's (the shell says so). */
  here = $state(false);
  /** Whether the page is drawn in Zen mode now. */
  get shown(): boolean {
    return this.on && this.here;
  }
  toggle(): void {
    this.on = !this.on;
  }
}

/** The window's Zen mode. */
export const zen = new Zen();

/** A row the reader sees at the scroller's top, outside what Zen hides, and where it stands. */
interface Pin {
  readonly el: Element;
  readonly top: number;
}

/**
 * The topmost element the reader sees in `scroller` that Zen leaves standing: looked for from the box's top down, a
 * third of the way in from its right, where a page's own content stands and no left rail does.
 */
export function pinIn(scroller: HTMLElement): Pin | null {
  const box = scroller.getBoundingClientRect();
  if (box.height <= 0 || typeof document.elementsFromPoint !== "function") return null;
  const x = box.left + box.width * 0.66;
  for (let y = box.top + 2; y < box.bottom; y += 12) {
    for (const el of document.elementsFromPoint(x, y)) {
      if (el === scroller || !scroller.contains(el) || el.closest(ZEN_HIDES) !== null) continue;
      const r = el.getBoundingClientRect();
      // A box as tall as the view is a page's frame, not a row the eye holds.
      if (r.height <= 0 || r.height >= box.height * 0.8) continue;
      return { el, top: r.top };
    }
  }
  return null;
}

/**
 * Turns Zen mode on or off with nothing moving but what it hides and shows (Fable's ruling, 2026-11-24): the topmost
 * row the reader sees on a scrolled page keeps its place, measured after the flip's layout and set before its paint; at
 * the page's top the header simply comes and goes; a focus inside what
 * Zen hides goes to the scroller first, so `Z` and `T` keep working; and the panes fitted to the window (the
 * explorers, the XML view) fit again in the same frame, as they do on a resize.
 */
export function flipZen(scroller: HTMLElement | null, flush: () => void): void {
  // At the page's top nothing is pinned: the header shows and hides above the content, which moves by its height (Zen's
  // own move). Pinned there, leaving Zen would scroll the header it brings back out of sight.
  const pin = scroller === null || scroller.scrollTop <= 0 ? null : pinIn(scroller);
  const entering = !zen.on;
  const active = document.activeElement;
  if (entering && scroller !== null && active instanceof HTMLElement && active.closest(ZEN_HIDES) !== null) scroller.focus({ preventScroll: true });
  // The windowed lists hold a row through their redraws; the header going or coming is no redraw of theirs to undo.
  releaseAnchor();
  zen.toggle();
  flush();
  window.dispatchEvent(new Event("resize"));
  if (scroller !== null && pin !== null && pin.el.isConnected) {
    const moved = pin.el.getBoundingClientRect().top - pin.top;
    if (moved !== 0) scroller.scrollTop += moved;
  }
}
