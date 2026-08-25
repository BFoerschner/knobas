/**
 * The global keyboard: the `Esc` unwind ladder and `⌘K`.
 *
 * The mockup ran one handler over one re-rendered DOM
 * (`signal-miller.html:4184-4202`). The rungs it had for surfaces that do not
 * exist yet — the launcher box's chain and query, the Miller column stack —
 * are not reproduced; what is left is the M1 ladder, in order:
 *
 * 1. a modal or popover is open  → the component closes it and calls
 *    `stopPropagation`, so this handler never sees the key at all;
 * 2. the detail slide-over is open → back to the room it was opened over;
 * 3. a non-room view is open       → back to the room;
 * 4. otherwise                     → nothing.
 *
 * Rung 4 is a rule, not an omission: `Esc` must never exit the app, quit a
 * context, or discard anything. A key that sometimes does nothing is what
 * makes the other three safe to press.
 *
 * `⌘T` is M3's timer and is deliberately **not** bound — binding it now would
 * train a habit the app cannot honour.
 */
import type { Router } from "./router.svelte";

export interface KeyHandlers {
  /** `⌘K` / `Ctrl+K`. Stream E's launcher; a stub until task 22 wires it. */
  openLauncher: () => void;
}

/**
 * Install the global handler. Returns its teardown.
 *
 * Bound to `window` rather than `document` so a `Modal`'s `stopPropagation`
 * on the dialog element reliably beats it: the dialog is inside the document,
 * and a propagation stopped anywhere below `window` never reaches here.
 */
export function installKeys(router: Router, handlers: KeyHandlers): () => void {
  function onkeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      handlers.openLauncher();
      return;
    }
    if (event.key !== "Escape") return;

    const route = router.route;
    // Rung 2 and rung 3 are the same move — back to the room — but they are
    // different states and worth reading as such.
    if (route.view === "room" && route.detail) {
      event.preventDefault();
      router.back();
      return;
    }
    if (route.view !== "room") {
      event.preventDefault();
      router.back();
    }
    // Rung 4: in a room with nothing open, Esc does nothing at all.
  }

  window.addEventListener("keydown", onkeydown);
  return () => window.removeEventListener("keydown", onkeydown);
}
