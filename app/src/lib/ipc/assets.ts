/**
 * The estate — `crates/knobas-app/src/commands/assets.rs`.
 *
 * The types are generated from the Rust (`./generated/assets.ts`, contract
 * §2.6 as amended by #1325, switched by #1328): a field is explained in its
 * Rust doc comment and nowhere else. The `invoke` functions stay hand-written,
 * pinned to the Rust by tests in that module which `include_str!` this file,
 * and so does {@link ImportProducer}, the type of a list this file declares
 * rather than of anything that crosses the bridge.
 *
 * A §10.8-ratified module pair on the `time` module's precedent (issue #428).
 * **Every** asset command belongs here — #432's routes, #439's import and
 * M4.1's alert reads as well as #428's original seven — so the bridge grows
 * one module rather than one more section of `entity.ts` per ticket. #429's
 * `assetTypes` was the first to arrive that way.
 *
 * There is no asset *event*. Every mutation writes a line to the activity
 * stream and the shell learns from the signal it already watches
 * (`EVENTS.activityNew`, read by `shell/latest-change.svelte.ts`). A channel
 * of the estate's own would be a second thing to keep in step with the first.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AssetDetail,
  AssetEdit,
  AssetRow,
  AssetType,
  DepLayer,
  DepView,
  DepViewSettings,
  DepViews,
  Dependencies,
  DependsOnThis,
  ImportOutcome,
  ImportPreview,
  Landing,
  MonitorRow,
  MonitoringSettings,
  OpenAlert,
  Produced,
  PropertyValue,
  RouteDetail,
  RouteEdit,
  RouteRow,
  UnmonitoredAsset,
  Visibility,
} from "./generated/assets";

export type {
  AlertAsset,
  AlertState,
  AssetChange,
  AssetDetail,
  AssetEdit,
  AssetProperty,
  AssetRow,
  AssetStatus,
  AssetType,
  AttachedMonitor,
  DepGroup,
  DepItem,
  DepKind,
  DepLayer,
  DepRelation,
  DepTiles,
  DepView,
  DepViewSettings,
  DepViews,
  Dependencies,
  DependsOnRow,
  DependsOnThis,
  Environment,
  ImportEntry,
  ImportOutcome,
  ImportPreview,
  Inherited,
  Landing,
  LayerInView,
  MonitorLink,
  MonitorRow,
  MonitorSample,
  MonitorTarget,
  MonitoredAsset,
  MonitoringSettings,
  OpenAlert,
  Produced,
  PropertyChange,
  PropertyKind,
  PropertyPlan,
  PropertyValue,
  RouteDetail,
  RouteEdit,
  RouteRow,
  TypedProperty,
  UnmonitoredAsset,
  UnresolvedMonitor,
  UptimeRatio,
  Visibility,
} from "./generated/assets";

// One name per arm of the three tagged unions, for a caller that holds one
// arm. Picked out of the generated union, so an arm has one declaration.

/** {@link PropertyValue}'s `text` arm. */
export type TextProperty = Extract<PropertyValue, { kind: "text" }>;
/** {@link PropertyValue}'s `number` arm. */
export type NumberProperty = Extract<PropertyValue, { kind: "number" }>;
/** {@link PropertyValue}'s `date` arm. */
export type DateProperty = Extract<PropertyValue, { kind: "date" }>;
/** {@link PropertyValue}'s `url` arm. */
export type UrlProperty = Extract<PropertyValue, { kind: "url" }>;

/** {@link AssetEdit}'s `name` arm. */
export type NameEdit = Extract<AssetEdit, { field: "name" }>;
/** {@link AssetEdit}'s `status` arm. */
export type StatusEdit = Extract<AssetEdit, { field: "status" }>;
/** {@link AssetEdit}'s `environment` arm. */
export type EnvironmentEdit = Extract<AssetEdit, { field: "environment" }>;
/** {@link AssetEdit}'s `owner` arm. */
export type OwnerEdit = Extract<AssetEdit, { field: "owner" }>;
/** {@link AssetEdit}'s `property` arm. */
export type PropertyEdit = Extract<AssetEdit, { field: "property" }>;
/** {@link AssetEdit}'s `monitors` arm. */
export type MonitorsEdit = Extract<AssetEdit, { field: "monitors" }>;

/** {@link RouteEdit}'s `name` arm. */
export type RouteNameEdit = Extract<RouteEdit, { field: "name" }>;
/** {@link RouteEdit}'s `url` arm. */
export type RouteUrlEdit = Extract<RouteEdit, { field: "url" }>;
/** {@link RouteEdit}'s `target` arm. */
export type RouteTargetEdit = Extract<RouteEdit, { field: "target" }>;
/** {@link RouteEdit}'s `visibility` arm. */
export type RouteVisibilityEdit = Extract<RouteEdit, { field: "visibility" }>;
/** {@link RouteEdit}'s `property` arm. */
export type RoutePropertyEdit = Extract<RouteEdit, { field: "property" }>;
/** {@link RouteEdit}'s `monitors` arm. */
export type RouteMonitorsEdit = Extract<RouteEdit, { field: "monitors" }>;

/** {@link Produced}'s `token_needed` arm. */
export type TokenNeeded = Extract<Produced, { state: "token_needed" }>;
/** {@link Produced}'s `landing_needed` arm. */
export type LandingNeeded = Extract<Produced, { state: "landing_needed" }>;
/** {@link Produced}'s `ready` arm. */
export type ProducedFile = Extract<Produced, { state: "ready" }>;

/**
 * The built-in type table: what a type is called, its monogram, the properties
 * it declares and the children usually held inside one.
 *
 * A constant on the backend, so the answer never changes within a build and a
 * caller may read it once. It needs no database, which is why it is the one
 * command here that answers before bring-up.
 */
export function assetTypes(): Promise<AssetType[]> {
  return invoke<AssetType[]>("asset_types");
}

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
 * What breaks if this asset goes down (#505): its descendants and everything
 * linked to it by `depends-on` or `runs-on`, transitively over both, plus the
 * routes that land on it or on any of those (#1553) listed apart.
 *
 * A read of its own and not part of {@link getAsset}: the pane's read runs on
 * every selection in every column, and this walk costs a round trip per step
 * of the closure.
 *
 * An id no asset carries is a rejection, not an empty answer.
 */
export function dependsOnThis(assetId: string): Promise<DependsOnThis> {
  return invoke<DependsOnThis>("depends_on_this", { assetId });
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

/** One route with its own history — what `#/route/<id>` opens on. */
export function getRoute(routeId: string): Promise<RouteDetail> {
  return invoke<RouteDetail>("get_route", { routeId });
}

/**
 * Expose a route on an asset, with or without a target.
 *
 * `visibility` omitted is `"internal"`, which is what an unclassified route
 * reads as. The id is minted by the backend, like an asset's.
 */
export function createRoute(
  assetId: string,
  name: string,
  url: string,
  targetId?: string | null,
  visibility?: Visibility,
  properties?: [string, PropertyValue][],
): Promise<RouteRow> {
  return invoke<RouteRow>("create_route", {
    assetId,
    name,
    url,
    targetId: targetId ?? null,
    visibility: visibility ?? null,
    properties: properties ?? null,
  });
}

/** Apply a list of edits to a route, each one a history line. */
export function editRoute(routeId: string, edits: RouteEdit[]): Promise<RouteRow> {
  return invoke<RouteRow>("edit_route", { routeId, edits });
}

/** Delete a route. Nothing hangs off one, so nothing refuses. */
export function deleteRoute(routeId: string): Promise<void> {
  return invoke<void>("delete_route", { routeId });
}

/**
 * One entry of the Import dialog's chooser (#508): the part of `assets::Producer`
 * the chooser draws. Hand-written, because nothing sends it over the bridge:
 * it types {@link IMPORT_PRODUCERS}, a list this file declares.
 */
export interface ImportProducer {
  /** What the two Import commands are sent as their `producer`. */
  id: string;
  /** What the chooser calls it. */
  label: string;
  /**
   * It reads a live system, so its text comes from {@link produceEstateFile}
   * rather than from a file the reader picked off the disk (#509).
   *
   * `CONTEXT.md`, *Importer*: **producer** is the wider word, and the estate
   * file is the one producer that is *not* an importer, because there is no
   * live system on the other end of it. That difference is the whole of what
   * the dialog branches on — an `<input type="file">` on one side, a token and
   * a produce command on the other.
   *
   * **Stated, not derived from the id.** The backend's own rule is a field on
   * `assets::Producer` and its doc says why: *"not `origin_key.is_empty()` …
   * a check that read the key would be measuring a representation of the thing
   * rather than the thing"*. A frontend that read `id !== "estate_file"` would
   * be making exactly that mistake one surface over, and it would draw a *Read*
   * button on any future producer that reads no live system.
   * `commands::assets`' `the_chooser_offers_producers_this_build_knows` holds
   * this flag to `Producer::importer` for every entry.
   */
  importer: boolean;
}

/**
 * The chooser's entries: where an estate file can come from (#508).
 *
 * **Three entries** since #510: the file a person picks off the disk, the
 * hcloud importer, and the Docker host importer. Each is a producer the backend
 * has to know an origin key for. The ids here are the authority on what the
 * chooser may send, and `commands::assets`'
 * `the_chooser_offers_producers_this_build_knows` reads this list and checks
 * every id against `assets::PRODUCERS`. The reverse is deliberately not
 * checked: the backend may declare a producer's origin key before anything can
 * produce that producer's file, which is what #508 did for hcloud.
 *
 * A **non-empty tuple** rather than an array, so the dialog can open on the
 * first entry without a fallback for a chooser with nothing in it — a state
 * this list is never in and no code should be written for.
 */
export const IMPORT_PRODUCERS: readonly [ImportProducer, ...ImportProducer[]] = [
  { id: "estate_file", label: "Estate file", importer: false },
  { id: "hcloud", label: "Hetzner Cloud", importer: true },
  { id: "docker", label: "Docker host", importer: true },
];

/**
 * What importing this estate file would do, having written nothing (#439).
 *
 * The **text** of the file, not a path: the dialog reads it with the browser's
 * own `File` API, so the Import needs no Tauri dialog plugin and no filesystem
 * capability. The parse is the backend's, so a file that is not an estate file
 * is refused once, in one voice.
 *
 * `producer` is which of {@link IMPORT_PRODUCERS} the reader chose (#508). It
 * is not in the file — a producer returns an estate file in the checked-in
 * shape — and what the backend takes from it is the origin key it declares,
 * the matching rule used when the file's id is not one the tree holds.
 *
 * Rejects with `invalid` for a producer the backend does not know, or for a
 * file that is not JSON, carries a key the format does not define, names a type
 * nobody declares, names a parent or a target that is nowhere, whose assets
 * hold each other, or whose origin key matches two assets at once.
 */
export function previewEstateImport(file: string, producer: string): Promise<ImportPreview> {
  return invoke<ImportPreview>("preview_estate_import", { file, producer });
}

/**
 * Run one importer against its live system (#509).
 *
 * `token` is sent only when the reader has just typed one; otherwise the
 * backend reads the keychain, under the `importer:` namespace an importer's
 * credential lives in (ADR-0015 — an importer is not a source, and its token is
 * not reachable by anything that walks sources). `landUnder` is where the
 * things this estate does not hold will land ({@link Landing}), and is owed
 * only once `landing_needed` has said so — `null` until then, and *never* as a
 * way of saying *the top*, which is `{ parent: null }`.
 *
 * **The Docker importer (#510) needs neither.** It spawns the docker CLI under
 * each engine asset's `docker_context`, so there is no credential, and a
 * container lands under the engine whose context found it, so there is no
 * landing to choose. A token sent to it is refused rather than stored.
 *
 * Rejects with `invalid` for a producer the backend does not know, for the
 * estate file (which is chosen from the disk and produces nothing) or for a
 * token sent to a producer that takes none; with `unauthorized` for a token the
 * live system refused; and with `unreachable` for a docker context this machine
 * cannot read.
 */
export function produceEstateFile(
  producer: string,
  token: string | null,
  landUnder: Landing | null,
): Promise<Produced> {
  return invoke<Produced>("produce_estate_file", { producer, token, landUnder });
}

/**
 * Apply the import {@link previewEstateImport} previewed.
 *
 * The file is sent again rather than the preview being sent back: the plan is
 * recomputed inside the write's own transaction, which is what makes what is
 * written and what was shown two runs of one rule.
 */
export function applyEstateImport(file: string, producer: string): Promise<ImportOutcome> {
  return invoke<ImportOutcome>("apply_estate_import", { file, producer });
}

/** What monitoring is set to; the ratified defaults where nothing is stored. */
export function monitoringSettings(): Promise<MonitoringSettings> {
  return invoke<MonitoringSettings>("monitoring_settings");
}

/**
 * Change both, and get back what is now stored.
 *
 * Nothing already sampled is rewritten: *warn* is derived at sample time, so a
 * new threshold decides the next poll and leaves the hours already drawn on
 * the Monitors tab as they were recorded.
 */
export function setMonitoringSettings(
  settings: MonitoringSettings,
): Promise<MonitoringSettings> {
  return invoke<MonitoringSettings>("set_monitoring_settings", { settings });
}

/**
 * The Monitors tab's roster: every monitor of a source filling the
 * monitoring role, tombstones included. The read narrows itself to the
 * role's fillers by source (#1297, §10.8's #1294 entry).
 *
 * One read for the whole tab. The chip counts are counts of these rows, so a
 * second command answering counts would be a second answer that could
 * disagree with the list beside it.
 */
export function monitorRoster(): Promise<MonitorRow[]> {
  return invoke<MonitorRow[]>("monitor_roster");
}

/**
 * Every open alert in the estate, newest first.
 *
 * **One read for both surfaces.** The top strip's count is this list's length
 * and the Assets view's list is this list, so the badge cannot disagree with
 * what is under it.
 */
export function openAlerts(): Promise<OpenAlert[]> {
  return invoke<OpenAlert[]>("open_alerts");
}

/**
 * Ack the open alert of one monitor — seen, not fixed (#446).
 *
 * Clears the reader's inbox item and **leaves the alert open**: only a return
 * to `up` closes one, so the estate goes on saying this thing is down while
 * the inbox stops saying it needs somebody. A history line lands on every
 * asset the monitor watches.
 *
 * **By the monitor and not the alert row's id**: the inbox item's subject is
 * the monitor entity, and one monitor has at most one open alert. Rejects with
 * `not_found` when it has none — which is what acking a row that recovered
 * while the reader was looking at it gets.
 */
export function ackAlert(monitorId: string): Promise<OpenAlert> {
  return invoke<OpenAlert>("ack_alert", { monitorId });
}

/**
 * Every asset with no confirmed `monitored-by` link to a monitor.
 *
 * **A statement about attachment, not about attention.** An asset whose only
 * monitor Kuma has paused is not here: somebody wired a check to it and then
 * silenced it, which the roster above says with the Paused chip.
 *
 * Unfiltered, for the reason `monitorRoster` is: the type filter's options are
 * the types this answer holds, so the tab cannot draw them without the list.
 */
export function unmonitoredAssets(): Promise<UnmonitoredAsset[]> {
  return invoke<UnmonitoredAsset[]>("unmonitored_assets");
}

// -- Dependencies (#871) ------------------------------------------------------

/**
 * The top-bar opening of Dependencies (R-11.10, R-11.23): the shared layers,
 * every kind, and every asset, route, repository, pipeline, open pull request
 * and linked ticket with its health and the known relations. One read; the
 * bands, tiles and lines are drawn from it in the UI.
 */
export function dependencies(): Promise<Dependencies> {
  return invoke<Dependencies>("dependencies");
}

// -- The layer writes and the saved views of Dependencies (#873) -------------

/** Add a shared layer at the bottom; answers with every layer. `conflict` for a taken name. */
export function createDepLayer(name: string): Promise<DepLayer[]> {
  return invoke<DepLayer[]>("create_dep_layer", { name });
}

/** Rename a shared layer; `conflict` for a name another layer has, `invalid` for a blank one. */
export function renameDepLayer(layerId: string, name: string): Promise<DepLayer[]> {
  return invoke<DepLayer[]>("rename_dep_layer", { layerId, name });
}

/** Remove a shared layer from every view; its kinds go to no layer. */
export function deleteDepLayer(layerId: string): Promise<DepLayer[]> {
  return invoke<DepLayer[]>("delete_dep_layer", { layerId });
}

/** Put the shared layers in this order, top to bottom: every layer once. */
export function orderDepLayers(layerIds: string[]): Promise<DepLayer[]> {
  return invoke<DepLayer[]>("order_dep_layers", { layerIds });
}

/** Put a kind at the end of a layer, or in no layer when `layerId` is `null`. */
export function placeDepKind(kind: string, layerId: string | null): Promise<DepLayer[]> {
  return invoke<DepLayer[]>("place_dep_kind", { kind, layerId });
}

/** Every saved view of Dependencies and which opens from a ticket and from the top bar. */
export function depViews(): Promise<DepViews> {
  return invoke<DepViews>("dep_views");
}

/** Save a view: new when `viewId` is `null` (*Save as a new view…*), else its name and settings. */
export function saveDepView(viewId: string | null, name: string, settings: DepViewSettings): Promise<DepView> {
  return invoke<DepView>("save_dep_view", { viewId, name, settings });
}

/** Delete a view; `conflict` for the last one. */
export function deleteDepView(viewId: string): Promise<DepViews> {
  return invoke<DepViews>("delete_dep_view", { viewId });
}

/** Choose which view opens from a ticket's page and which from the top bar. */
export function setDepViewDefaults(fromTicket: string, fromTop: string): Promise<DepViews> {
  return invoke<DepViews>("set_dep_view_defaults", { fromTicket, fromTop });
}
