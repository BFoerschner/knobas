/**
 * Which layers Dependencies hides (#871), held for the session as module
 * `$state`, the way Tickets holds *Board / List* (#813): never written to
 * storage or a command, so a restart shows every layer again. #873's saved
 * views take this over and keep it per view.
 */
export interface HiddenLayers {
  readonly ids: ReadonlySet<string>;
  set(layer: string, hidden: boolean): void;
}

export function createHiddenLayers(): HiddenLayers {
  let ids = $state.raw<ReadonlySet<string>>(new Set());
  return {
    get ids() {
      return ids;
    },
    set(layer, hidden) {
      const next = new Set(ids);
      if (hidden) next.add(layer);
      else next.delete(layer);
      ids = next;
    },
  };
}

/** The window's one set. Tests build their own. */
export const hiddenLayers = createHiddenLayers();
