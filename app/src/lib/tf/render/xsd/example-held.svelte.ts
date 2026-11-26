/**
 * An example opened in the XML view (step E1; Fable's ruling, 2026-11-24): a place of its schema, `example/ORDER`,
 * that the schema explorer draws over itself. Small on purpose: the file header reads it before the explorer loads.
 *
 * - **The settings an example is made with** are held until knobas quits, by repository, path and content, as folds
 *   are: an address that comes back to the example after its explorer was dropped makes the same document again.
 * - **What the header says** of the example shown: its file's name, and how it was made.
 */
import type { ExampleOptions } from "../../../render/xsd-example";

/** The place an opened example is at: `example/` and the element's name as `el/` writes it. */
export const EXAMPLE_PLACE = "example/";

/** The element an `example/` place names, or `null` for any other place. */
export function exampleOfPlace(place: string | null | undefined): string | null {
  if (place == null || !place.startsWith(EXAMPLE_PLACE)) return null;
  const name = place.slice(EXAMPLE_PLACE.length);
  return name === "" ? null : name;
}

const held = new Map<string, ExampleOptions>();

/** The settings held for a schema (`repository\npath\ncontent`), if any were changed from the first ones. */
export function heldOptions(key: string): ExampleOptions | undefined {
  return held.get(key);
}

/** Holds a schema's settings until knobas quits. */
export function holdOptions(key: string, options: ExampleOptions): void {
  held.set(key, options);
}

/** Forgets every held setting (tests). */
export function forgetHeld(): void {
  held.clear();
  shown.head = null;
}

/** An example's file name, as the maker names it: the element's local name, the seed. */
export function exampleFileName(element: string, seed: number): string {
  // An XML name's prefix goes, as the maker's `root.name` has none: not an entity id's key.
  return `${element.replace(/^[^:]*:/, "")}-example-${seed}.xml`;
}

/** How an example was made, in the header's meta: what the made-by comment says, its lines, and that it is not saved. */
export function exampleMeta(options: Pick<ExampleOptions, "seed" | "optional">, lines: number | null): string {
  const parts = options.optional === "all" ? "every optional part" : "required parts only";
  return `Example made by knobas, seed ${options.seed}, ${parts}.${lines === null ? "" : ` ${lines.toLocaleString("en")} lines.`} Not saved.`;
}

class Shown {
  /** What the file header says of the opened example; `null` where none is opened. Raw: its owner clears it by identity. */
  head = $state.raw<{ place: string; name: string; meta: string; xml: string } | null>(null);
}

/** What the file header says of the opened example. */
export const shown = new Shown();
