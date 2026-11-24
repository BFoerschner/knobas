/**
 * The XML view's component, held once loaded: it loads with the first document opened, never at start-up; after that a
 * document opened is drawn in the press's own task, as the API explorer's is (`oas-module.ts`).
 */
type DocView = typeof import("./TfDocView.svelte");

let held: DocView | null = null;

/** The view's module: a promise the first time, the module itself once it is in. */
export function docViewModule(): DocView | Promise<DocView> {
  return held ?? import("./TfDocView.svelte").then((loaded) => (held = loaded));
}
