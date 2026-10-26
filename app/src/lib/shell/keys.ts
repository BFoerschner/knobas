/**
 * Keyboard helpers the ticket-first shell's views share.
 *
 * The rooms UI's global keyboard (its `Esc` ladder, `⌘K`, `⌘T`) went with the
 * rooms UI (#881). The new shell installs its own keyboard
 * (`tf/TfShell.svelte`, `tf/escape.ts`), and since #1086 history's keys too.
 */

/**
 * Whether a key press is history's back or forward, or neither (#693).
 *
 * `⌘[` / `⌘]` (and `⌘←` / `⌘→`) are back and forward. They are not rungs of
 * the `Esc` ladder: `Esc` closes things, and back goes to the previous
 * address, which may be the view `Esc` just left. Outside a text field only,
 * because a field owns those keys (outdent, start of line).
 */
export function historyStep(event: KeyboardEvent): "back" | "forward" | null {
  // A key a focused control already answered is that control's: ⌘← on a
  // view's tab bar or in the tree walk moves there and must not also go back.
  if (event.defaultPrevented || event.shiftKey) return null;
  // The brackets with ⌘ or Ctrl, as browsers bind them on every platform; the
  // arrows with ⌘ alone, because Ctrl+← / → is the Mac's switch-Spaces. ⌥ is
  // allowed with ⌘ only, for a Mac layout (German, say) that types `[` with
  // ⌥; with Ctrl it is AltGr, which types the bracket and means no shortcut.
  if (event.altKey && !event.metaKey) return null;
  const arrow = !event.altKey && event.metaKey;
  const bracket = event.metaKey || event.ctrlKey;
  const step =
    (bracket && event.key === "[") || (arrow && event.key === "ArrowLeft")
      ? "back"
      : (bracket && event.key === "]") || (arrow && event.key === "ArrowRight")
        ? "forward"
        : null;
  if (step === null || inTextField(event.target)) return null;
  return step;
}

/** A field that types: its keys are its own. The ticket-first shell's `Esc` asks too. */
export function inTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return !["button", "checkbox", "radio", "submit", "reset", "range", "color", "file"].includes(
    target.type,
  );
}

/**
 * Whether a key press is ⌘R (Ctrl+R off the Mac), the page's *Refresh* or
 * *Sync now* (#2156; `tf/page-refresh.svelte.ts`). With ⇧ or ⌥ too: those are
 * the webview's other reloads, and none of them may happen. In a text field
 * as well, which has no ⌘R of its own. ⌥R types `®` on the Mac, so with ⌥
 * the physical key answers, not the character.
 */
export function refreshKey(event: KeyboardEvent): boolean {
  if (!event.metaKey && !event.ctrlKey) return false;
  return event.key.toLowerCase() === "r" || (event.altKey && event.code === "KeyR");
}
