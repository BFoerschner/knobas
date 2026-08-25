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
  /** How long before it dismisses itself. */
  ms?: number;
}

export interface ToastItem extends ToastSpec {
  id: number;
}

/** Long enough to read a sentence, short enough not to stack up. */
const DEFAULT_MS = 6500;

let nextId = 0;

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
  setTimeout(() => dismiss(id), ms);
  return id;
}

/**
 * Remove a toast. Idempotent: the auto-dismiss timer always fires, including
 * for a toast the reader has already closed by hand.
 */
export function dismiss(id: number): void {
  toasts.items = toasts.items.filter((toast) => toast.id !== id);
}
