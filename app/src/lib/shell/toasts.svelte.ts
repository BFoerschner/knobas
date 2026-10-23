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
}

export interface ToastItem extends ToastSpec {
  id: number;
}

/** Long enough to read a sentence, short enough not to stack up. */
const DEFAULT_MS = 6500;

let nextId = 0;

/** The armed auto-dismiss timer of each live toast, by id. */
const timers = new Map<number, ReturnType<typeof setTimeout>>();

/**
 * The live stack, oldest first.
 *
 * A `$state` object with an `items` array rather than a bare `$state([])`: an
 * exported `let` cannot be reassigned from outside its module, so the store
 * needs one stable object whose field moves.
 */
export const toasts = $state<{ items: ToastItem[] }>({ items: [] });

/** Raise a toast. Returns its id, so a caller can dismiss it early. */
export function push(spec: ToastSpec): number {
  const id = ++nextId;
  toasts.items.push({ ...spec, id });
  const ms = spec.ms ?? DEFAULT_MS;
  timers.set(
    id,
    setTimeout(() => dismiss(id), ms),
  );
  return id;
}

/**
 * Drop the stack: a test starts on an empty screen. The dropped toasts'
 * dismiss timers are cleared, and the ids keep counting so a timer that
 * somehow survives could never remove a later toast (#1762).
 */
export function reset(): void {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  toasts.items = [];
}

/**
 * Remove a toast. Idempotent: the auto-dismiss timer always fires, including
 * for a toast the reader has already closed by hand.
 */
export function dismiss(id: number): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  toasts.items = toasts.items.filter((toast) => toast.id !== id);
}
