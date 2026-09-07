/**
 * The estate — `crates/knobas-app/src/commands/assets.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a field, a union member or a command name added on
 * one side only fails `cargo test`, not merely `svelte-check`.
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

import type { ActivityRow, LinkEntry } from "./entity";

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
 * What a person recorded, and not the same thing as `AssetRow.health`, which
 * is the worst of this and everything underneath. `"none"` means *nobody has
 * said*, which is a different thing from `"up"`. Its monitors' states join the
 * health in M4.1.
 */
export type AssetStatus = "up" | "warn" | "down" | "none";

/**
 * Which environment an asset belongs to — `assets::Environment`.
 *
 * Stored on the asset, and `null` here means *not set here* — what is in force
 * is `AssetDetail.effective_environment`, the nearest asset at or above that
 * sets one.
 */
export type Environment = "dev" | "stage" | "prod" | "shared";

/**
 * A value **in force** on an asset, and the asset that sets it —
 * `assets::Inherited`.
 *
 * Stories 8, 9 and 10 in one shape: what the value is, and where to go to
 * change it. `source_id` equal to the asset's own id is *set here*; anything
 * else is inherited from that ancestor, and the pane links to it.
 */
export interface Inherited<T> {
  value: T;
  /** The asset the value is set on — the asset itself when it is set here. */
  source_id: string;
  source_name: string;
}

/**
 * One property a type declares — `knobas_core::asset::TypedProperty`.
 *
 * The `kind` is what a pane's editor needs and what an {@link AssetProperty}
 * cannot carry: a declared key nobody has filled in arrives with `value: null`,
 * and there is no kind in a `null`.
 */
export interface TypedProperty {
  /** The key it is stored under in the properties bag. */
  key: string;
  /** What the pane calls it. */
  label: string;
  kind: PropertyKind;
}

/**
 * One built-in asset type — `knobas_core::asset::AssetType`.
 *
 * Read off the backend rather than declared here, so the nineteen types are
 * one list. The **ids are wire values and never change**; the labels are
 * display and may be reworded.
 */
export interface AssetType {
  id: string;
  label: string;
  /** Two characters — the chip a column row draws. */
  monogram: string;
  /** In the order the pane shows them. `custom` declares none. */
  properties: TypedProperty[];
  /**
   * The child types conventionally suggested under one of these, by id —
   * story 17's *usual here*.
   *
   * A suggestion and not a constraint: the create dialog offers all nineteen
   * whatever is in here, and an empty list is a real answer.
   */
  suggests: string[];
}

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
  /**
   * The worst of {@link status} and every descendant's own status, down over
   * warn over up over none (story 37). Monitors join the "own" half in M4.1.
   */
  health: AssetStatus;
  /**
   * The worst status **strictly underneath**; `"none"` when nothing under it
   * has been rated. What colours the badge — a `down` asset holding one `warn`
   * container has `health: "down"` and `inside: "warn"`, and the badge is
   * about what is inside.
   */
  inside: AssetStatus;
  /** How many descendants carry `warn` or `down` — the *N* in the badge. */
  problems_inside: number;
  /**
   * How many **work items** this asset is linked to — story 32's other badge
   * (#435).
   *
   * A work item is an entity the mirror holds: a ticket, a build, a page, a
   * commit. Contexts, notes and other assets are knobas' own and are not
   * counted, which is what makes the badge *linked work* rather than a count
   * of links. Confirmed links only, either direction, one per link.
   */
  linked_work: number;
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

/**
 * One row of a room's Assets tile — `assets::MemberAsset` (#434).
 *
 * The asset plus **where it sits**, and a carrier rather than a `path` field
 * on {@link AssetRow}: a Miller column draws no path, so the column reads
 * leave `path_text` unselected and this one tile does not put it on every
 * row of every walk.
 */
export interface MemberAsset {
  asset: AssetRow;
  /** The ancestors' names, outermost first, `" / "` between; `null` at the
   * top of the estate. */
  path: string | null;
}

/**
 * Who can reach a route — `assets::Visibility`.
 *
 * Two values: *internal* is reachable from inside the estate, *public* from
 * outside it. Every finer shade — which VPN, which network — is a property
 * with a name of its own. `"internal"` is what an unclassified route reads as.
 */
export type Visibility = "internal" | "public";

/**
 * One route — `assets::RouteRow`.
 *
 * The same shape at both ends: in {@link AssetDetail.exposes} it is what this
 * asset exposes, and in {@link AssetDetail.reachable_via} it is a route whose
 * target is somewhere on this asset's containment path. **Where it lands** is
 * `target_id`: equal to the asset's own id means it lands here, and anything
 * else is named by `target_name` — an ancestor when `held_by` holds it, and
 * something inside otherwise. That comparison is what story 31's dashed wire
 * is drawn from.
 */
export interface RouteRow {
  /** `route:<uuid>` — and the `#/route/<id>` address. */
  id: string;
  /** The asset that exposes it. Never null. */
  asset_id: string;
  asset_name: string;
  /** The asset it lands on; `null` for an endpoint that lands on nothing. */
  target_id: string | null;
  target_name: string | null;
  name: string;
  /** The URL or endpoint, carrying its scheme. */
  url: string;
  /** Who can reach it. `"internal"` is what an unclassified route reads as. */
  visibility: Visibility;
  /** A route has no type, so every property is the reader's own. */
  properties: AssetProperty[];
}

/**
 * Everything the pane draws for one route — `assets::RouteDetail`.
 *
 * No held-by path of its own: a route sits on the asset that exposes it, and
 * `route.asset_id` is where the Tree opens.
 */
export interface RouteDetail {
  route: RouteRow;
  /** This route's own activity lines, newest first. */
  history: ActivityRow[];
}

/** Everything the fixed right pane draws — `assets::AssetDetail`. */
export interface AssetDetail {
  asset: AssetRow;
  properties: AssetProperty[];
  /** The environment in force and the asset that sets it; `null` when nothing
   * at or above this asset sets one. */
  effective_environment: Inherited<Environment> | null;
  /** The owner in force and the asset that sets it. */
  effective_owner: Inherited<string> | null;
  /** Outermost first, **excluding the asset itself**. Empty at the top. */
  held_by: AssetRow[];
  holds: AssetRow[];
  /** The routes this asset exposes, by name (#432). */
  exposes: RouteRow[];
  /**
   * The routes whose target is on this asset's containment path — landing on
   * it, on something that holds it, or on something it holds — nearest first
   * (#432). See {@link RouteRow} for which is which.
   */
  reachable_via: RouteRow[];
  /** This asset's own activity lines, newest first. */
  history: ActivityRow[];
  /**
   * Every confirmed link this asset takes part in, either end, newest first
   * (#435).
   *
   * The same shape a ticket's detail carries, drawn by the same panel: linking
   * an asset is the same gesture as linking a ticket (story 36), so the pane
   * has no link list of its own.
   */
  links: LinkEntry[];
  /**
   * The Uptime Kuma names of the monitors watching this asset, as an import
   * kept them (#439).
   *
   * **Names, not monitors.** A name becomes a `monitored-by` link the moment
   * the mirror holds a monitor called that. So this is what the estate file
   * said, and {@link monitoring} is what has been resolved out of it — a name
   * in both has arrived, a name in only this one is still waiting.
   */
  monitors: string[];
  /**
   * The monitors watching this asset, by name — the pane's *monitoring*
   * section (#445).
   */
  monitoring: AttachedMonitor[];
}

/**
 * One monitor watching an asset — `assets::AttachedMonitor`.
 *
 * The attachment is a `monitored-by` link; the state and the address are the
 * mirror's, and both are null when the mirror no longer holds the monitor —
 * which is what a **paused** monitor is, since Kuma drops it from `/metrics`
 * and the adapter tombstones it.
 */
export interface AttachedMonitor {
  /** `<source>:<monitor id in Kuma>` — the address its detail opens at. */
  entity_id: string;
  /** Its name, which is the name an estate file uses to ask for it. */
  name: string;
  /** `up`, `down`, `pending`, `maintenance`, or null when there is no reading. */
  state: string | null;
  /** Its own page in Uptime Kuma (story 71); null when there is none left. */
  web_url: string | null;
  /**
   * Whether it has left the mirror — paused in Kuma, or deleted.
   *
   * *Tombstoned* is `CONTEXT.md`'s word for it; the *Linked* panel under the
   * pane's monitoring section renders the same fact — `LinkEnd.deleted_at` —
   * with the older spelling *withdrawn*.
   */
  tombstoned: boolean;
}

/**
 * One entry of an import preview's *already in the tree* or *new* group —
 * `assets::ImportEntry`.
 *
 * Assets and routes in one shape, because the question a group answers is
 * about the **file**: what of it does knobas already hold. `kind` is which.
 */
export interface ImportEntry {
  /** The file's own id, which is the id the asset or route carries. */
  id: string;
  /** `"asset"` or `"route"` — the word `knobas.entity.kind` holds. */
  kind: string;
  name: string;
  /** The type's label for an asset; `null` for a route, which has no type. */
  type_label: string | null;
  /** The parent for an asset, the exposing asset for a route. */
  parent_id: string | null;
}

/**
 * What an apply would do with one property the file names —
 * `assets::PropertyPlan`.
 *
 * `"kept"` is the reader's own value surviving: a property edited by hand is
 * never overwritten by a later import of the same file (story 24).
 */
export type PropertyPlan = "set" | "kept";

/**
 * One property whose file value differs from the stored one —
 * `assets::PropertyChange`.
 *
 * Only the ones that differ, and both fates in one list: *the file says `cx23`
 * and it stays `cx33` because you typed that* is the sentence the preview
 * exists to say.
 */
export interface PropertyChange {
  key: string;
  /** The type's label for a declared key, the key itself for a custom one. */
  label: string;
  /** What the asset carries now; `null` for a key it does not carry yet. */
  from: PropertyValue | null;
  /** What the file says. */
  to: PropertyValue;
  plan: PropertyPlan;
}

/**
 * One asset already in the tree that an apply would change —
 * `assets::AssetChange`.
 *
 * An asset with nothing to change is in {@link ImportPreview.known} instead,
 * so the length of this list is the honest answer to *what would this do*.
 */
export interface AssetChange {
  id: string;
  /** The name **in the tree**: an import never renames. */
  name: string;
  properties: PropertyChange[];
  /** Monitor names the file lists that this asset does not carry yet. */
  monitors: string[];
}

/**
 * One `monitored-by` link an apply would draw — `assets::MonitorLink`.
 *
 * Only the names the mirror actually holds. A name it does not hold is kept on
 * the asset and reported as an {@link UnresolvedMonitor} instead.
 */
export interface MonitorLink {
  asset_id: string;
  asset_name: string;
  /** The Uptime Kuma name, as the file spells it and the mirror holds it. */
  monitor_name: string;
  /** The mirrored monitor's entity id — the other end of the link. */
  monitor_id: string;
}

/**
 * One monitor name on one asset that answers to nothing in the mirror —
 * `assets::UnresolvedMonitor`.
 *
 * {@link MonitorLink}'s negative. Reported on **every** preview and not only
 * on the one that first kept the name: {@link AssetChange.monitors} lists what
 * an apply would write, so a name already on the asset is absent from it, and
 * a second preview of an unchanged file would otherwise say nothing at all
 * about six names that still find no monitor.
 */
export interface UnresolvedMonitor {
  asset_id: string;
  asset_name: string;
  /** The Uptime Kuma name, as the file spells it. */
  monitor_name: string;
}

/**
 * What an import would do, before it has done any of it —
 * `assets::ImportPreview`.
 *
 * The three groups the Import dialog draws, plus the monitor links, which are
 * a write and therefore have to be announced by the same read, and the names
 * that found no monitor.
 */
export interface ImportPreview {
  /** What the file calls the estate. */
  name: string;
  /** Entries whose id knobas already holds. */
  known: ImportEntry[];
  /** Entries an apply would create, in the order it would create them. */
  new: ImportEntry[];
  /** The known assets with something to change, and what. */
  changes: AssetChange[];
  monitor_links: MonitorLink[];
  /** The names the mirror does not hold, by asset and then by name. */
  unresolved: UnresolvedMonitor[];
}

/** What an import did — `assets::ImportOutcome`. */
export interface ImportOutcome {
  assets_created: number;
  routes_created: number;
  /** Properties the file's value was written to. */
  properties_set: number;
  /** Properties left as they were because a hand edit claimed them. */
  properties_kept: number;
  /** Monitor names newly kept on an asset. */
  monitors_kept: number;
  /** `monitored-by` links drawn. */
  monitors_linked: number;
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

/** Rename a route — `assets::RouteEdit::Name`. */
export interface RouteNameEdit {
  field: "name";
  value: string;
}

/** Set the URL or endpoint — `assets::RouteEdit::Url`. */
export interface RouteUrlEdit {
  field: "url";
  /** Carries a scheme; a bare host is refused. */
  value: string;
}

/** Set or clear the asset a route lands on — `assets::RouteEdit::Target`. */
export interface RouteTargetEdit {
  field: "target";
  /** `null` makes it an endpoint that lands on nothing knobas knows. */
  value: string | null;
}

/** Set who can reach it — `assets::RouteEdit::Visibility`. */
export interface RouteVisibilityEdit {
  field: "visibility";
  value: Visibility;
}

/** Set or clear one of the route's own properties — `assets::RouteEdit::Property`. */
export interface RoutePropertyEdit {
  field: "property";
  key: string;
  /** `null` removes the key. */
  value: PropertyValue | null;
}

/**
 * One field of a route changing — `assets::RouteEdit`.
 *
 * {@link AssetEdit}'s shape for its reason. The asset that **exposes** a route
 * is not among them: a route is the address of the thing that answers it, so
 * re-exposing one elsewhere is a different route with a different history.
 */
export type RouteEdit =
  | RouteNameEdit
  | RouteUrlEdit
  | RouteTargetEdit
  | RouteVisibilityEdit
  | RoutePropertyEdit;

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
 * The member assets of a stored context — the room's Assets tile (#434).
 *
 * Worst health first, then by name. Membership is the one rule
 * `contextMembers` answers with (ADR-0008), reaching assets **through their
 * ancestors**: a VM in the context brings what it holds, so a list of one VM
 * and its four containers is five rows.
 *
 * A context that does not exist answers with an empty list, not a rejection.
 */
export function contextAssets(ctxId: string): Promise<MemberAsset[]> {
  return invoke<MemberAsset[]>("context_assets", { ctxId });
}

/**
 * The assets a **source** room's Assets tile draws (#435): the ones this
 * source's monitors are attached to, by a `monitored-by` link, worst health
 * first.
 *
 * **Empty until M4.1.** No adapter emits a `monitor` yet, so the backend's
 * statement matches nothing — which is the read answering honestly rather than
 * a stub, and the tile fills itself the day the Kuma adapter lands.
 *
 * A source nothing was synced under answers with an empty list, not a
 * rejection.
 */
export function sourceAssets(sourceId: string): Promise<MemberAsset[]> {
  return invoke<MemberAsset[]>("source_assets", { sourceId });
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
 * What importing this estate file would do, having written nothing (#439).
 *
 * The **text** of the file, not a path: the dialog reads it with the browser's
 * own `File` API, so the Import needs no Tauri dialog plugin and no filesystem
 * capability. The parse is the backend's, so a file that is not an estate file
 * is refused once, in one voice.
 *
 * Rejects with `invalid` for a file that is not JSON, carries a key the format
 * does not define, names a type nobody declares, names a parent or a target
 * that is nowhere, or whose assets hold each other.
 */
export function previewEstateImport(file: string): Promise<ImportPreview> {
  return invoke<ImportPreview>("preview_estate_import", { file });
}

/**
 * Apply the import {@link previewEstateImport} previewed.
 *
 * The file is sent again rather than the preview being sent back: the plan is
 * recomputed inside the write's own transaction, which is what makes what is
 * written and what was shown two runs of one rule.
 */
export function applyEstateImport(file: string): Promise<ImportOutcome> {
  return invoke<ImportOutcome>("apply_estate_import", { file });
}

/**
 * The two numbers monitoring is shaped by — `commands::assets::MonitoringSettings`.
 *
 * Both are clamped by the backend on the way in and on the way out, so what
 * this posts and what it gets back can differ, and the section draws the
 * answer.
 */
export interface MonitoringSettings {
  /** How many days of samples knobas keeps. Default 90, floor 1. */
  sample_retention_days: number;
  /**
   * The response time in milliseconds above which an otherwise-up monitor
   * samples as *warn*. Default 1500.
   */
  response_time_warn_ms: number;
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
 * One row of the Monitors tab's roster — `assets::MonitorRow` (#448).
 *
 * The whole tab in one shape. The three groups of fields are worth telling
 * apart: the identity and the address are the mirror's; `monitor_type`,
 * `target`, `response_time_ms`, `uptime` and `cert_days_remaining` are read
 * out of the mirrored payload; and `state` and `samples` are knobas' own
 * timeseries — *warn* exists nowhere in Uptime Kuma and is derived at sample
 * time, so a chip counting warns counts samples and not the mirror.
 */
export interface MonitorRow {
  /** `<source>:<monitor id in Kuma>` — the entity id, and its detail address. */
  entity_id: string;
  /** The source that mirrored it. */
  source_id: string;
  /** Its name in Uptime Kuma — the name an estate file uses to ask for it. */
  name: string;
  /**
   * The state knobas last recorded: the newest sample in the window, and the
   * mirror's own reading where the window holds none.
   *
   * `up`, `warn`, `down`, `pending`, `maintenance`, or `null` for a genuine
   * miss. This is the bar's right-hand end, which is what makes the chip and
   * the bar one statement.
   */
  state: string | null;
  /** `http`, `ping`, `docker`, `port` — Kuma's word for the kind of monitor. */
  monitor_type: string | null;
  /** What it watches: the URL, else the hostname with its port. */
  target: string | null;
  /** Kuma's last reading in milliseconds; `null` for a poll that did not answer. */
  response_time_ms: number | null;
  /**
   * When knobas last read this monitor — RFC 3339.
   *
   * `/metrics` carries no clock at all, so there is no "last checked" to
   * mirror and this is when knobas looked.
   */
  checked_at: string;
  /** Kuma's sliding-window uptime ratios, by its own labels, in label order. */
  uptime: UptimeRatio[];
  /** Days until the watched certificate expires; `null` for a monitor with none. */
  cert_days_remaining: number | null;
  /** Its own page in Uptime Kuma; `null` when there is none left. */
  web_url: string | null;
  /**
   * Whether it has left the mirror — paused in Kuma, or deleted.
   *
   * *Tombstoned* is the word `CONTEXT.md` gives the state and
   * {@link AttachedMonitor} uses; *Paused* is what the chip says, because
   * pausing is what a reader did to make it true.
   */
  tombstoned: boolean;
  /** The assets it is attached to by a `monitored-by` link, by name. */
  assets: MonitoredAsset[];
  /**
   * The write ops the tab may offer on this row — `knobas_source::WriteOp`
   * identifiers, already filtered by the backend (issue #452).
   *
   * The inbox's `actions` exactly: the surface renders what it is handed and
   * decides nothing. Two filters have already run — what the source declares,
   * which is empty for an Uptime Kuma configured with only an API key, and
   * what this row's state has a use for: `pause_monitor` while it is live,
   * `resume_monitor` once it is {@link tombstoned}.
   */
  actions: string[];
  /** Its samples inside the bar's 24-hour window, oldest first. */
  samples: MonitorSample[];
}

/** One uptime ratio at the window Kuma computed it over — `assets::UptimeRatio`. */
export interface UptimeRatio {
  /** Kuma's own label: `1d`, `30d`, `365d`. */
  window: string;
  /** `0` to `1`, as Kuma publishes it — not a percentage. */
  ratio: number;
}

/** One asset a monitor watches — `assets::MonitoredAsset`. */
export interface MonitoredAsset {
  /** `asset:<uuid>` — and the `#/asset/<id>` address the name links to. */
  id: string;
  name: string;
  /** The ancestors' names, outermost first; `null` at the top of the estate. */
  path: string | null;
}

/** One sample as the bar draws it — `assets::MonitorSample`. */
export interface MonitorSample {
  /** RFC 3339. */
  taken_at: string;
  /** `up`, `warn`, `down`, `pending`, `maintenance`, or `null` — a gap. */
  state: string | null;
}

/**
 * The Monitors tab's roster: every mirrored monitor, tombstones included.
 *
 * One read for the whole tab. The chip counts are counts of these rows, so a
 * second command answering counts would be a second answer that could
 * disagree with the list beside it.
 */
export function monitorRoster(): Promise<MonitorRow[]> {
  return invoke<MonitorRow[]>("monitor_roster");
}

/**
 * What trouble an open alert is about — `assets::AlertState`.
 *
 * Two words and not four: an alert exists only for `down` and `warn`, so
 * `AssetStatus`' `up` and `none` have no meaning here. The spellings are
 * shared with a status on purpose, so a reader can compare an alert with the
 * health it caused.
 */
export type AlertState = "down" | "warn";

/**
 * One asset an alert's monitor watches — `assets::AlertAsset`.
 *
 * A list on the alert and not a field, because `monitored-by` is an ordinary
 * link and two assets may name one monitor. Empty is a real answer: a monitor
 * attached to nothing still opens an alert.
 */
export interface AlertAsset {
  /** `asset:<uuid>` — what the row opens. */
  id: string;
  name: string;
  /** Its ancestors, outermost first, or `null` at the top of the estate. */
  path: string | null;
}

/**
 * One open alert — `assets::OpenAlert` (#444).
 *
 * **No address of its own.** The `id` is the row's number, what the alert is
 * about is the monitor, and what a reader opens is the asset — spec #427
 * story 61.
 */
export interface OpenAlert {
  id: number;
  /** The monitor's entity id. */
  monitor_id: string;
  monitor_name: string;
  state: AlertState;
  opened_at: string;
  /** When somebody said they had seen it. Written by #446's ack. */
  acked_at: string | null;
  assets: AlertAsset[];
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
 * One row of the Monitors tab's *Not monitored* roster —
 * `assets::UnmonitoredAsset` (#449).
 *
 * Not an `AssetRow`: this is a line in a list of gaps, so it carries what the
 * list draws — where the asset sits and what kind of thing it is — and none of
 * the rollup an `AssetRow` would drag with it.
 */
export interface UnmonitoredAsset {
  /** `asset:<uuid>` — the `#/asset/<id>` address the name opens. */
  id: string;
  /** One of the built-in types; what the tab's type filter narrows by. */
  type_id: string;
  /** What that type is called, resolved by the backend. */
  type_label: string;
  /** The type's two-character chip. */
  monogram: string;
  name: string;
  /** The ancestors' names, outermost first; `null` at the top of the estate. */
  path: string | null;
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
