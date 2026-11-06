/**
 * **A second press lands only where it was aimed** (design system, The press rule; plan of round 14). A press can move
 * what stands under the pointer, or where focus is: a row leaves and the next row's *Done* takes focus, *Log…* slides under
 * a resting pointer, a dialog opens over the button. A person's press does nothing when it lands on a different control from
 * the press before it, within {@link BY_KEY_MS} by key or {@link BY_POINTER_MS} under a still pointer. The same control
 * pressed twice is untouched, because that hold is the page's (`press-hold.svelte.ts`).
 *
 * The wait ends as soon as the person aims: any other key (`Tab`, an arrow, `j`, typing), a pointer moved more than a hand's
 * tremble, or a wheel. Each press it swallows starts the wait over, so mashing a key finishes one row and not one every
 * 400 ms. A held key's repeat never presses anything but a field.
 *
 * Only a person's press: a script's `click()` isn't trusted and goes through, as `still-press.ts` lets it. Beside this guard,
 * `row-hold.ts`, `still-press.ts` and `TfDialog`'s hold stay: they say what a page shows or how long a closed dialog holds.
 */

export const BY_KEY_MS = 400;
export const BY_POINTER_MS = 700;
/** How far a double-click's hand drifts without meaning to go anywhere, in px. */
const TREMBLE = 4;

const CONTROL =
  'a[href], button, input, select, textarea, summary, [contenteditable="true"], [role=button], [role=menuitem], [role=menuitemradio], [role=option], [role=tab], [role=radio], [role=checkbox], [role=switch], [tabindex]:not([tabindex="-1"])';
/** What takes `Space` as a character and a repeated `Enter` as lines. */
const TYPED_IN = 'textarea, [contenteditable="true"], input:not([type=button], [type=checkbox], [type=color], [type=file], [type=radio], [type=range], [type=reset], [type=submit])';
const ALONE = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"]);

/** The control `target` is part of. A label's press arrives again on its field, and the two are one press. */
function controlOf(target: EventTarget | null): Element | null {
  if (!(target instanceof Element)) return null;
  return target.closest("label")?.control ?? target.closest(CONTROL) ?? target.closest("label");
}

const typedIn = (target: EventTarget | null): boolean => target instanceof Element && target.closest(TYPED_IN) !== null;

/** The press before this one. */
interface Press {
  control: Element;
  at: number;
  /** Where the pointer pressed; `null` by key. */
  point: { x: number; y: number } | null;
}

/** `trusted` is what a test swaps, since a test can't send a trusted event. */
export function installPressGuard(trusted: (event: Event) => boolean = (event) => event.isTrusted): () => void {
  let last: Press | null = null;
  /** The pointer press under way was swallowed, from its `pointerdown` to its `dblclick`. */
  let stopping = false;

  const stray = (target: EventTarget | null, ms: number): boolean => {
    if (last === null || Date.now() - last.at >= ms) return false;
    if (target instanceof Node && last.control.contains(target)) return false;
    return controlOf(target) !== last.control;
  };
  const stop = (event: Event) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };

  const onkey = (event: KeyboardEvent) => {
    if (!trusted(event) || event.isComposing || ALONE.has(event.key)) return;
    const typing = typedIn(event.target);
    if (event.key !== "Enter" && (event.key !== " " || typing)) {
      last = null;
      return;
    }
    const control = controlOf(event.target);
    if (event.repeat) {
      if (!typing && (event.key === "Enter" || control !== null)) stop(event);
      return;
    }
    // A modified `Enter` is sent on purpose (⌘Enter), and asks nothing of what focus landed on.
    const modified = event.shiftKey || event.altKey || event.ctrlKey || event.metaKey;
    if (!modified && last !== null && stray(event.target, BY_KEY_MS)) {
      last.at = Date.now();
      stop(event);
      return;
    }
    if (control !== null) last = { control, at: Date.now(), point: null };
  };

  const onpointerdown = (event: PointerEvent) => {
    if (!trusted(event)) return;
    stopping = false;
    if (event.button !== 0) return;
    if (last?.point != null && stray(event.target, BY_POINTER_MS)) {
      stopping = true;
      last.at = Date.now();
      // Not `preventDefault`: a cancelled `pointerdown` skips the `mousedown` that has to be stopped to keep focus where it is.
      event.stopImmediatePropagation();
      return;
    }
    const control = controlOf(event.target);
    if (control !== null) last = { control, at: Date.now(), point: { x: event.clientX, y: event.clientY } };
  };
  /** The rest of a swallowed pointer press. */
  const onrest = (event: Event) => {
    if (stopping && trusted(event)) stop(event);
  };
  const onclick = (event: MouseEvent) => {
    if (!trusted(event)) return;
    // By key the press was noted on its way down; only `Space` waits for the key to come up to act.
    if (event.detail === 0) {
      if (last?.point === null) last.at = Date.now();
      return;
    }
    if (stopping) {
      stop(event);
      return;
    }
    // The wait runs from when the press acted, and a long hold on the button isn't part of it.
    if (last?.point != null && controlOf(event.target) === last.control) last.at = Date.now();
  };

  const onmove = (event: PointerEvent) => {
    if (last === null) return;
    const from = last.point;
    const moved = from === null ? Boolean(event.movementX || event.movementY) : Math.hypot(event.clientX - from.x, event.clientY - from.y) > TREMBLE;
    if (moved) last = null;
  };
  const onwheel = () => {
    last = null;
  };

  window.addEventListener("keydown", onkey, true);
  window.addEventListener("pointerdown", onpointerdown, true);
  window.addEventListener("mousedown", onrest, true);
  window.addEventListener("mouseup", onrest, true);
  window.addEventListener("dblclick", onrest, true);
  window.addEventListener("click", onclick, true);
  window.addEventListener("pointermove", onmove, true);
  window.addEventListener("wheel", onwheel, { capture: true, passive: true });
  return () => {
    window.removeEventListener("keydown", onkey, true);
    window.removeEventListener("pointerdown", onpointerdown, true);
    window.removeEventListener("mousedown", onrest, true);
    window.removeEventListener("mouseup", onrest, true);
    window.removeEventListener("dblclick", onrest, true);
    window.removeEventListener("click", onclick, true);
    window.removeEventListener("pointermove", onmove, true);
    window.removeEventListener("wheel", onwheel, true);
  };
}
