/**
 * What an asset's or a route's history line says, in one sentence (#429).
 *
 * Moved out of the tree's pane when the asset page took the history over: the
 * page draws the asset's lines and a picked route's, and both read the same
 * activity rows.
 */
import type { AssetDetail } from "../ipc/assets";

function said(value: unknown): string {
  if (value === null || value === undefined) return "nothing";
  if (typeof value === "object" && value !== null && "value" in value) {
    return String((value as { value: unknown }).value);
  }
  return String(value);
}

/**
 * What an asset id is *called*, out of the rows the page already has.
 *
 * A move's history line carries the new parent's **id**, which is the only
 * thing the backend can honestly write down: the name at the moment of the
 * move is not the name now. After the move that parent is on the held-by
 * path, so the line can say `Moved to knobas-jira` instead of `Moved to
 * asset:knobas-jira`. `null` when it is not, and the id is then drawn as it
 * is rather than guessed at.
 */
export function nameIn(detail: AssetDetail | null, value: unknown): string | null {
  if (detail === null || typeof value !== "string") return null;
  const known = [detail.asset, ...detail.held_by, ...detail.holds];
  return known.find((row) => row.id === value)?.name ?? null;
}

/** One history line, with `nameOf` answering an asset id's name where it is known. */
export function historyLine(verb: string, raw: unknown, nameOf: (value: unknown) => string | null): string {
  const fields = (raw ?? {}) as Record<string, unknown>;
  if (verb === "created") return "Created";
  if (verb === "deleted") return "Deleted";
  if (verb === "moved") return `Moved to ${nameOf(fields.to) ?? said(fields.to)}`;
  // The two lines an import writes (#439). Neither is an old-to-new edit, so
  // both fell through to the composition below and rendered as `nothing:
  // nothing → nothing` on every asset in an imported estate.
  if (verb === "imported") {
    return fields.estate === undefined ? "Imported" : `Imported from ${said(fields.estate)}`;
  }
  // The alert pair (#446): *somebody saw it* and *it healed* are different words.
  if (verb === "acked") return `Alert acked: ${said(fields.monitor_name)} is ${said(fields.state)}`;
  if (verb === "recovered") return `Alert closed: ${said(fields.monitor_name)} recovered`;
  if (verb === "edited" && fields.field === "monitors" && Array.isArray(fields.added)) {
    // *Kept*, not *added*: a name is written down on the asset precisely
    // because no monitor answers to it yet (spec #427).
    return `monitors: kept ${fields.added.map(said).join(", ")}`;
  }
  const field = fields.field === "property" ? said(fields.key) : said(fields.field);
  // A route's `target` edit carries asset **ids** on both sides, for the
  // reason a move's line does: the name at the moment of the edit is not the
  // name now.
  const shown = (value: unknown): string => (fields.field === "target" ? (nameOf(value) ?? said(value)) : said(value));
  return `${field}: ${shown(fields.from)} → ${shown(fields.to)}`;
}
