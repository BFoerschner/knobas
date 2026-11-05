/**
 * The removal rule's second way (design system, Disabled): a press that removes
 * something acts at once and offers *Undo* on its toast, and repeats collapse
 * into one toast, whose *Undo* puts them all back.
 */
import { push, toasts } from "../../shell/toasts.svelte";

/** One thing removed, and where it stood in its list. */
export interface Removed<T> {
  name: string;
  at: number;
  item: T;
}

/** `list` with `removed` back in their places, the last removed first, so each index means what it meant. */
export function putBack<T>(list: readonly T[], removed: readonly Removed<T>[]): T[] {
  const out = [...list];
  for (const { at, item } of [...removed].reverse()) out.splice(Math.min(at, out.length), 0, item);
  return out;
}

/**
 * What a list calls after each removal: `key` names the toast, `plural` is what
 * the collapsed one counts (*Removed 2 renderers*), and `restore` gets every
 * removal the toast still stands for.
 */
export function removalToast<T>(key: string, plural: string, restore: (removed: readonly Removed<T>[]) => void) {
  let live: { id: number; removed: Removed<T>[] } | null = null;
  return (one: Removed<T>): void => {
    const held = live !== null && toasts.items.some((toast) => toast.id === live?.id) ? live.removed : [];
    const removed = [...held, one];
    const id = push({
      key,
      text: removed.length === 1 ? `Removed ${one.name}` : `Removed ${removed.length} ${plural}`,
      action: {
        label: "Undo",
        run: () => {
          live = null;
          restore(removed);
        },
      },
    });
    live = { id, removed };
  };
}
