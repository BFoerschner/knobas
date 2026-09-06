/**
 * The estate — `crates/knobas-app/src/commands/assets.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a field, a union member or a command name added on
 * one side only fails `cargo test`, not merely `svelte-check`.
 *
 * A §10.8-ratified module pair on the `time` module's precedent (issue #428).
 * **Every** asset command belongs here — the route commands #432 adds, the
 * import #439 adds and M4.1's alert reads as well as these six — so the bridge
 * grows one module rather than one more section of `entity.ts` per ticket.
 *
 * There is no asset *event*. Every mutation writes a line to the activity
 * stream and the shell learns from the signal it already watches
 * (`EVENTS.activityNew`, read by `shell/latest-change.svelte.ts`). A channel
 * of the estate's own would be a second thing to keep in step with the first.
 */
import { invoke } from "@tauri-apps/api/core";

import type { ActivityRow } from "./entity";

/**
 * What a property value may be — `knobas_core::asset::PropertyKind`.
 *
 * The four story 6 names and no fifth; the *secret* kind is deferred with its
 * keychain convention, so there is nowhere in an asset a credential can be
 * typed.
 */
export type PropertyKind = "text" | "number" | "date" | "url";

/** Free text — a hostname, an image tag, a size. */
export interface TextProperty {
  kind: "text";
  /** Never blank: clearing a property is `null`, not `""`. */
  value: string;
}

/** A number — a port, a row count. Finite; JSON has no other kind. */
export interface NumberProperty {
  kind: "number";
  value: number;
}

/** A date, `YYYY-MM-DD`. Refused by the backend if it is not one. */
export interface DateProperty {
  kind: "date";
  value: string;
}

/** An http(s) URL. Refused by the backend if it carries no scheme. */
export interface UrlProperty {
  kind: "url";
  value: string;
}

/**
 * One property value — `assets::PropertyValue`.
 *
 * Tagged, and the tag is stored beside the value: a bare scalar would make
 * `"8080"` and `8080` the same property and would leave a date
 * indistinguishable from a string.
 */
export type PropertyValue = TextProperty | NumberProperty | DateProperty | UrlProperty;

/**
 * An asset's **own** status — `assets::AssetStatus`.
 *
 * Not its health: that is the worst of this and its monitors' states, and it
 * arrives with #431 and M4.1. `"none"` means *nobody has said*, which is a
 * different thing from `"up"`.
 */
export type AssetStatus = "up" | "warn" | "down" | "none";

/**
 * Which environment an asset belongs to — `assets::Environment`.
 *
 * Stored on the asset. The inheritance from the nearest ancestor that sets it
 * is #431's; until then a `null` here means only *not set here*.
 */
export type Environment = "dev" | "stage" | "prod" | "shared";

/**
 * One row of a Miller column — `assets::AssetRow`.
 *
 * `Row` and not `Node`: `CONTEXT.md`'s **Asset** entry lists *node* among the
 * words to avoid, and every list line this app already draws is a `…Row`.
 *
 * `type_label` and `monogram` are resolved by the backend out of the built-in
 * type table, so no surface here keeps a copy of nineteen types and their
 * chips.
 */
export interface AssetRow {
  /** `asset:<uuid>`, and also this asset's entity id. */
  id: string;
  /** The asset that holds this one; `null` at the top of the estate. */
  parent_id: string | null;
  type_id: string;
  type_label: string;
  /** Two characters — the type's chip. `"??"` for a type this build lost. */
  monogram: string;
  name: string;
  status: AssetStatus;
  /** Set **on this asset**; `null` where it inherits (#431). */
  environment: Environment | null;
  /** Set **on this asset**; `null` where it inherits (#431). */
  owner: string | null;
  /** Whether anything sits under it — what the chevron and the next column
   * are drawn from. */
  has_children: boolean;
}

/**
 * One row of the pane's property list — `assets::AssetProperty`.
 *
 * Typed properties come first in the type's declared order, **including the
 * ones nobody has filled in** — that is how a reader learns a VM is meant to
 * have an IP. Custom keys follow, by key, with `custom: true`.
 */
export interface AssetProperty {
  key: string;
  /** The type's label for a declared key; the key itself for a custom one. */
  label: string;
  /** `null` for a declared property with no value yet. */
  value: PropertyValue | null;
  custom: boolean;
}

/** Everything the fixed right pane draws — `assets::AssetDetail`. */
export interface AssetDetail {
  asset: AssetRow;
  properties: AssetProperty[];
  /** Outermost first, **excluding the asset itself**. Empty at the top. */
  held_by: AssetRow[];
  holds: AssetRow[];
  /** This asset's own activity lines, newest first. */
  history: ActivityRow[];
}

/** Rename — `assets::AssetEdit::Name`. */
export interface NameEdit {
  field: "name";
  value: string;
}

/** Set the asset's own status — `assets::AssetEdit::Status`. */
export interface StatusEdit {
  field: "status";
  value: AssetStatus;
}

/** Set or clear the environment — `assets::AssetEdit::Environment`. */
export interface EnvironmentEdit {
  field: "environment";
  /** `null` clears it, so the asset inherits again. */
  value: Environment | null;
}

/** Set or clear the owner — `assets::AssetEdit::Owner`. */
export interface OwnerEdit {
  field: "owner";
  /** `null` clears it. */
  value: string | null;
}

/** Set or clear one property — `assets::AssetEdit::Property`. */
export interface PropertyEdit {
  field: "property";
  key: string;
  /** `null` removes the key. Blank text is refused: clear it instead. */
  value: PropertyValue | null;
}

/**
 * One field changing — `assets::AssetEdit`.
 *
 * A discriminated union rather than a bag of optionals, because each edit
 * writes its own history line with a `from` and a `to`, and a bag would make
 * "cleared" and "not mentioned" the same value on the wire.
 */
export type AssetEdit = NameEdit | StatusEdit | EnvironmentEdit | OwnerEdit | PropertyEdit;

/**
 * One Miller column: what `parentId` holds.
 *
 * Omitted (or `null`) is the estate's **top level** — the first column — not
 * "every asset".
 */
export function assetTree(parentId?: string | null): Promise<AssetRow[]> {
  return invoke<AssetRow[]>("asset_tree", { parentId: parentId ?? null });
}

/** One asset, with its properties, its held-by path, what it holds, and its history. */
export function getAsset(assetId: string): Promise<AssetDetail> {
  return invoke<AssetDetail>("get_asset", { assetId });
}

/**
 * Create an asset under `parentId`, or at the top of the estate.
 *
 * The id is minted by the backend: creating is not naming.
 */
export function createAsset(
  typeId: string,
  name: string,
  parentId?: string | null,
  properties?: [string, PropertyValue][],
): Promise<AssetRow> {
  return invoke<AssetRow>("create_asset", {
    parentId: parentId ?? null,
    typeId,
    name,
    properties: properties ?? null,
  });
}

/** Apply a list of edits, each one a history line with its old and new value. */
export function editAsset(assetId: string, edits: AssetEdit[]): Promise<AssetRow> {
  return invoke<AssetRow>("edit_asset", { assetId, edits });
}

/**
 * Move an asset under another, or to the top of the estate.
 *
 * A move that would make a cycle rejects with `invalid`, naming the asset it
 * would have run into.
 */
export function moveAsset(assetId: string, newParentId: string | null): Promise<AssetRow> {
  return invoke<AssetRow>("move_asset", { assetId, newParentId });
}

/** Delete a leaf. An asset that still holds something rejects with `conflict`. */
export function deleteAsset(assetId: string): Promise<void> {
  return invoke<void>("delete_asset", { assetId });
}
