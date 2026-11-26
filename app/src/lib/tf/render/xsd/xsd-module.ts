/**
 * The schema explorer's component, held once loaded: it loads with the first schema opened, never at start-up; after
 * that a schema opened is drawn in the press's own task, as the API explorer's is (`oas-module.ts`).
 */
type Explorer = typeof import("./TfXsd.svelte");

let held: Explorer | null = null;

/** The explorer's module: a promise the first time, the module itself once it is in. */
export function xsdModule(): Explorer | Promise<Explorer> {
  return held ?? import("./TfXsd.svelte").then((loaded) => (held = loaded));
}
