/**
 * The asset editor's rules (#875, R-11.19, R-11.19a), pure but for the one
 * function that runs the save over the ports it is handed.
 *
 * *Save* sends one `edit_asset` with only the fields that changed (one
 * transaction), then `move_asset` when *Inside* changed, then one
 * `create_link` per new *Depends on*. A failure stops there: the parts before
 * it stay saved, the dialog names the part that did not, and a retry sends only
 * what is left.
 */
import type { AssetDetail, AssetEdit, AssetRow, AssetStatus, AssetType, Environment, PropertyKind, PropertyValue } from "../../ipc/assets";
import { ipcErrorMessage } from "../../ipc";

/** One property row as the form holds it: text, whatever its kind. */
export interface PropertyField {
  key: string;
  label: string;
  kind: PropertyKind;
  value: string;
  custom: boolean;
}

/** Everything the editor's fields hold. */
export interface AssetForm {
  name: string;
  /** *Inside*: the holder's id, or `null` for the top of the estate. */
  parent: string | null;
  /** `null` is *Inherit*. */
  environment: Environment | null;
  /** `""` is *Inherit*. */
  owner: string;
  status: AssetStatus;
  properties: PropertyField[];
  /** *Add a property* rows. */
  added: { key: string; value: string }[];
  /** New *Depends on* targets, by id, in the order they were picked. */
  dependsOn: string[];
}

function textOf(value: PropertyValue | null): string {
  return value === null ? "" : String(value.value);
}

/** The form an asset opens with. */
export function formOf(detail: AssetDetail, types: readonly AssetType[]): AssetForm {
  const type = types.find((t) => t.id === detail.asset.type_id);
  return {
    name: detail.asset.name,
    parent: detail.asset.parent_id,
    environment: detail.asset.environment,
    owner: detail.asset.owner ?? "",
    status: detail.asset.status,
    properties: detail.properties.map((property) => ({
      key: property.key,
      label: property.label,
      kind: property.custom ? "text" : (type?.properties.find((p) => p.key === property.key)?.kind ?? property.value?.kind ?? "text"),
      value: textOf(property.value),
      custom: property.custom,
    })),
    added: [],
    dependsOn: [],
  };
}

function valueOf(label: string, kind: PropertyKind, text: string): PropertyValue | null {
  const value = text.trim();
  if (value === "") return null;
  if (kind === "number") {
    const number = Number(value);
    if (!Number.isFinite(number)) throw new Error(`${label} must be a number.`);
    return { kind, value: number };
  }
  return { kind, value };
}

/** Only what changed, as one `edit_asset` list. Throws for a value that cannot be one. */
export function editsOf(detail: AssetDetail, form: AssetForm): AssetEdit[] {
  const edits: AssetEdit[] = [];
  const asset = detail.asset;
  const name = form.name.trim();
  if (name !== asset.name) {
    if (name === "") throw new Error("A name is needed.");
    edits.push({ field: "name", value: name });
  }
  if (form.status !== asset.status) edits.push({ field: "status", value: form.status });
  if (form.environment !== asset.environment) edits.push({ field: "environment", value: form.environment });
  const owner = form.owner.trim() || null;
  if (owner !== asset.owner) edits.push({ field: "owner", value: owner });
  for (const field of form.properties) {
    const before = detail.properties.find((p) => p.key === field.key);
    // A number field's binding hands back a number (or null when empty), not text.
    const typed = field.value == null ? "" : String(field.value);
    if (typed.trim() === textOf(before?.value ?? null)) continue;
    edits.push({ field: "property", key: field.key, value: valueOf(field.label, field.kind, typed) });
  }
  for (const row of form.added) {
    const key = row.key.trim();
    if (key === "" && row.value.trim() === "") continue;
    if (key === "") throw new Error("A property needs a key.");
    const value = valueOf(key, "text", row.value);
    if (value !== null) edits.push({ field: "property", key, value });
  }
  return edits;
}

/** The three parts a save sends, in order. */
export type SavePart = "fields" | "move" | "links";

/** What a save will send. `move` is `undefined` when *Inside* did not change. */
export interface SavePlan {
  edits: AssetEdit[];
  move: string | null | undefined;
  links: string[];
}

export function planOf(detail: AssetDetail, form: AssetForm): SavePlan {
  return {
    edits: editsOf(detail, form),
    move: form.parent !== detail.asset.parent_id ? form.parent : undefined,
    links: [...form.dependsOn],
  };
}

export interface SavePorts {
  editAsset: (assetId: string, edits: AssetEdit[]) => Promise<AssetRow>;
  moveAsset: (assetId: string, newParentId: string | null) => Promise<AssetRow>;
  createLink: (fromId: string, toId: string, relation?: string) => Promise<unknown>;
}

/** How a save went: the parts that saved, and the one that did not. */
export interface SaveOutcome {
  saved: SavePart[];
  /** The links that went, so a retry sends only the rest. */
  linked: string[];
  failed: { part: SavePart; message: string } | null;
}

/** The relation *Depends on* records. */
export const DEPENDS_ON = "depends-on";

/** Run a save in the spec's order, stopping at the first part that fails. */
export async function runSave(ports: SavePorts, assetId: string, plan: SavePlan): Promise<SaveOutcome> {
  const outcome: SaveOutcome = { saved: [], linked: [], failed: null };
  const step = async (part: SavePart, run: () => Promise<unknown>) => {
    try {
      await run();
      if (!outcome.saved.includes(part)) outcome.saved.push(part);
      return true;
    } catch (error) {
      outcome.failed = { part, message: ipcErrorMessage(error) };
      return false;
    }
  };
  if (plan.edits.length > 0 && !(await step("fields", () => ports.editAsset(assetId, plan.edits)))) return outcome;
  if (plan.move !== undefined && !(await step("move", () => ports.moveAsset(assetId, plan.move ?? null)))) return outcome;
  for (const target of plan.links) {
    if (!(await step("links", () => ports.createLink(assetId, target, DEPENDS_ON)))) return outcome;
    outcome.linked.push(target);
  }
  return outcome;
}

/** What the dialog says when a part did not save, keeping what did. */
export function failureLine(outcome: SaveOutcome): string {
  const failed = outcome.failed;
  if (failed === null) return "";
  const what = { fields: "the fields", move: "the move", links: "what it depends on" }[failed.part];
  const kept = outcome.saved.map((part) => ({ fields: "the fields", move: "the move", links: "some of what it depends on" })[part]);
  const head = kept.length > 0 ? `Saved ${kept.join(" and ")}, but not ${what}` : `Did not save ${what}`;
  return `${head}: ${failed.message}`;
}

/** Why *Delete…* is disabled, or `null` when it is not. */
export function deleteBlock(detail: AssetDetail): string | null {
  const holds = detail.holds.length;
  const exposes = detail.exposes.length;
  const parts = [
    holds > 0 ? `It holds ${holds} ${holds === 1 ? "asset" : "assets"}: move or delete them first.` : "",
    exposes > 0 ? `It exposes ${exposes} ${exposes === 1 ? "route" : "routes"}: delete them first.` : "",
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" ") : null;
}

/** The delete confirm's words, in prototype-6's. */
export function deleteConfirm(detail: AssetDetail): string {
  const lands = detail.reachable_via.filter((route) => route.target_id === detail.asset.id).length;
  const routes = lands > 0 ? ` ${lands} route${lands > 1 ? "s" : ""} landing on it will land on nothing.` : "";
  return `Delete ${detail.asset.name}?${routes} Links to it stay, marked deleted.`;
}
