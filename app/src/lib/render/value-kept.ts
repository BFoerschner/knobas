/**
 * A chain kept for a field (the XML view's design, 2026-11-23, section 18): a named pipeline that says which fields it
 * is for, by how their names end and in which files. Every field whose names end that way, in every file the pattern
 * takes, in any repository, opens on it.
 *
 * - **A field is named by how its names end**: `MIME_EMBEDDED/MIME_DATA`, `Signature/@Id`, and in JSON
 *   `attachments[].content`. Positions are never part of it. The same `MIME_DATA` stands under an order's header, under
 *   each item and in an invoice, so the end of the path and not the whole of it.
 * - **The longest match wins**: of two kept chains, the one that names more of the field.
 * - **Pressed, never by itself** (Björn's answer 10): a kept chain is a chip that opens on a press; nothing is decoded
 *   that nobody looked at. {@link KEPT_APPLIES_BY_ITSELF} is that answer.
 */
import type { KeptField, NamedPipeline } from "../ipc/app";
import { accepts } from "./registry";

/** Björn's answer 10 (2026-11-23): a kept chain stays a chip that opens on a press. `true` would draw it by itself. */
export const KEPT_APPLIES_BY_ITSELF = false;

/** Whether a field's names `path` end as `field` says: whole names only, `/` or `.` between them. */
export function endsAs(path: string, field: string): boolean {
  if (field === "" || path.length < field.length || !path.endsWith(field)) return false;
  if (path.length === field.length) return true;
  const before = path[path.length - field.length - 1];
  // `[]` closes an array's item: `attachments[].content` ends as `content` and as `[].content`.
  return before === "/" || before === "." || (field.startsWith("[") && before !== undefined);
}

/** The chain kept for the field `path` in the file `fileName`, the longest field first; `null` for none. */
export function keptFor(path: string, fileName: string, pipelines: readonly NamedPipeline[]): { pipeline: NamedPipeline; kept: KeptField } | null {
  let best: { pipeline: NamedPipeline; kept: KeptField } | null = null;
  for (const pipeline of pipelines) {
    for (const kept of pipeline.fields ?? []) {
      if (!accepts(kept.files, fileName) || !endsAs(path, kept.field)) continue;
      if (best === null || kept.field.length > best.kept.field.length) best = { pipeline, kept };
    }
  }
  return best;
}

/** The field a chain is offered for, from a value's names: the last two, `MIME_EMBEDDED/MIME_DATA`, `Signature/@Id`. */
export function offeredField(path: string): string {
  const xml = path.includes("/") || !path.includes(".");
  if (xml) return path.split("/").slice(-2).join("/");
  // JSON: the last two members, an array's items written `[]` after its name.
  return path.split(".").slice(-2).join(".");
}
