/**
 * The dialogs and drawers the ticket-first shell has open (#809), so `Esc` can
 * close them one at a time, topmost first (`escape.ts`).
 *
 * A dialog or drawer opens itself here with the function that closes it and
 * unregisters on unmount. The Connections drawer and the shell's own dialogs
 * arrive in later tickets; this is the hook they register through.
 */
export interface Layer {
  kind: "dialog" | "drawer";
  close: () => void;
}

export interface Layers {
  readonly open: readonly Layer[];
  /** Register an open layer; the answer unregisters it. */
  add(layer: Layer): () => void;
}

export function createLayers(): Layers {
  // `$state.raw`, not `$state`: a deep proxy of a layer is not the layer, and
  // the unregister below finds its entry by identity.
  let open = $state.raw<Layer[]>([]);
  return {
    get open() {
      return open;
    },
    add(layer) {
      open = [...open, layer];
      return () => {
        open = open.filter((candidate) => candidate !== layer);
      };
    },
  };
}

/** The window's one stack. Tests build their own. */
export const layers = createLayers();
