/**
 * Zen mode (ux31; Björn, 2026-11-22: "if z, call it zen mode too. Maybe have it for renderers in general, if
 * applicable?"): a file on the Code page alone, the repository's crumbs, header, tabs and tree hidden, for the
 * renderers where a file wants the room: Markdown, PDF, JAR, an image, the API explorer, a diagram, a page. Its key and where it applies
 * are kept here, in one place: the file view's button, its key, its tooltip and Settings › Keyboard read them.
 *
 * - `Z` (outside a field) or the file header's button turns it on and off; `Esc` leaves it (an inline panel's rung).
 * - It holds while the reader moves from file to file; on a file of another renderer nothing is hidden.
 * - Leaving the file view (another tab, another page) ends it. It is a way of reading, not a setting: never kept.
 */

/** The key that turns Zen mode on and off, on the Code page's file view. Free there: `T` is *Go to file*. */
export const ZEN_KEY = "z";

/** How the key is drawn: the tooltip, Settings › Keyboard. */
export const ZEN_KEY_LABEL = ZEN_KEY.toUpperCase();

/** The renderers Zen mode applies to. */
export const ZEN_RENDERERS: readonly string[] = ["markdown", "pdf", "jar", "image", "openapi", "mermaid", "d2", "graphviz", "html"];

/** Whether a file drawn by `drawAs` may be shown in Zen mode. */
export function zenApplies(drawAs: string | null): boolean {
  return drawAs !== null && ZEN_RENDERERS.includes(drawAs);
}

/** Whether a key press is Zen mode's: `z` with no ⌘, Ctrl or ⌥, not answered already. A field is the caller's to ask. */
export function isZenKey(event: KeyboardEvent): boolean {
  return event.key.toLowerCase() === ZEN_KEY && !event.metaKey && !event.ctrlKey && !event.altKey && !event.defaultPrevented;
}

class Zen {
  /** Whether the reader asked for it. */
  on = $state(false);
  /** Whether the shown file view holds a file Zen mode applies to (its file view says so). */
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
