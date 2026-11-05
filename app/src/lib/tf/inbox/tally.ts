/**
 * The removal rule's last line (design system, Disabled): repeats collapse into
 * one toast. While a tally's toast is on screen the next press replaces it,
 * holding everything pressed so far, so ten quick answers are one toast and its
 * *Undo* takes back all ten. Once the toast has gone the tally starts again.
 */
import { dismiss, push, toasts, type ToastSpec } from "../../shell/toasts.svelte";

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
