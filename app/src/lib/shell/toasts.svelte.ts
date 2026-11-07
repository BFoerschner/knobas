/**
 * The toast stack.
 *
 * A module-level rune rather than a context, because a toast is raised from
 * places that are not components — a keyboard handler, a lifecycle retry, an
 * `invoke` rejection caught in a plain function — and threading a context to
 * each of them would be ceremony around a list of at most a handful of items.
 */

/** A button on a toast: *Retry*, *Re-enter password*, *Undo*. */
export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastSpec {
  /**
   * Rendered as **text**. A toast routinely carries an `IpcError.message`,
   * which comes from a source system and is therefore untrusted (gotcha 7).
   */
  text: string;
  tone?: "plain" | "err";
  action?: ToastAction;
  /**
   * More buttons after {@link action}, in order: a write the queue refused
   * offers both *Open write queue* and *Edit and resend* (#752).
   */
  more?: ToastAction[];
  /** How long before it dismisses itself. */
  ms?: number;
  /** Runs once when it goes, however it goes: see {@link heldWrite}. */
  onend?: () => void;
  /**
   * What the toast is about: one setting, one page's publication, one read. A new toast with the same subject takes the
   * live one's place, in its new words and with the first *Undo*, so *Undo* goes back to before the first press (F-648).
   */
  subject?: string;
}

export interface ToastItem extends Omit<ToastSpec, "onend"> {
  id: number;
}

/** Long enough to read a sentence, short enough not to stack up. */
const DEFAULT_MS = 6500;
const UNDO = "Undo";
/** What a toast keeps once the pointer or focus has left it: enough to reach back for *Undo*. */
const AFTER_HOLD_MS = 2000;

let nextId = 0;

/** What keeps a toast on screen past its time. */
export type ToastHold = "hover" | "focus";

/** A live toast's auto-dismiss: armed, or stopped with what it had left while something holds it. */
interface ToastTimer {
  handle: ReturnType<typeof setTimeout> | null;
  /** When an armed timer fires; once stopped, what was left of it. */
  ends: number;
  left: number;
  holds: Set<ToastHold>;
}

const timers = new Map<number, ToastTimer>();

function arm(id: number, ms: number): ReturnType<typeof setTimeout> {
  return setTimeout(() => dismiss(id), ms);
}

/**
 * The live stack, oldest first.
 *
 * A `$state` object with an `items` array rather than a bare `$state([])`: an
 * exported `let` cannot be reassigned from outside its module, so the store
 * needs one stable object whose field moves.
 */
export const toasts = $state<{ items: ToastItem[] }>({ items: [] });

/** What runs when a toast ends, for a write held until then ({@link heldWrite}). */
const endings = new Map<number, () => void>();

/**
 * The same words again while a toast still says them: the one toast stands and its time starts over (the removal rule).
 * Its buttons are the newest press's. Two *Undo*s are one that takes back both, because each press has something to undo.
 */
function again(toast: ToastItem, spec: ToastSpec, ms: number): number {
  const before = toast.action;
  const action = spec.action;
  if (before?.label === UNDO && action?.label === UNDO) toast.action = { label: UNDO, run: () => { before.run(); action.run(); } };
  else if (action === undefined) delete toast.action;
  else toast.action = action;
  if (spec.more === undefined) delete toast.more;
  else toast.more = spec.more;
  if (spec.subject !== undefined) toast.subject = spec.subject;
  return standsAgain(toast.id, spec, ms);
}

/** What a toast standing again adds to itself: its ending, and a new time. */
function standsAgain(id: number, spec: ToastSpec, ms: number): number {
  const ending = endings.get(id);
  if (spec.onend !== undefined) endings.set(id, ending === undefined ? spec.onend : () => { ending(); spec.onend?.(); });
  const timer = timers.get(id)!;
  timer.left = ms;
  if (timer.handle !== null) {
    clearTimeout(timer.handle);
    timer.handle = arm(id, ms);
    timer.ends = Date.now() + ms;
  }
  return id;
}

/** A new toast about what `toast` is about: the new words where it stands, and the first *Undo*, so it undoes all of them. */
function replaced(toast: ToastItem, spec: ToastSpec, ms: number): number {
  toast.text = spec.text;
  if (spec.tone === undefined) delete toast.tone;
  else toast.tone = spec.tone;
  const action = toast.action?.label === UNDO ? toast.action : spec.action;
  if (action === undefined) delete toast.action;
  else toast.action = action;
  if (spec.more === undefined) delete toast.more;
  else toast.more = spec.more;
  return standsAgain(toast.id, spec, ms);
}

/**
 * Raise a toast. Returns its id, so a caller can dismiss it early. A toast about a live toast's subject takes its place; a
 * repeat of a live toast's words is that toast again.
 */
export function push(spec: ToastSpec): number {
  const ms = spec.ms ?? DEFAULT_MS;
  const about = spec.subject === undefined ? undefined : toasts.items.find((item) => item.subject === spec.subject);
  if (about !== undefined) return replaced(about, spec, ms);
  const same = toasts.items.find((item) => item.text === spec.text && (item.tone ?? "plain") === (spec.tone ?? "plain"));
  if (same !== undefined) return again(same, spec, ms);
  const id = ++nextId;
  const { onend, ...item } = spec;
  toasts.items.push({ ...item, id });
  if (onend !== undefined) endings.set(id, onend);
  timers.set(id, { handle: arm(id, ms), ends: Date.now() + ms, left: ms, holds: new Set() });
  return id;
}

/**
 * A press that acted at once, with its way back. `undo` runs on *Undo*. Repeats of the same words are one toast whose
 * *Undo* takes all of them back.
 */
export function undoable(text: string, undo: () => void, spec: Omit<ToastSpec, "text" | "action" | "onend"> = {}): number {
  return push({ ...spec, text, action: { label: UNDO, run: undo } });
}

/**
 * A press with no reverse command behind it: the page hides the thing at once, and `send` runs when the toast ends,
 * unless *Undo* was pressed, when `undo` puts the thing back and nothing is ever sent. A quit inside those seconds sends
 * nothing, so use it only where that is harmless.
 */
export function heldWrite(text: string, send: () => void, undo: () => void, spec: Omit<ToastSpec, "text" | "action" | "onend" | "subject"> = {}): number {
  let undone = false;
  return push({
    ...spec,
    text,
    action: {
      label: UNDO,
      run: () => {
        undone = true;
        undo();
      },
    },
    onend: () => {
      if (!undone) send();
    },
  });
}

/** Change what a live toast says, in place and on its time: the write it named has moved on. Gone already, nothing happens. */
export function amend(id: number, spec: Pick<ToastSpec, "text" | "action">): void {
  const toast = toasts.items.find((item) => item.id === id);
  if (toast === undefined) return;
  // Two presses of one write, the second now saying what the first says: one toast, as a repeat is.
  const twin = toasts.items.find((item) => item.id !== id && item.text === spec.text && (item.tone ?? "plain") === (toast.tone ?? "plain"));
  if (twin !== undefined) dismiss(twin.id);
  toast.text = spec.text;
  if (spec.action === undefined) delete toast.action;
  else toast.action = spec.action;
}

/** The pointer or focus is on a toast: its time stops, so an *Undo* being reached for stays (F-621). */
export function hold(id: number, why: ToastHold): void {
  const timer = timers.get(id);
  if (timer === undefined) return;
  timer.holds.add(why);
  if (timer.handle === null) return;
  clearTimeout(timer.handle);
  timer.handle = null;
  timer.left = Math.max(0, timer.ends - Date.now());
}

/** It left. With nothing else holding the toast, its time runs again, and never for less than it takes to reach back. */
export function release(id: number, why: ToastHold): void {
  const timer = timers.get(id);
  if (timer === undefined) return;
  timer.holds.delete(why);
  if (timer.holds.size > 0 || timer.handle !== null) return;
  const ms = Math.max(timer.left, AFTER_HOLD_MS);
  timer.handle = arm(id, ms);
  timer.ends = Date.now() + ms;
}

/**
 * Drop the stack: a test starts on an empty screen. The dropped toasts'
 * dismiss timers are cleared, and the ids keep counting so a timer that
 * somehow survives could never remove a later toast (#1762).
 */
export function reset(): void {
  for (const timer of timers.values()) if (timer.handle !== null) clearTimeout(timer.handle);
  timers.clear();
  endings.clear();
  toasts.items = [];
}

/**
 * Remove a toast. Idempotent: the auto-dismiss timer always fires, including
 * for a toast the reader has already closed by hand.
 */
export function dismiss(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined && timer.handle !== null) clearTimeout(timer.handle);
  timers.delete(id);
  toasts.items = toasts.items.filter((toast) => toast.id !== id);
  const ending = endings.get(id);
  endings.delete(id);
  ending?.();
}

/**
 * Repeats that collapse into one toast (the removal rule, design system,
 * Disabled). While a tally's toast is on screen the next press replaces it,
 * holding everything pressed so far, so ten quick answers are one toast and its
 * *Undo* takes back all ten. Once the toast has gone the tally starts again.
 */
export function tally<T>(draw: (held: readonly T[]) => ToastSpec): (one: T) => void {
  let id: number | null = null;
  let held: T[] = [];
  return (one) => {
    if (id !== null && toasts.items.some((toast) => toast.id === id)) dismiss(id);
    else held = [];
    held = [...held, one];
    id = push(draw(held));
  };
}
