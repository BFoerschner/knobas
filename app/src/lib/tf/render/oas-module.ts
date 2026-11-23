/**
 * The API explorer's component, held once loaded (the ux30 end gate). It loads with the first spec opened, never at
 * start-up; after that a spec opened is drawn in the press's own task: the module itself, not a promise of it, so
 * the file view's `{#await}` draws at once instead of a task (and often a frame) later.
 */
type Explorer = typeof import("./TfOpenApi.svelte");

let held: Explorer | null = null;

/** The explorer's module: a promise the first time, the module itself once it is in. */
export function explorerModule(): Explorer | Promise<Explorer> {
  return held ?? import("./TfOpenApi.svelte").then((loaded) => (held = loaded));
}
