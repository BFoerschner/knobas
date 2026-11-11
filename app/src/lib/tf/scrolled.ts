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
