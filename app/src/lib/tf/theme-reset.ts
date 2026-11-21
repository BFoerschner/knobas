/**
 * Putting knobas's own themes back (ADR-0055, the decisions' 3.9): both window slots knobas's, both code slots
 * following, nothing deleted, with *Undo*. Four ways reach it: the Theme page's button, the launcher's action, a key
 * heard anywhere (where the zoom keys are, a field and a dialog too) and `knobas --default-theme` for one run.
 *
 * The key is ⌘⇧⌫ on the Mac and Ctrl+Shift+Backspace elsewhere: no Alt, since AltGr is Ctrl+Alt on a German board.
 * It is spelled here and nowhere else.
 */
import { undoable } from "../shell/toasts.svelte";
import type { Themes } from "./themes.svelte";

/** The key as the Mac writes it; `keyHint` writes it for this platform. */
export const RESET_HINT = "⌘⇧⌫";

/** Whether a key press is the reset key: ⌘ or Ctrl, with Shift, on Backspace, and no Alt. */
export function isResetKey(event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">): boolean {
  return (event.metaKey || event.ctrlKey) && event.shiftKey && !event.altKey && event.key === "Backspace";
}

/** knobas's own themes back, said in a toast with *Undo*. */
export function putKnobasBack(themes: Themes): void {
  const undo = themes.putKnobasBack();
  undoable("knobas's own themes are back. Your themes are kept.", undo, { subject: "theme-reset" });
}
