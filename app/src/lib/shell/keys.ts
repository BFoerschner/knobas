/**
 * Keyboard helpers the ticket-first shell's views share.
 *
 * The rooms UI's global keyboard (its `Esc` ladder, `⌘K`, `⌘T`, and history
 * on `⌘[` / `⌘]` and the mouse's buttons, #693) went with the rooms UI (#881).
 * The new shell installs its own keyboard (`tf/TfShell.svelte`, `tf/escape.ts`).
 */

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
