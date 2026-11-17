/**
 * The boxes that have scrolled (PR #2479): what a page hiding or a return reads its place from. Reading every
 * element's `scrollTop` on a hide cost the largest Confluence page 92 ms with the main thread a quarter as fast, for
 * its 30,000 elements, of which a handful ever scroll. A box away from its top got there by scrolling, and each scroll
 * raises a `scroll` event, which one capturing listener hears for every box.
 */
const boxes = new Set<Element>();
let listening = false;

/** Starts listening; the module does it on load, a test after replacing the document. */
export function listenForScrolls(): void {
  if (listening || typeof document === "undefined") return;
  listening = true;
  document.addEventListener(
    "scroll",
    (event) => {
      if (event.target instanceof Element) boxes.add(event.target);
    },
    { capture: true, passive: true },
  );
}
listenForScrolls();

/** The boxes inside `root` (not `root` itself) that sit away from their top or left edge now. */
export function scrolledWithin(root: Element): Element[] {
  const out: Element[] = [];
  for (const el of boxes) {
    if (!el.isConnected) {
      boxes.delete(el);
      continue;
    }
    if (el !== root && root.contains(el) && (el.scrollTop !== 0 || el.scrollLeft !== 0)) out.push(el);
  }
  // Document order, as `querySelectorAll("*")` gave them.
  return out.sort((a, b) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1));
}

/**
 * **Each box's place, noted as the address changes** (the ux18 pass, 2026-11-17): the shell notes them as a move starts,
 * before any page is built or hidden, while the last frame's layout is still clean, so reading them costs nothing. A
 * page hidden by the move takes its place from the note ({@link notedScrolls}) where it read every box after the new
 * page was in the DOM, which laid the half-built page out inside the build of a repository's first open (19 ms of it
 * at CPU 4x on Björn's PC). The note stands for the task the move runs in: the hide's effects run in its microtasks.
 */
let noted: Map<Element, [number, number]> | null = null;

/** Notes the shell's scroller and every box away from its top or left edge now. */
export function noteScrolls(): void {
  if (typeof document === "undefined") return;
  const now = new Map<Element, [number, number]>();
  for (const main of document.querySelectorAll(".tf-main")) now.set(main, [main.scrollTop, main.scrollLeft]);
  for (const el of boxes) {
    if (!el.isConnected || now.has(el)) continue;
    if (el.scrollTop !== 0 || el.scrollLeft !== 0) now.set(el, [el.scrollTop, el.scrollLeft]);
  }
  noted = now;
  setTimeout(() => {
    if (noted === now) noted = null;
  }, 0);
}

/** The places {@link noteScrolls} noted in this task, or `null` where no move noted them. */
export function notedScrolls(): ReadonlyMap<Element, [number, number]> | null {
  return noted;
}
