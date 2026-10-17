/**
 * Dependencies' *Import…* (#1066, R-11.20, R-11.23), pure: how a preview's
 * values read, whether it would do anything, and the toast an import leaves.
 * The dialog is `TfImport.svelte`; the commands are the rooms UI's
 * `ImportDialog` ones, unchanged.
 */
import type { ImportOutcome, ImportPreview, PropertyValue } from "../../ipc/assets";

/** How a property value reads on one line of the preview. */
export function shown(value: PropertyValue | null): string {
  if (value === null) return "—";
  return value.kind === "number" ? String(value.value) : value.value;
}

/** How many things an apply of this preview would write. */
export const changeCount = (preview: ImportPreview): number =>
  preview.new.length + preview.changes.length + preview.monitor_links.length;

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The toast after an import: what it wrote, in the outcome's order. */
export function importedLine(name: string, outcome: ImportOutcome): string {
  const parts = [
    outcome.assets_created > 0 ? `${count(outcome.assets_created, "asset", "assets")} created` : null,
    outcome.routes_created > 0 ? `${count(outcome.routes_created, "route", "routes")} created` : null,
    outcome.properties_set > 0 ? `${count(outcome.properties_set, "property", "properties")} set` : null,
    outcome.properties_kept > 0
      ? `${count(outcome.properties_kept, "property", "properties")} kept as you edited ${outcome.properties_kept === 1 ? "it" : "them"}`
      : null,
    outcome.monitors_linked > 0 ? `${count(outcome.monitors_linked, "monitor", "monitors")} attached` : null,
  ].filter((part) => part !== null);
  return `Imported ${name}: ${parts.length > 0 ? parts.join(", ") : "nothing changed"}`;
}
