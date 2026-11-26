/**
 * Putting knobas's own themes back (ADR-0055, the decisions' 3.9): both window slots knobas's, both code slots
 * following, nothing deleted, with *Undo*. Three ways reach it: the Theme page's button, the launcher's action, and
 * `knobas --default-theme` for one run. No key (Björn, 2026-11-22: the ⌘⇧⌫ of the design is not wanted).
 */
import { undoable } from "../shell/toasts.svelte";
import type { Themes } from "./themes.svelte";

/** knobas's own themes back, said in a toast with *Undo*. */
export function putKnobasBack(themes: Themes): void {
  const undo = themes.putKnobasBack();
  undoable("knobas's own themes are back. Your themes are kept.", undo, { subject: "theme-reset" });
}
