/**
 * The dialogs and drawers the ticket-first shell has open (#809), so `Esc` can
 * close them one at a time, topmost first (`escape.ts`).
 *
 * A dialog or drawer opens itself here with the function that closes it and
 * unregisters on unmount. The Connections drawer and the shell's own dialogs
 * arrive in later tickets; this is the hook they register through.
 *
 * A `panel` is an inline panel on the page (*Request…*'s list, a file's
 * *Pipeline*): `Esc` closes it before it leaves the page, but it does not own
 * the keyboard as a dialog does, so the page's keys and history still work.
 */
export interface Layer {
  kind: "dialog" | "drawer" | "panel";
  close: () => void;
}

export interface Layers {
  readonly open: readonly Layer[];
  /** Register an open layer; the answer unregisters it. */
  add(layer: Layer): () => void;
  /** Tests only: forget every open layer without closing it (#1743). */
  reset(): void;
}

export function createLayers(): Layers {
  // The stack itself is a plain array, and `open` only publishes it. Read back
  // inside an effect's teardown during a flush, the signal can still give the
  // value from before this flush's writes, so a pop that filtered it dropped
  // the push of a menu opening in the same flush, and a push could bring back
  // a layer already gone (the stuck menu, ux27, 2026-11-21).
  let stack: Layer[] = [];
  // `$state.raw`, not `$state`: a deep proxy of a layer is not the layer, and
  // the unregister below finds its entry by identity.
  let open = $state.raw<Layer[]>(stack);
  const publish = (next: Layer[]) => {
    stack = next;
    open = next;
  };
  return {
    get open() {
      return open;
    },
    add(layer) {
      publish([...stack, layer]);
      return () => publish(stack.filter((candidate) => candidate !== layer));
    },
    reset() {
      publish([]);
    },
  };
}

/** The window's one stack. Tests build their own. */
export const layers = createLayers();
