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
 * 4. a tile is maximised (#250)    → the room draws its grid again;
 * 5. otherwise                     → nothing.
 *
 * Rung 5 is a rule, not an omission: `Esc` must never exit the app, quit a
 * context, or discard anything. A key that sometimes does nothing is what
 * makes the other four safe to press.
 *
 * Rung 4 is the one rung the address cannot answer: a maximised tile is a
 * viewing gesture the room keeps to itself, so the ladder asks through
 * `restoreTile` and the answer says whether the press counted. It sits below
 * the detail on purpose -- a detail open over a maximised tile closes first,
 * and the tile is still there for the next press.
 *
 * `⌘[` / `⌘]` (and `⌘←` / `⌘→`) are history's back and forward (#693), and
 * so are the mouse's own back and forward buttons. They are not rungs either:
 * `Esc` closes things, and back goes to the previous address, which may be
 * the detail `Esc` just closed. Outside a text field only, because a field
 * owns those keys (outdent, start of line).
 *
 * `⌘T` is the timer (#278). It is not part of the ladder: it is a verb, not an
 * unwind, and it acts wherever the reader is.
 */
import type { Router } from "./router.svelte";

export interface KeyHandlers {
  /**
   * `⌘K` / `Ctrl+K`.
   *
   * ## The whole contract between the shell and the launcher
   *
   * The shell owns **opening and closing** the overlay and the `Esc` rung
   * ordering; the launcher owns **everything inside it**. That line is drawn
   * here because the two live in different trees (`lib/shell/**` and
   * `lib/launcher/**`), and a launcher whose keyboard lived in the shell would
   * be one behaviour with two owners.
   *
   * Consequences worth stating rather than rediscovering:
   *
   * * The launcher is **rung 1**, above the detail slide-over. It binds `⌘K`
   *   itself and unwinds its own `Esc` (a non-empty query is cleared first,
   *   and only an already-empty box closes the overlay), calling
   *   `stopPropagation` on both rungs — so while the overlay is up this
   *   handler never sees the key. That is what keeps one keystroke from
   *   unwinding two ladders, which is indistinguishable from a bug.
   * * Both sides bind `⌘K` and they converge: this sets the flag, the
   *   component toggles it, and both land on "open" from a closed box.
   */
  openLauncher: () => void;
  /**
   * Rung 4: restore the grid if a tile is maximised (#250).
   *
   * Returns whether it did anything. The room owns the state and the redraw;
   * this handler is the shell reaching it, the way `openLauncher` reaches the
   * launcher, so the ladder stays router-driven and asks only when every
   * rung above has passed. `false` is what lets the key fall through to
   * rung 5 -- a press that restored nothing must not be `preventDefault`ed
   * as though it had.
   */
  restoreTile: () => boolean;
  /**
   * `⌘T` / `Ctrl+T` — the timer (#278).
   *
   * ## Where the three behaviours live, and why not here
   *
   * One keystroke, three outcomes: a running timer stops, a foreground entity
   * starts, and neither opens the picker. **None of that is decided here.**
   * This handler says *the key was pressed*; `shell/timer.svelte.ts`'s
   * `press()` decides what it means, because deciding needs the running timer
   * and the foreground, and a keyboard that read both would be a second owner
   * of the timer's state.
   *
   * The same line the `⌘K` contract above draws between the shell and the
   * launcher, for the same reason.
   *
   * It is bound **unconditionally**, above the `Esc` ladder and outside it: a
   * verb is not a rung.
   *
   * **Nothing currently intercepts it, `Modal.svelte` included** — that
   * component stops propagation for `Escape` alone — so ⌘T pressed inside
   * ⌘T's own picker does reach this handler. It is harmless in the state the
   * picker opens in: the picker opens only when nothing is running and
   * nothing is in front of the reader, so `press()` answers `"pick"` again and
   * the shell re-opens a dialog that is already up. It stops being harmless
   * the moment another surface can start a timer while the picker is open,
   * which is what #282's passive attribution brings; the fix then is a
   * `stopPropagation` in the modal that wants the key, not a rung here.
   */
  toggleTimer: () => void;
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
    const step = historyStep(event);
    if (step !== null) {
      event.preventDefault();
      if (step === "back") router.goBack();
      else router.goForward();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      handlers.openLauncher();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "t") {
      // `preventDefault` unconditionally: on a Mac ⌘T is the browser's
      // new-tab, which does nothing in a Tauri window, and on Linux Ctrl+T is
      // the same. Letting it through would be letting a keystroke mean two
      // things depending on the build.
      event.preventDefault();
      handlers.toggleTimer();
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
      return;
    }
    // Rung 4: a maximised tile, which only the room knows about.
    if (handlers.restoreTile()) {
      event.preventDefault();
      return;
    }
    // Rung 5: in a room with nothing open, Esc does nothing at all.
  }

  /**
   * The mouse's back (3) and forward (4) buttons. Navigated on `mouseup`, the
   * moment a browser does; every event of the press is `preventDefault`ed so
   * the webview adds no navigation or selection of its own.
   */
  function onmouse(event: MouseEvent) {
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    if (event.type !== "mouseup") return;
    if (event.button === 3) router.goBack();
    else router.goForward();
  }

  const mouse = ["mousedown", "mouseup", "auxclick"] as const;
  window.addEventListener("keydown", onkeydown);
  for (const type of mouse) window.addEventListener(type, onmouse);
  return () => {
    window.removeEventListener("keydown", onkeydown);
    for (const type of mouse) window.removeEventListener(type, onmouse);
  };
}

/** Whether a key press is history's back or forward, or neither (#693). */
function historyStep(event: KeyboardEvent): "back" | "forward" | null {
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

/** A field that types: its keys are its own. */
function inTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  )
    return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return ![
    "button",
    "checkbox",
    "radio",
    "submit",
    "reset",
    "range",
    "color",
    "file",
  ].includes(target.type);
}
