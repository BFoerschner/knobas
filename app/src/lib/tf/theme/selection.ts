/**
 * Whether this engine's highlights inherit: an element's `::selection` takes what its parent's has when no rule names
 * the element's own (CSS Pseudo-Elements 4, highlight inheritance). Chromium's do, so WebView2's; WebKit's do not.
 * `tokens.css` colours a theme's selected text through `<html>`'s own `::selection`, and gives an engine without it
 * the rules for every element under `data-sel-each` (`tf/themes.svelte.ts`). Asked once, the first time a theme's
 * selection is drawn: a frame-free style read of two elements, put in and taken out at once. Anything that cannot be
 * read answers no, which draws the selection the way that works everywhere.
 */
export function selectionInherits(doc: Document | undefined = typeof document === "undefined" ? undefined : document): boolean {
  if (doc === undefined || doc.body === null || typeof doc.defaultView?.getComputedStyle !== "function") return false;
  // jsdom resolves no pseudo-element's style, and says so on the console.
  if (/jsdom/i.test(doc.defaultView.navigator?.userAgent ?? "")) return false;
  const style = doc.createElement("style");
  style.textContent = ".knobas-sel-probe::selection { background-color: rgb(1, 2, 3); }";
  const outer = doc.createElement("div");
  outer.className = "knobas-sel-probe";
  // Laid out never: taken out in the same task, before any frame. Not `display: none`, whose styles Chromium resolves
  // apart from its highlights'.
  outer.style.cssText = "position: absolute; visibility: hidden";
  const inner = doc.createElement("span");
  outer.append(inner);
  try {
    doc.head.append(style);
    doc.body.append(outer);
    return doc.defaultView.getComputedStyle(inner, "::selection").backgroundColor === "rgb(1, 2, 3)";
  } catch {
    return false;
  } finally {
    outer.remove();
    style.remove();
  }
}
