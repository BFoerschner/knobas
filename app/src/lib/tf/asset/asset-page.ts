/**
 * The asset page's rules (`TfAsset.svelte`), pure.
 *
 * The page is one asset read whole: what it is and where it sits in its
 * header, what is stored on it and how it is reached in the main column, and
 * what is in force on it, what breaks with it, what watches it and what it is
 * linked to beside that. *Edit* turns the header, *In force* and *Properties*
 * into one form over `asset-edit.ts`'s rules, saved by one `edit_asset`.
 */
import type { AssetDetail, AssetProperty, AssetStatus, PropertyKind } from "../../ipc/assets";
import type { AssetForm } from "../deps/asset-edit";

/** How a health or a status reads, and its tone. */
export interface Reading {
  word: string;
  tone: "ok" | "warn" | "err" | "muted";
}

/** The design system's tones for an asset's status: up is ok, warn is warn, down is err, none is muted. */
export function readingOf(status: AssetStatus): Reading {
  switch (status) {
    case "up":
      return { word: "up", tone: "ok" };
    case "warn":
      return { word: "warning", tone: "warn" };
    case "down":
      return { word: "down", tone: "err" };
    default:
      return { word: "no reading", tone: "muted" };
  }
}

/** What *Own status* offers, in the asset editor's words. */
export const STATUSES: readonly { value: AssetStatus; label: string }[] = [
  { value: "none", label: "Not set, monitors decide" },
  { value: "up", label: "Up" },
  { value: "warn", label: "Warning" },
  { value: "down", label: "Down" },
];

/**
 * Whether a value is a code value, drawn in mono (design system, *Type*): a
 * URL, or one word of text such as a hostname, an address, an image or a
 * port list. A sentence is prose and stays in the interface face; a number or
 * a date is a figure.
 */
export function isCode(property: AssetProperty): boolean {
  const value = property.value;
  if (value === null) return false;
  if (value.kind === "url") return true;
  return value.kind === "text" && !/\s/.test(value.value.trim());
}

/** A property's value as text; `null` is `null`, drawn as *Not set*. */
export function shownValue(property: AssetProperty): string | null {
  return property.value === null ? null : String(property.value.value);
}

/** The properties in their two groups: what the type declares, then the rest. */
export function groupsOf(properties: readonly AssetProperty[]): { typed: AssetProperty[]; other: AssetProperty[] } {
  return {
    typed: properties.filter((property) => !property.custom),
    other: properties.filter((property) => property.custom),
  };
}

/** How many of the type's own properties are filled in, for the panel's heading. */
export function filledOf(properties: readonly AssetProperty[]): { set: number; of: number } {
  const typed = properties.filter((property) => !property.custom);
  return { set: typed.filter((property) => property.value !== null).length, of: typed.length };
}

/** Long text is edited in a box that wraps; one line in a field. */
export const LONG_TEXT = 60;

/** Whether a form field is drawn as a box rather than a line. */
export function wantsBox(kind: PropertyKind, value: string): boolean {
  return kind === "text" && (value.length > LONG_TEXT || value.includes("\n"));
}

/** What *Inherit* would give for environment and owner: the nearest holder that sets each. */
export function inheritedOf(detail: AssetDetail): { environment: string | null; owner: string | null } {
  const up = [...detail.held_by].reverse();
  const env = up.find((row) => row.environment !== null) ?? null;
  const owner = up.find((row) => row.owner !== null) ?? null;
  return {
    environment: env === null ? null : `${env.environment} from ${env.name}`,
    owner: owner === null ? null : `${owner.owner} from ${owner.name}`,
  };
}

/** A new *Add a property* row, text until the reader picks another kind. */
export function blankRow(): AssetForm["added"][number] {
  return { key: "", value: "", kind: "text" };
}

/** A monitor's state word as Kuma recorded it, in the shell's tones: up is ok, down is err, pending is info, anything else muted. */
export function monitorTone(state: string): Reading["tone"] | "info" {
  if (state === "up") return "ok";
  if (state === "down") return "err";
  if (state === "pending") return "info";
  return "muted";
}
