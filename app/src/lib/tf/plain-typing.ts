/**
 * **A one-line box keeps the words as typed** (design system, Forms; found in the app's own window, 2026-11-07). macOS's
 * text checking reaches a field in the webview: a pause then a space turns `retry w` into `Retry w`, and `retr` offers
 * *Rear* in a bubble over the page. A search, a branch, a label or an address must not change under the hand, so every
 * one-line input is told to leave them alone, as it gains focus, unless it says otherwise itself. A textarea and a
 * composer write sentences and keep the system's help.
 */

/** The inputs a person types a line into. A checkbox, a date or a number has no words to change. */
const LINE = new Set(["text", "search", "url", "email", "password", "tel", ""]);

function plain(target: EventTarget | null): void {
  if (!(target instanceof HTMLInputElement) || !LINE.has(target.getAttribute("type")?.toLowerCase() ?? "")) return;
  if (target.hasAttribute("autocorrect") || target.hasAttribute("spellcheck")) return;
  target.setAttribute("autocorrect", "off");
  target.setAttribute("autocapitalize", "off");
  target.spellcheck = false;
}

export function installPlainTyping(): () => void {
  const onfocus = (event: FocusEvent) => plain(event.target);
  document.addEventListener("focusin", onfocus, true);
  // A box that had focus before this ran, which autofocus can give.
  plain(document.activeElement);
  return () => document.removeEventListener("focusin", onfocus, true);
}
