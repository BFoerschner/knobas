//! The estate: assets, the tree they sit in, the routes they expose, the
//! Import that loads them from a file, and the commands that read and edit all
//! of it (spec #427 "M4.0 Estate", issues #428, #429, #431, #432, #434, #435
//! and #439).
//!
//! Six of the fifteen landed with #428; `asset_types` joined them with #429,
//! when the create dialog gave the built-in table a reader, `context_assets`
//! with #434 and `source_assets` with #435, when the room's Assets tile did,
//! the four route commands with #432, and the Import's pair with #439.
//!
//! `commands/assets.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one. The same arrangement `backup/`, `sources/` and `time/` use, and the
//! §10.8 exception this module lands under names the `time` pair as its
//! precedent.
//!
//! # What an asset is, and what it is not
//!
//! `CONTEXT.md`, **Asset**: a knobas-owned entity with a type, typed and
//! custom properties, and a place in the estate's tree. Two rows written
//! together, the pair notes have had since `0006` -- `knobas.entity` for the
//! address, the kind and the title, `knobas.asset` for everything else. It is
//! never a mirrored item: no sync writes one, and `0006`'s
//! `item_entity_reserved_chk` makes that structural rather than conventional.
//!
//! # The tree is a parent field (ADR-0014)
//!
//! An asset's place is `parent_id` and nothing else. There is no `holds`
//! relation and there is no closure table; `runs-on`, `depends-on`,
//! `monitored-by` and the rest stay ordinary links, and a container held under
//! a compose project is allowed to run on a VM elsewhere in the tree.
//!
//! **Cycles are refused at write time**, by [`move_to`], which walks the
//! ancestors of the proposed parent before it writes and names the asset it
//! would have run into. Migration `0017`'s `asset_no_self_parent_chk` closes
//! the one-step case structurally; nothing in SQL can see the longer ones, and
//! a trigger would be a second copy of a rule this module already owns.
//!
//! # Every mutation is a line in the asset's history
//!
//! Story 11: *"every mutation of an asset -- a property edit with old and new
//! value, a move, a rename -- is a line in its history"*. The activity stream
//! is that record and there is no second history table: [`create`], [`edit`],
//! [`move_to`] and [`delete`] each write through
//! `knobas_core::activity::record`, and [`get`] reads the same rows back
//! scoped to the asset. The detail carries **`from` and `to`** on every edit,
//! because "who changed the IP" is only answerable if the old value was
//! written down at the moment it stopped being true.
//!
//! # What is inherited, and what rolls up (#431)
//!
//! Two read-time computations, neither of them a stored column, because both
//! are answers about a *path* and a stored copy would be a second writer of
//! the parent field:
//!
//! * **Environment and owner are inherited** (stories 8, 9, 10). The value in
//!   force on an asset is the one set on the nearest asset at or above it, and
//!   the read hands back **which asset that was** -- so the pane can say *set
//!   here* or *inherited from `hel1`* and give the reader one click to the
//!   place the value can be changed. Setting a value on a child overrides the
//!   ancestor's for that child's whole subtree, and clearing it falls back to
//!   the ancestor again; both fall out of "nearest wins" and neither is a rule
//!   of its own. [`inherited`] is that walk, and it is pure over rows the
//!   reads already fetched.
//! * **Health rolls up** (story 37). An asset's health is the worst of its own
//!   status and every descendant's, **down over warn over up over none**, and
//!   `problems_inside` counts the descendants carrying `warn` or `down` --
//!   what story 32's amber-or-red badge draws. [`ROLLUP`] is the one statement
//!   that answers both, plus the worst status *strictly underneath*, which is
//!   what colours the badge. Monitors join the "own health" half in M4.1; that
//!   is the only part of story 37 not here.
//!
//! # Assets in contexts (#434)
//!
//! An asset is a member of a context through its **ancestors**: ADR-0008's
//! ratified clause, expanded over `parent_id` and never over links, inside
//! `knobas_core::context::member_ids`' one statement. This module owns no part
//! of that rule -- [`in_context`] asks for the answer and reads the assets
//! among the ids it gets back, so the room's Assets tile, the per-context
//! inbox filter and the tray's proposal scope are one walk and cannot
//! disagree.
//!
//! # Routes, read from both ends (#432)
//!
//! `CONTEXT.md`, **Route**: *"a knobas-owned entity an asset exposes: a URL or
//! endpoint, with or without a target asset. An asset is reachable via the
//! routes that land on it or on something that holds it"*. Both ends are
//! fields on one row (ADR-0014), and the two halves of that sentence are the
//! two reads [`get`] makes:
//!
//! * [`ROUTES_EXPOSED`] -- what this asset exposes, by `asset_id`;
//! * [`ROUTES_REACHABLE`] -- what lands on this asset **or anywhere else on
//!   its containment path**, above it or below it. A container's Traefik route
//!   is exposed by the proxy, lands on the container, and is also how the VM
//!   holding the container is reached. That statement's own docs give the
//!   argument in full, including the two readings it reconciles and the estate
//!   that decides between them.
//!
//! There is **one row shape for both**, [`RouteRow`], and *where* a route
//! lands is read off it rather than carried beside it: in `reachable_via`, a
//! route whose `target_id` is the asset's own id lands here, and any other
//! `target_id` is the asset on the path that it lands on, named by
//! `target_name` -- an ancestor when the pane's held-by path holds it and
//! something inside otherwise. A second field saying the same thing would be a
//! fact on the wire twice, and story 31's dashed wire is exactly that
//! comparison.
//!
//! A route has **no type and therefore no typed properties**: nineteen types
//! describe things that hold things, and a URL is not one of them. Its
//! properties are the reader's own, all four kinds, which is where the
//! certificate expiry of spec story 14 lives -- knobas does not own
//! monitoring, so an expiry knobas *checks* is a Kuma monitor (M4.1) and an
//! expiry knobas *records* is a property.
//!
//! # The Import (#439)
//!
//! `CONTEXT.md`, **Import**: *"loading assets from outside — an estate file,
//! later an adapter — with a preview of what is already in the tree and what
//! is new"*. [`preview_import`] and [`apply_import`] are those two halves, and
//! the preview **is** the plan: the apply reads its own preview's `changes`
//! and `monitor_links` to know what to write, so *"the preview said it would"*
//! is a property of the code rather than a pair of rules kept in step.
//!
//! Four decisions worth finding here rather than in a diff:
//!
//! * **What the file may overwrite is a property nobody has claimed.** Spec
//!   #427: *"a hand edit is any activity line by the user on that property"*.
//!   [`HAND_EDITED`] is that sentence as one statement, and [`ACTOR_IMPORT`] is
//!   what keeps the import's own lines out of it.
//! * **Nothing already in the tree is re-parented, renamed, or given a new
//!   environment or owner.** An import creates, and it sets *properties*:
//!   where an asset sits, what it is called and which environment and owner
//!   are set **on it** are what a person chose on purpose, and #439's
//!   criteria are about the property bag. The file's `environment` and `owner`
//!   are therefore read on the insert path alone, which is why the checked-in
//!   estate sets them at its root and nowhere else (`estate_file.rs` asserts
//!   exactly that) -- the root is created once and everything under it
//!   inherits.
//!   It is also what makes the uncapped recursive CTEs behind
//!   [`recompute_paths`] and [`ROLLUP`] safe under an import: the only
//!   `parent_id` written is on a row being inserted, in [`ordered`]'s
//!   parent-first order, and a file whose assets hold each other is refused by
//!   name before the first insert.
//! * **A monitor name is kept even when it resolves to nothing.** `0020` is
//!   the column, and it is the *"a name the mirror does not hold yet is kept on
//!   the asset"* half of spec #427's import sentence; the other half, drawing
//!   `monitored-by` links for the names the mirror does hold, is
//!   [`monitor_plan`], which since #442's Kuma adapter answers with real links
//!   -- and reports the names that still find nothing, so a reader is told on
//!   every preview and not only on the one that first kept a name (#445).
//! * **The file's plain scalars become tagged values here.** #428 chose
//!   `{"kind":…,"value":…}` and left the translation to this ticket;
//!   [`property_of`] is it, and the kind a type declares is what decides.
//!
//! # What this module deliberately does not do yet
//!
//! The create/edit surface (#429), the keyboard walk and spines (#430), the
//! routes above, and the Assets tile in every kind of room (#434, #435) have
//! landed: [`in_context`] serves a *stored* room and [`monitored_by`] a
//! *source* room, while *All work*'s top level is [`tree`] with no parent and
//! a project room reads nothing at all -- the frontend rule that picks among
//! the four is `app/src/lib/shell/assets-tile.ts`. A route is **not moved
//! between exposing assets** -- there is no `move_route` -- because a route is
//! the address *of* the thing that answers it: re-exposing one somewhere else
//! is a different route with a different history, which is a delete and a
//! create. The wires story 31 draws between a route row and its target are
//! #433's.
//!
//! Monitors arrived with #442's Kuma adapter, so [`monitored_by`] fills a
//! source room and [`AssetDetail::monitoring`] fills the pane's *monitoring*
//! section (#445). What is **not** here is health taking a monitor's state --
//! story 37's other half, and #444's, along with the alert an asset's monitor
//! opens.

use std::collections::{HashMap, HashSet};

use knobas_core::activity::ActivityRow;
use knobas_core::asset::{self, AssetType, PropertyKind};
use knobas_core::entity::EntityRef;
use knobas_core::link::LinkEntry;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::IpcError;

/// The activity actor for everything a person does to the estate.
const ACTOR: &str = "user";

/// The namespace and the `knobas.entity.kind` an asset is written under.
///
/// One word for both, the way `knobas_core::note` writes `note:` ids with kind
/// `note`, and one of `knobas_core::entity::RESERVED_NAMESPACES` -- which is
/// what `0006`'s `item_entity_reserved_chk` refuses to a mirror row and
/// therefore what makes an asset unsweepable. Spelled here rather than indexed
/// out of that array because an index is not a name; a test pins the two
/// together.
pub const NAMESPACE: &str = "asset";

/// The namespace and the `knobas.entity.kind` a route is written under.
///
/// [`NAMESPACE`]'s twin, and reserved for the same reason: `0006`'s
/// `item_entity_reserved_chk` has listed `route` since it was written, so no
/// mirror row can name a `route:` id and no sweep can reach one.
pub const ROUTE_NAMESPACE: &str = "route";

/// How many history lines the pane asks for.
///
/// A pane, not an audit log: the reader wants the recent story of this asset,
/// and a scroll of five hundred lines is a different surface. The whole stream
/// stays readable through `recent_activity`.
const HISTORY_LIMIT: i64 = 50;

/// What separates one ancestor's name from the next in `path_text`.
///
/// Read by the launcher through `knobas_search::corpus::ASSET`, so it is the
/// separator a reader sees. Not `knobas_core::payload::ANCESTOR_SEPARATOR`:
/// that one is the spelling a *payload* read joins a source's ancestors with,
/// and borrowing it would tie the estate's own rendering to an adapter rule
/// that may change for reasons that have nothing to do with assets.
const PATH_SEPARATOR: &str = " / ";

/// The relation a monitor is attached to an asset with (spec #427, story 67).
///
/// Spelled here because [`monitored_by_source`] reads it and there is nothing
/// else on this side of the bridge that knows the word: the relation
/// vocabulary is a *rendering* decision and lives in
/// `app/src/lib/detail/relations.ts`, which is where `monitored-by` is given
/// its two readings. The two spellings are pinned together by
/// `commands::assets`' mirror test, the way `DEFAULT_RELATION` is.
pub const MONITORED_BY: &str = "monitored-by";

/// The `knobas.entity.kind` a mirrored Uptime Kuma check carries.
///
/// Written before anything emitted it, so that a source room's tile would fill
/// with no change to this module the day the adapter landed -- which is what
/// happened in #442. It is one of `knobas_core::entity::RESERVED_NAMESPACES`
/// for the separate reason that `monitor:` ids are knobas' to give.
const MONITOR_KIND: &str = "monitor";

// ---------------------------------------------------------------------------
// The wire vocabularies
// ---------------------------------------------------------------------------

/// An asset's **own** status -- what a person recorded about it.
///
/// Not its health: story 37's health is the worst of this and its monitors'
/// states, and its *effective* health also takes the worst descendant. Both
/// are computed at read time, by [`ROLLUP`] over this column -- the monitors'
/// half is M4.1's and the only part of story 37 not here. This is the stored
/// fact those reads are built on, and [`AssetRow`] carries all three.
///
/// `none` is the default and means *nobody has said*, which is a different
/// thing from `up`. The four spellings are `0017`'s `asset_status_chk`, and
/// [`tests::every_status_is_one_the_schema_accepts`] reads the migration to
/// keep the two lists in step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Up,
    Warn,
    Down,
    #[default]
    None,
}

impl AssetStatus {
    /// Every status, for a walk that must not miss one.
    pub const ALL: [AssetStatus; 4] = [
        AssetStatus::Up,
        AssetStatus::Warn,
        AssetStatus::Down,
        AssetStatus::None,
    ];

    /// The stored spelling, which is also the wire value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AssetStatus::Up => "up",
            AssetStatus::Warn => "warn",
            AssetStatus::Down => "down",
            AssetStatus::None => "none",
        }
    }

    fn parse(value: &str) -> Result<Self, IpcError> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| IpcError::internal(format!("unknown asset status {value:?}")))
    }

    /// Where this status sits in story 37's order -- **down over warn over up
    /// over none** -- as a number whose *smaller* value is the worse one.
    ///
    /// Smaller-is-worse so that "the worst of a set" is a plain `min`, which
    /// is what [`ROLLUP`] takes over a subtree. That `case` expression is this
    /// function written in SQL, and
    /// [`tests::the_rollup_ranks_the_statuses_the_way_rust_does`] reads the
    /// statement out of this file to keep the two in step -- the discipline
    /// [`tests::every_status_is_one_the_schema_accepts`] applies to `0017`.
    ///
    /// Note what the order says about `none`: an asset nobody has rated shows
    /// its children's `up`, because `up` is *worse* than "nobody has said".
    /// That is spec #427's ordering read literally, and it is the reading that
    /// makes a branch of healthy things read as healthy.
    #[must_use]
    pub fn severity(self) -> i32 {
        match self {
            AssetStatus::Down => 0,
            AssetStatus::Warn => 1,
            AssetStatus::Up => 2,
            AssetStatus::None => 3,
        }
    }

    /// The status a [`severity`](Self::severity) belongs to.
    ///
    /// Anything outside the four is `none` rather than an error: the only
    /// producer is [`ROLLUP`]'s `case`, whose `else` arm is `none` already.
    #[must_use]
    fn of_severity(rank: i32) -> Self {
        AssetStatus::ALL
            .into_iter()
            .find(|status| status.severity() == rank)
            .unwrap_or(AssetStatus::None)
    }
}

/// Which environment an asset belongs to.
///
/// Stored on the asset, and story 8 inherits it from the nearest ancestor that
/// sets it -- that walk is #431's. The four spellings are `0017`'s
/// `asset_environment_chk`, kept in step with this list by
/// [`tests::every_environment_is_one_the_schema_accepts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Dev,
    Stage,
    Prod,
    Shared,
}

impl Environment {
    /// Every environment, for a walk that must not miss one.
    pub const ALL: [Environment; 4] = [
        Environment::Dev,
        Environment::Stage,
        Environment::Prod,
        Environment::Shared,
    ];

    /// The stored spelling, which is also the wire value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Dev => "dev",
            Environment::Stage => "stage",
            Environment::Prod => "prod",
            Environment::Shared => "shared",
        }
    }

    fn parse(value: &str) -> Result<Self, IpcError> {
        Self::ALL
            .into_iter()
            .find(|environment| environment.as_str() == value)
            .ok_or_else(|| IpcError::internal(format!("unknown environment {value:?}")))
    }
}

/// Who can reach a route.
///
/// Two values, because the question a reader asks of a URL is *can somebody
/// outside open this* -- and every finer shade (which VPN, which network,
/// which allow-list) is a fact with a name of its own and belongs in a
/// property. `internal` is the default: it is the safe reading of a route
/// nobody has classified, and `0018`'s `route_visibility_chk` is the same two
/// spellings, kept in step by
/// [`tests::every_visibility_is_one_the_schema_accepts`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Internal,
    Public,
}

impl Visibility {
    /// Every visibility, for a walk that must not miss one.
    pub const ALL: [Visibility; 2] = [Visibility::Internal, Visibility::Public];

    /// The stored spelling, which is also the wire value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Internal => "internal",
            Visibility::Public => "public",
        }
    }

    fn parse(value: &str) -> Result<Self, IpcError> {
        Self::ALL
            .into_iter()
            .find(|visibility| visibility.as_str() == value)
            .ok_or_else(|| IpcError::internal(format!("unknown route visibility {value:?}")))
    }
}

/// One property value, carrying the kind it was entered as.
///
/// Tagged, and the tag is stored: `{"kind":"number","value":8080}` on the wire
/// *and* in the `properties` jsonb. A bare JSON scalar would make `"8080"` and
/// `8080` the same property and would leave a date indistinguishable from a
/// string, which is the whole of what story 6's four kinds are for.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyValue {
    Text { value: String },
    Number { value: f64 },
    Date { value: String },
    Url { value: String },
}

impl PropertyValue {
    /// Which of the four this is.
    #[must_use]
    pub fn kind(&self) -> PropertyKind {
        match self {
            PropertyValue::Text { .. } => PropertyKind::Text,
            PropertyValue::Number { .. } => PropertyKind::Number,
            PropertyValue::Date { .. } => PropertyKind::Date,
            PropertyValue::Url { .. } => PropertyKind::Url,
        }
    }

    /// Refuse a value nothing could read back.
    ///
    /// Each rule is a failure a reader would otherwise meet later and
    /// elsewhere: a blank text is a property that reads as set and shows
    /// nothing (**clear it instead**, which is what `null` on an edit does), a
    /// non-finite number has no JSON spelling at all, a date that is not
    /// `YYYY-MM-DD` cannot be ordered against another one, and a URL with no
    /// scheme is a link the *open URL* action (#431) cannot open.
    fn vet(&self, key: &str) -> Result<(), IpcError> {
        let blank = |what: &str| {
            Err(IpcError::invalid(format!(
                "{key:?} was given a blank {what} -- clear the property instead"
            )))
        };
        match self {
            PropertyValue::Text { value } => {
                if value.trim().is_empty() {
                    return blank("text");
                }
            }
            PropertyValue::Number { value } => {
                if !value.is_finite() {
                    return Err(IpcError::invalid(format!(
                        "{key:?} was given a number that is not finite"
                    )));
                }
            }
            PropertyValue::Date { value } => {
                if value.trim().is_empty() {
                    return blank("date");
                }
                if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
                    return Err(IpcError::invalid(format!(
                        "{key:?} was given {value:?}, which is not a YYYY-MM-DD date"
                    )));
                }
            }
            PropertyValue::Url { value } => {
                if value.trim().is_empty() {
                    return blank("url");
                }
                if !(value.starts_with("http://") || value.starts_with("https://")) {
                    return Err(IpcError::invalid(format!(
                        "{key:?} was given {value:?}, which is not an http(s) URL"
                    )));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The wire shapes
// ---------------------------------------------------------------------------

/// One row of a Miller column, and the shape every read hands back.
///
/// `Row` and not `Node`: `CONTEXT.md`'s **Asset** entry lists *node* among the
/// words to avoid, and every list line this app already draws is a `…Row`
/// (`EntityRow`, `LinkRow`, `ActivityRow`, `NoteRow`, `ContextRow`). The tree
/// shape is `parent_id`'s, not this type's name's.
///
/// `has_children` is what the column draws its chevron from and what tells the
/// Tree that there is a next column to open. It is computed by the read rather
/// than stored: a stored count is a second copy of the parent field, and the
/// two would disagree the first time a move failed halfway.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AssetRow {
    /// `asset:<uuid>`, and also the id of this asset's `knobas.entity` row.
    pub id: String,
    /// The asset that holds this one; `null` at the top of the estate.
    pub parent_id: Option<String>,
    /// One of [`knobas_core::asset::TYPES`].
    pub type_id: String,
    /// What that type is called -- resolved here so no surface keeps a copy of
    /// the table.
    pub type_label: String,
    /// The type's two-character chip.
    pub monogram: String,
    pub name: String,
    /// The asset's **own** status. See [`AssetStatus`].
    pub status: AssetStatus,
    /// The environment set **on this asset**; `null` where it inherits (#431).
    pub environment: Option<Environment>,
    /// The owner set **on this asset**; `null` where it inherits (#431).
    pub owner: Option<String>,
    /// Whether anything sits under it.
    pub has_children: bool,
    /// The asset's **effective health**: the worst of [`status`](Self::status)
    /// and every descendant's own status, down over warn over up over none
    /// (story 37). Monitors join the "own" half in M4.1.
    pub health: AssetStatus,
    /// The worst status **strictly underneath** this asset; `none` when it
    /// holds nothing, or nothing under it has been rated.
    ///
    /// A field of its own rather than something the badge reads off
    /// [`health`](Self::health), because those two answer different questions
    /// the moment an asset is worse than what it holds: a `down` VM holding
    /// one `warn` container has `health = down` and `inside = warn`, and the
    /// badge is about what is *inside*, so it is amber.
    pub inside: AssetStatus,
    /// How many descendants carry `warn` or `down` -- the *N* in story 32's
    /// "N problems inside" badge. `0` draws no badge.
    pub problems_inside: i64,
    /// How many **work items** this asset is linked to -- story 32's *other*
    /// badge, the one #435 adds beside the problem count.
    ///
    /// A *work item* is an entity the mirror holds: a ticket, a build, a page,
    /// a commit. Contexts, notes and other assets are knobas' own and are not
    /// counted, which is the whole of what "linked **work**" means -- a
    /// container held under a compose project links to the VM it runs on, and
    /// that link is containment's neighbour rather than work.
    ///
    /// Confirmed links only, in either direction, and one per link rather than
    /// one per item: a pair carries one active link per relation
    /// (`link_pair_active_idx`), so an asset linked to one ticket as both
    /// `deployed-from` and `documented-in` is two facts and reads as two. See
    /// [`LINKED_WORK`].
    pub linked_work: i64,
}

/// One row of the pane's property list.
///
/// Typed properties come first, **in the type's declared order and including
/// the ones nobody has filled in** -- that is how a reader learns a VM is
/// meant to have an IP. Custom keys follow, in key order, and carry
/// `custom: true` so the pane can rule a line between the two groups without
/// keeping its own copy of the schema.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AssetProperty {
    pub key: String,
    /// What the pane calls it: the type's label for a typed key, the key
    /// itself for a custom one.
    pub label: String,
    /// `null` for a declared property with no value yet.
    pub value: Option<PropertyValue>,
    /// Whether this key is one the type declares.
    pub custom: bool,
}

/// A value **in force** on an asset, and the asset that sets it.
///
/// Stories 8, 9 and 10 in one shape: what the value is, and *where to go to
/// change it*. The source is a whole asset's id and name rather than a bare
/// `inherited: bool`, because "inherited" without a name leaves the reader
/// hunting up the path for the ancestor that decided it -- and the pane's one
/// click to that ancestor is exactly what story 10 asks for. Whether it was
/// set *here* is [`source_id`](Self::source_id) equalling the asset's own id,
/// which the pane compares rather than being told twice.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Inherited<T> {
    /// The value in force -- an [`Environment`] for one field of
    /// [`AssetDetail`], an owner's name for the other.
    pub value: T,
    /// The asset the value is set on -- this asset itself when it is set here.
    pub source_id: String,
    /// That asset's name, so the pane needs no second read to label the link.
    pub source_name: String,
}

/// Everything the fixed right pane draws for one asset.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AssetDetail {
    pub asset: AssetRow,
    /// Known keys first, custom after. See [`AssetProperty`].
    pub properties: Vec<AssetProperty>,
    /// The environment in force here and the asset that sets it (story 8);
    /// `null` when nothing at or above this asset sets one.
    pub effective_environment: Option<Inherited<Environment>>,
    /// The owner in force here and the asset that sets it (story 9).
    pub effective_owner: Option<Inherited<String>>,
    /// The path down to this asset, **outermost first and excluding itself**.
    /// Empty for an asset at the top of the estate.
    pub held_by: Vec<AssetRow>,
    /// What sits directly under it, in the column's own order.
    pub holds: Vec<AssetRow>,
    /// The routes **this asset exposes** (#432), by name.
    pub exposes: Vec<RouteRow>,
    /// The routes whose target is on this asset's containment path -- landing
    /// on it, on something that holds it, or on something it holds -- nearest
    /// first (#432). [`ROUTES_REACHABLE`] argues that rule.
    ///
    /// A route whose [`target_id`](RouteRow::target_id) is this asset's own id
    /// lands here; any other target is the asset on the path it lands on, and
    /// [`target_name`](RouteRow::target_name) names it. That comparison is the
    /// whole of "reached through something else", so there is no second field
    /// carrying it.
    pub reachable_via: Vec<RouteRow>,
    /// This asset's own lines from the activity stream, newest first.
    pub history: Vec<ActivityRow>,
    /// Every confirmed link this asset takes part in, either end, newest
    /// first -- story 36's *Link to…* read back (#435).
    ///
    /// `knobas_core::link::entries_of`'s answer, unfiltered and unsorted here:
    /// the panel that draws it is the one a ticket's detail draws
    /// (`app/src/lib/detail/LinksPanel.svelte`), and a second population --
    /// "work links" against "asset links" -- would be a second rule for the
    /// reader to learn on a surface whose whole point is that linking an asset
    /// is the same gesture as linking a ticket. What the *badge* counts is
    /// narrower and is [`AssetRow::linked_work`].
    pub links: Vec<LinkEntry>,
    /// The Uptime Kuma names of the monitors watching this asset, as an
    /// [`import`](apply_import) kept them (#439, `0020`).
    ///
    /// **Names, not monitors.** A name becomes a `monitored-by` link the
    /// moment the mirror holds a monitor called that -- and until then it is
    /// all knobas has, because no adapter emits the `monitor` kind before
    /// M4.1. So this list is what the estate file said and the [`links`] list
    /// is what has been resolved out of it; a name in both is a name whose
    /// monitor has arrived.
    ///
    /// On [`AssetDetail`] and not on [`AssetRow`], for [`properties`]' reason:
    /// a Miller column draws neither, and putting it on the row would put it
    /// on every column of every walk to serve one pane.
    ///
    /// [`links`]: AssetDetail::links
    /// [`properties`]: AssetDetail::properties
    pub monitors: Vec<String>,
    /// The monitors watching this asset, by name -- the pane's *monitoring*
    /// section (spec #427 story 33, issue #445).
    ///
    /// [`monitors`] and this are the two halves of one sentence: that list is
    /// what the estate file *said*, this is what has been found. A name in
    /// both is a name whose monitor has arrived; a name in only the first is
    /// one still waiting, and it is what the import's preview reports as
    /// [`UnresolvedMonitor`].
    ///
    /// Read out of [`links`] rather than by a second walk of `knobas.link`, so
    /// this section and the *Linked* panel under it cannot disagree about what
    /// is attached.
    ///
    /// [`links`]: AssetDetail::links
    /// [`monitors`]: AssetDetail::monitors
    pub monitoring: Vec<AttachedMonitor>,
}

/// One monitor watching an asset, as the pane's monitoring section draws it.
///
/// The attachment is the link; the state and the address are the mirror's, and
/// both are optional because both can honestly be missing -- a monitor Kuma
/// paused is a tombstone with neither (#442, `knobas_source_kuma::map`), and a
/// monitor whose source the reader disabled leaves the mirror without leaving
/// the link.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AttachedMonitor {
    /// The monitor's entity id -- `<source>:<monitor id in Kuma>`, and the
    /// address the *Linked* panel opens.
    pub entity_id: String,
    /// Its name, which is the name an estate file uses to ask for it.
    pub name: String,
    /// The state word the adapter last recorded: `up`, `down`, `pending`,
    /// `maintenance`. `null` when the mirror holds no reading -- a tombstone,
    /// or a state code the adapter has no word for.
    pub state: Option<String>,
    /// Its own page in Uptime Kuma, straight from the mirror row's `web_url`
    /// (spec #427 story 71). `null` when there is no page left to open.
    pub web_url: Option<String>,
    /// Whether the monitor has left the mirror -- paused in Kuma, or deleted.
    ///
    /// `CONTEXT.md`'s word for this state is **tombstone**: "marking an item
    /// deleted-at-source while keeping the row", against *withdraw*, which
    /// that glossary gives to a queued write pulled back. The fact itself is
    /// [`knobas_core::link::LinkEnd::deleted_at`], which the *Linked* panel
    /// under this section renders with the older spelling.
    ///
    /// A fact the reader is shown rather than a filter, for that field's own
    /// reason: a section that silently dropped a paused monitor would tell an
    /// asset somebody deliberately silenced a check on that nothing watches it.
    pub tombstoned: bool,
}

/// One row of the Monitors tab's roster -- every monitor the mirror holds, with
/// what the tab draws beside it (spec #427 story 68, issue #448).
///
/// **The whole roster in one read, and no per-monitor round trip.** A tab that
/// fetched a monitor's samples on demand would be one call per row on a
/// surface whose point is that every row is comparable at a glance; the counts
/// on the chips are a count of *these rows*, so the frontend cannot draw them
/// without the whole list anyway.
///
/// The fields split three ways and it is worth knowing which is which. The
/// identity and the address are `knobas.entity`'s and `sync.item`'s. The
/// **reading** -- [`monitor_type`], [`target`], [`response_time_ms`],
/// [`uptime`], [`cert_days_remaining`] -- is read out of the mirrored
/// payload, under ADR-0007's interim discipline (see [`reading_of`]). And
/// [`state`] and [`samples`] are knobas' own timeseries: *warn* exists
/// nowhere in Uptime Kuma and is derived at sample time (`0021`), so a chip
/// counting warns has to count samples and not the mirror.
///
/// [`monitor_type`]: MonitorRow::monitor_type
/// [`target`]: MonitorRow::target
/// [`response_time_ms`]: MonitorRow::response_time_ms
/// [`uptime`]: MonitorRow::uptime
/// [`cert_days_remaining`]: MonitorRow::cert_days_remaining
/// [`state`]: MonitorRow::state
/// [`samples`]: MonitorRow::samples
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MonitorRow {
    /// `<source>:<monitor id in Kuma>` -- the entity id, and the address a
    /// detail slide-over opens at.
    pub entity_id: String,
    /// The source that mirrored it. On the row because a second monitoring
    /// source is one more configured source and no new code (`samples::KIND`
    /// samples any source that emits the kind), so the roster has to be able
    /// to say which Kuma a monitor came from.
    pub source_id: String,
    /// Its name in Uptime Kuma, which is the name an estate file uses to ask
    /// for it.
    pub name: String,
    /// **The state knobas last recorded**: the newest [`sample`](MonitorSample)
    /// where there is one, and the mirror's own `state` where there is not.
    ///
    /// The newest sample first and not the mirror first, because the mirror
    /// carries Kuma's four words and the sample carries knobas' five: *warn*
    /// is derived from the response-time threshold at sample time and exists
    /// in no payload. A chip counting warns off the mirror would count none,
    /// for ever. The mirror is the fallback for the one case a sample cannot
    /// cover -- a monitor mirrored by a run that wrote no sample, or one whose
    /// samples retention has swept -- and it is the same
    /// `payload->>'state'` read the pane's monitoring section makes (#445).
    ///
    /// `null` is a genuine miss: a tombstone with no reading left, or a state
    /// code the adapter had no word for.
    pub state: Option<String>,
    /// `http`, `ping`, `docker`, `port` -- Kuma's own word for what kind of
    /// check this is. `null` where the payload does not say.
    pub monitor_type: Option<String>,
    /// **What it watches**: the URL where there is one, else the hostname with
    /// its port where there is one. One field and not three, because it is one
    /// column of the roster and the three are never all present -- an HTTP
    /// monitor has a URL and no hostname, a ping has a hostname and no URL.
    pub target: Option<String>,
    /// The reading Kuma last published, in milliseconds. `null` for a poll
    /// that did not answer -- Kuma's `-1` sentinel, which the adapter already
    /// carries as an absence.
    pub response_time_ms: Option<f64>,
    /// **When knobas last read this monitor** -- `sync.item.synced_at`, the
    /// same instant the run's samples carry.
    ///
    /// Kuma's `/metrics` has no clock in it at all (#442): there is no "last
    /// checked" to mirror, so the honest answer is when knobas looked. For a
    /// tombstoned monitor it is the run that noticed it had gone.
    pub checked_at: chrono::DateTime<chrono::Utc>,
    /// Kuma's sliding-window uptime ratios, by its own window labels (`1d`,
    /// `30d`, `365d`), in label order.
    ///
    /// A list and not three fields, for the reason the adapter's payload is a
    /// map: the windows are Kuma's choice and a release that adds a fourth
    /// should widen the payload rather than need a field here.
    pub uptime: Vec<UptimeRatio>,
    /// Days until the watched certificate expires, for a monitor that watches
    /// one. `null` for every monitor that does not, which is most of them.
    pub cert_days_remaining: Option<f64>,
    /// Its own page in Uptime Kuma (story 71); `null` when there is none left.
    pub web_url: Option<String>,
    /// Whether it has left the mirror -- **paused** in Kuma, or deleted.
    ///
    /// *Tombstoned* is `CONTEXT.md`'s word and [`AttachedMonitor`]'s field
    /// name; *Paused* is what the chip says, because pausing is what a reader
    /// did to make it true. A tombstoned monitor is still a row here: it keeps
    /// its samples, so the hours before it vanished are still drawable, and a
    /// roster that dropped it would say nothing about a monitor somebody
    /// silenced.
    pub tombstoned: bool,
    /// The assets this monitor is attached to by a `monitored-by` link, by
    /// name -- with the path that tells two containers called `postgres`
    /// apart. Empty for a monitor nothing is attached to, which the tab says
    /// in as many words.
    pub assets: Vec<MonitoredAsset>,
    /// Its samples inside the bar's window, **oldest first**.
    ///
    /// Raw and not bucketed, because the bucketing is the tab's and is tested
    /// there: the backend would otherwise own a number of segments that only a
    /// stylesheet knows. Bounded by the poll interval's own floor -- 60 s
    /// (`knobas_sync::config::check_interval`), so at most 1 440 per monitor
    /// per day -- which is why the window and not a `limit` is what bounds it.
    pub samples: Vec<MonitorSample>,
}

/// One uptime ratio, at the window Kuma computed it over.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct UptimeRatio {
    /// Kuma's own label: `1d`, `30d`, `365d`.
    pub window: String,
    /// `0.0` to `1.0`, as Kuma publishes it -- not a percentage.
    pub ratio: f64,
}

/// One asset a monitor is attached to.
///
/// Not a [`MemberAsset`]: a roster row needs a name, an address and a path,
/// and a tile's row carries a whole [`AssetRow`] with a health rollup behind
/// it -- a read per asset per monitor for facts this column does not draw.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MonitoredAsset {
    /// `asset:<uuid>` -- and the `#/asset/<id>` address the name links to.
    pub id: String,
    pub name: String,
    /// The ancestors' names, outermost first, `" / "` between; `null` for an
    /// asset at the top of the estate.
    pub path: Option<String>,
}

/// One sample as the bar draws it: when it was taken and what it read.
///
/// The response time is deliberately **not** here. The bar is coloured by
/// state, one segment per bucket, and a response time per sample would be
/// 1 440 numbers per monitor crossing the bridge for a chart this milestone
/// does not draw (spec #427, *Out of scope*: "charts beyond the 24-hour
/// bar"). The reading the tab does show is [`MonitorRow::response_time_ms`],
/// the latest one.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MonitorSample {
    pub taken_at: chrono::DateTime<chrono::Utc>,
    /// `up`, `warn`, `down`, `pending`, `maintenance`, or `null` for a poll
    /// whose state did not resolve -- which the bar draws as a gap.
    pub state: Option<String>,
}

/// One route, as both ends read it.
///
/// The same shape in `exposes` and in `reachable_via`, deliberately: it is one
/// row in the database and the two lists are two `where` clauses over it, so a
/// shape per direction would be two things to keep in step for no fact gained.
/// Both names are resolved by the read, so neither end has to fetch the other
/// to draw a line.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RouteRow {
    /// `route:<uuid>`, and also the id of this route's `knobas.entity` row --
    /// which is what makes `#/route/<id>` an address (spec §2).
    pub id: String,
    /// The asset that exposes it. Never null: a route with nothing answering
    /// it is not a row this model can hold.
    pub asset_id: String,
    /// That asset's name.
    pub asset_name: String,
    /// The asset it lands on, or `null` for an endpoint that lands on nothing
    /// knobas knows.
    pub target_id: Option<String>,
    /// That asset's name, `null` with the target.
    pub target_name: Option<String>,
    /// What the route is called -- *Gitea*, *TeamCity (tunnel)*.
    pub name: String,
    /// The URL or endpoint, carrying its scheme.
    pub url: String,
    pub visibility: Visibility,
    /// The reader's own keys, by key. A route has no type, so every one of
    /// them is custom -- see [`custom_properties`].
    pub properties: Vec<AssetProperty>,
}

/// Everything the pane draws for one route.
///
/// Its own history (story 11 applied to the other entity this module writes)
/// and the row. **No held-by path of its own**: a route sits on the asset that
/// exposes it, and [`RouteRow::asset_id`] is where the Tree opens -- which is
/// what makes `#/route/<id>` land at the exposing asset with the route
/// selected rather than in a surface of its own.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RouteDetail {
    pub route: RouteRow,
    /// This route's own lines from the activity stream, newest first.
    pub history: Vec<ActivityRow>,
}

/// One field of an asset, and what it is being set to.
///
/// A tagged union rather than a struct of optionals, and for the reason
/// `time::TimerTarget` is one: an edit is *one* field changing, every edit
/// writes its own history line with a `from` and a `to`, and a bag of
/// `Option`s would make "cleared" and "not mentioned" the same value on the
/// wire. `null` on the three nullable fields is therefore an unambiguous
/// **clear**.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum AssetEdit {
    Name {
        value: String,
    },
    Status {
        value: AssetStatus,
    },
    Environment {
        value: Option<Environment>,
    },
    Owner {
        value: Option<String>,
    },
    Property {
        key: String,
        value: Option<PropertyValue>,
    },
}

/// One field of a route, and what it is being set to.
///
/// [`AssetEdit`]'s shape for [`AssetEdit`]'s reason: one field at a time, one
/// history line each with a `from` and a `to`, and `null` on the two nullable
/// fields is an unambiguous **clear**. The asset that exposes a route is not
/// among them -- see the module docs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum RouteEdit {
    Name {
        value: String,
    },
    Url {
        value: String,
    },
    /// The asset it lands on; `null` makes it an endpoint that lands on
    /// nothing knobas knows, which is a legal route.
    Target {
        value: Option<String>,
    },
    Visibility {
        value: Visibility,
    },
    Property {
        key: String,
        value: Option<PropertyValue>,
    },
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// The column list every read below selects, written out in each statement.
///
/// **Never `fts`**: reading a `tsvector` into a row panics at run time
/// (interfaces §1), which is the one rule `knobas_search::corpus` makes
/// structural and the reason it is repeated here in prose. `path_text` is left
/// out of **these three** -- the launcher reads it through the corpus and no
/// *Miller column* draws it, so selecting it on every column of a walk would
/// be bytes nobody asked for. The two **tile** statements, [`MEMBER_ASSETS`]
/// (#434) and [`MONITORED_ASSETS`] (#435), do select it, and that is the whole
/// reason they are statements of their own rather than two more of these: a
/// room's Assets tile is a flat list of assets from anywhere in the estate, so
/// where each one sits is the column that makes it readable.
///
/// Five statements rather than one spliced constant: the SQL audit this repo
/// runs is over literal text, and a statement assembled from fragments is one
/// a reader cannot check by reading.
const CHILDREN: &str = "select a.id, a.parent_id, a.type_id, a.name, a.status, a.environment,
            a.owner,
            exists (select 1 from knobas.asset c where c.parent_id = a.id) as has_children
       from knobas.asset a
      where a.parent_id is not distinct from $1::text
      order by a.name asc, a.id asc";

/// The ancestors of one asset, outermost first.
const ANCESTORS: &str = "with recursive up as (
         select a.id, a.parent_id, 0 as depth from knobas.asset a
          where a.id = (select parent_id from knobas.asset where id = $1)
         union all
         select p.id, p.parent_id, up.depth + 1
           from knobas.asset p join up on p.id = up.parent_id
     )
     select a.id, a.parent_id, a.type_id, a.name, a.status, a.environment, a.owner,
            exists (select 1 from knobas.asset c where c.parent_id = a.id) as has_children,
            up.depth
       from knobas.asset a join up on up.id = a.id
      order by up.depth desc";

/// One asset by id.
const ONE: &str = "select a.id, a.parent_id, a.type_id, a.name, a.status, a.environment,
            a.owner,
            exists (select 1 from knobas.asset c where c.parent_id = a.id) as has_children
       from knobas.asset a where a.id = $1";

/// The properties bag and the monitor names, read on their own because they
/// are the two columns a *column row* has no use for.
///
/// `monitors` joined this read with the import (#439) rather than becoming a
/// statement of its own: it is one more column of the row the pane is already
/// fetching, and a second round trip for an array that is empty on most assets
/// would be a read per pane for nothing.
const PROPERTIES: &str = "select properties, monitors from knobas.asset where id = $1";

/// What one asset **exposes** (#432).
///
/// Both names are joined in rather than fetched by the caller: a pane drawing
/// "Gitea → knobas-gitea" would otherwise need a read per row, and the two
/// joins are the same index lookups the foreign keys already index. `ta` is a
/// **left** join because a route with no target is a legal row.
///
/// Three statements rather than one spliced constant, for [`CHILDREN`]'s
/// reason: the SQL audit is over literal text.
const ROUTES_EXPOSED: &str = "select r.id, r.asset_id, r.target_id, r.name, r.url,
            r.visibility, r.properties,
            ea.name as asset_name, ta.name as target_name
       from knobas.route r
       join knobas.asset ea on ea.id = r.asset_id
       left join knobas.asset ta on ta.id = r.target_id
      where r.asset_id = $1
      order by r.name asc, r.id asc";

/// What one asset is **reachable via**: every route whose target sits on this
/// asset's own containment path -- the asset itself, anything that holds it,
/// anything it holds (#432).
///
/// # Why the path and not only the ancestors
///
/// The two documents this ticket answers to read the relation in opposite
/// directions, and both are true sentences about reachability:
///
/// * `CONTEXT.md`, **Route**: *"an asset is reachable via the routes that land
///   on it **or on something that holds it**"* -- the ancestors. A URL landing
///   on a VM is how the service inside it is reached.
/// * Issue #432's acceptance criterion: *"the container reads it under
///   reachable-via, **and so does the VM that holds the container**"* -- the
///   descendants. A URL landing on a container is how the machine running it
///   is reached.
///
/// The estate settles it (ADR-0013: the real thing is the witness).
/// **Every** route in `testenv/hetzner/estate.json` lands on a container, so
/// under the ancestors alone no server, no engine and no site in the real
/// estate would ever read a single route -- the pane's *Reachable via* would
/// be empty on every asset a person actually looks at, and "how is this box
/// reached" would have no answer. So the rule is the union the two readings
/// have between them, and it is one rule rather than two: **a route reaches an
/// asset when its target is on the same containment path**, in either
/// direction. Nothing else changes; the direction it was reached from is read
/// off the row by the pane, which has the held-by path already.
///
/// **Why the descendants are the whole subtree and not one hop.** The
/// criterion's own words -- *the VM that holds the container* -- read like one
/// step, and in the real estate they are not: a container sits inside a
/// `container_engine` which sits on the `vm`
/// (`testenv/hetzner/estate.json`: `knobas-teamcity` the container, *Docker
/// engine (knobas-teamcity)*, `knobas-teamcity` the VM). A one-hop rule would
/// stop at the engine and answer nothing for the box, which is the question
/// being asked. There is no honest depth between one and all of them.
///
/// The cost is that the estate's **root** reads every route the estate has.
/// That is the reading taken deliberately rather than a case not thought of:
/// it is true (everything under it is reached by those routes), it is ordered
/// nearest-first so the useful rows are at the top, and a cap belongs to the
/// surface that finds it too long rather than to the read -- the pane draws a
/// list per asset, and the asset a person works on is never the root.
///
/// `up` is [`ANCESTORS`]' walk, starting at the asset **itself** rather than
/// at its parent, and `down` is [`ROLLUP`]'s subtree walk. `union` rather than
/// `union all` where they meet: the asset is depth 0 of both and is one row,
/// not two. A route cannot appear twice -- its one target is either above,
/// below or the asset, never two of those, because `parent_id` has no cycles.
///
/// `order by depth` puts the nearest first, which is the order the question is
/// asked in: what lands *here* before what lands on the box this is on.
///
/// `ta` is an inner join here and a left join in [`ROUTES_EXPOSED`], and that
/// is not an inconsistency: every row this statement can return has a target,
/// because `r.target_id = path.id` is what selected it.
const ROUTES_REACHABLE: &str = "with recursive up (id, parent_id, depth) as (
         select a.id, a.parent_id, 0 from knobas.asset a where a.id = $1
         union all
         select p.id, p.parent_id, up.depth + 1
           from knobas.asset p join up on p.id = up.parent_id
     ),
     down (id, depth) as (
         select a.id, 0 from knobas.asset a where a.id = $1
         union all
         select c.id, down.depth + 1
           from knobas.asset c join down on c.parent_id = down.id
     ),
     path (id, depth) as (
         select id, depth from up
         union
         select id, depth from down
     )
     select r.id, r.asset_id, r.target_id, r.name, r.url, r.visibility, r.properties,
            ea.name as asset_name, ta.name as target_name
       from path
       join knobas.route r on r.target_id = path.id
       join knobas.asset ea on ea.id = r.asset_id
       join knobas.asset ta on ta.id = r.target_id
      order by path.depth asc, r.name asc, r.id asc";

/// One route by id.
const ROUTE_ONE: &str = "select r.id, r.asset_id, r.target_id, r.name, r.url,
            r.visibility, r.properties,
            ea.name as asset_name, ta.name as target_name
       from knobas.route r
       join knobas.asset ea on ea.id = r.asset_id
       left join knobas.asset ta on ta.id = r.target_id
      where r.id = $1";

/// The health rollup for a set of assets: the worst status at or under each
/// one, the worst status strictly under it, and how many descendants carry a
/// problem (stories 32 and 37).
///
/// **One statement, run once per read**, rather than the same recursive block
/// spliced into all three of the statements above. It is still a whole
/// literal a reader checks by reading -- which is what that rule is for -- and
/// it keeps spec #427's ordering, *"down over warn over up over none"*, in one
/// place instead of three; the price is one round trip per column, against
/// three copies of a rule that would drift.
///
/// `depth` is what separates the two answers. The anchor row is the asset
/// itself at depth 0, so `health`'s `min` sees the asset's own status and
/// `inside`'s does not -- and `problems_inside` counts only what is
/// underneath, which is the whole meaning of the badge. The `case` arms are
/// [`AssetStatus::severity`] in SQL, and a test reads them back out of this
/// string.
///
/// No depth cap, for [`move_to`]'s reason: `parent_id` has one writer and it
/// refuses cycles before it writes, with `asset_no_self_parent_chk` under it.
const ROLLUP: &str = "with recursive under (root, id, depth) as (
         select a.id, a.id, 0 from knobas.asset a where a.id = any($1::text[])
         union all
         select u.root, c.id, u.depth + 1
           from under u join knobas.asset c on c.parent_id = u.id
     )
     select u.root,
            min(case d.status when 'down' then 0 when 'warn' then 1
                              when 'up' then 2 else 3 end) as health,
            min(case when u.depth = 0 then 3
                     else case d.status when 'down' then 0 when 'warn' then 1
                                        when 'up' then 2 else 3 end end) as inside,
            count(*) filter (where u.depth > 0 and d.status in ('warn','down'))
              as problems_inside
       from under u join knobas.asset d on d.id = u.id
      group by u.root";

/// How many work items each of a set of assets is linked to -- the *linked
/// work* badge (story 32, issue #435).
///
/// **`sync.live_item` is what makes an item "work".** The join is the whole
/// definition: an entity the mirror holds is a ticket, a build, a page or a
/// commit, and an entity it does not hold is a context, a note or another
/// asset -- knobas' own, and not work. Going through the *view* rather than
/// `sync.item` also carries the tombstone and disabled-source filters, so a
/// badge never counts a ticket the source withdrew. That is deliberately
/// **not** what `knobas_core::link::entries_of` does: the pane's list keeps a
/// withdrawn end visible and marks it, because a link that dangles must not
/// vanish silently -- but a *number* has nothing to mark, and a count that
/// included the withdrawn ticket would send the reader looking for work that
/// is not there.
///
/// Confirmed only: `knobas.confirmed_link` cannot hold a proposal, so a
/// suggestion nobody has accepted is not counted and cannot be, however this
/// statement is later edited.
///
/// A statement of its own, run once per read like [`ROLLUP`] and for the same
/// reason: a whole literal a reader checks by reading, rather than a fragment
/// spliced into the five column statements.
const LINKED_WORK: &str = "select a.id as root, count(*) as linked_work
       from unnest($1::text[]) as a(id)
       join knobas.confirmed_link l on l.from_id = a.id or l.to_id = a.id
       join sync.live_item i
         on i.entity_id = case when l.from_id = a.id then l.to_id else l.from_id end
      group by a.id";

/// [`LINKED_WORK`] for `ids`, keyed by asset id.
///
/// An asset linked to nothing is **absent** rather than zero -- `count(*)`
/// over a join that matched nothing produces no group -- so the caller reads
/// `0` off the missing entry, which is the same answer with one fewer row on
/// the wire.
async fn linked_work(pool: &PgPool, ids: &[String]) -> Result<HashMap<String, i64>, IpcError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(LINKED_WORK).bind(ids).fetch_all(pool).await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in &rows {
        out.insert(
            row.try_get::<String, _>("root")?,
            row.try_get::<i64, _>("linked_work")?,
        );
    }
    Ok(out)
}

/// What [`ROLLUP`] answers about one asset.
///
/// Every id handed to [`rollup`] comes back, because the statement's anchor
/// row is the asset itself -- so the only way [`row_of`] finds no entry is an
/// asset deleted between its read and the rollup's. There is no `Default` for
/// that case on purpose: the honest answer is the row's **own** status, which
/// `row_of` reads off the row it already has, and a `Default` would have made
/// a live `up` asset report `none` for the one frame it took to vanish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rollup {
    health: AssetStatus,
    inside: AssetStatus,
    problems_inside: i64,
}

/// [`ROLLUP`] for `ids`, keyed by asset id.
async fn rollup(pool: &PgPool, ids: &[String]) -> Result<HashMap<String, Rollup>, IpcError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(ROLLUP).bind(ids).fetch_all(pool).await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in &rows {
        out.insert(
            row.try_get::<String, _>("root")?,
            Rollup {
                health: AssetStatus::of_severity(row.try_get("health")?),
                inside: AssetStatus::of_severity(row.try_get("inside")?),
                problems_inside: row.try_get("problems_inside")?,
            },
        );
    }
    Ok(out)
}

fn row_of(
    row: &sqlx::postgres::PgRow,
    rollups: &HashMap<String, Rollup>,
    linked: &HashMap<String, i64>,
) -> Result<AssetRow, IpcError> {
    let id: String = row.try_get("id")?;
    let type_id: String = row.try_get("type_id")?;
    let declared = asset::find(&type_id);
    let status = AssetStatus::parse(&row.try_get::<String, _>("status")?)?;
    let environment: Option<String> = row.try_get("environment")?;
    let rolled = rollups.get(&id).copied();
    // Read here rather than in the field below: `id` is moved into the row.
    let work = linked.get(&id).copied().unwrap_or(0);
    Ok(AssetRow {
        id,
        parent_id: row.try_get("parent_id")?,
        type_label: declared.map_or_else(|| type_id.clone(), |t| t.label.to_owned()),
        // A type the table has lost is drawn as `??` rather than refused: the
        // row exists, a reader has to be able to see it and move it, and the
        // one way that can happen is an estate file (#439) naming a type this
        // build does not carry.
        monogram: declared.map_or("??", |t| t.monogram).to_owned(),
        type_id,
        name: row.try_get("name")?,
        status,
        environment: environment.as_deref().map(Environment::parse).transpose()?,
        owner: row.try_get("owner")?,
        has_children: row.try_get("has_children")?,
        health: rolled.map_or(status, |r| r.health),
        inside: rolled.map_or(AssetStatus::None, |r| r.inside),
        problems_inside: rolled.map_or(0, |r| r.problems_inside),
        linked_work: work,
    })
}

/// Every row of one read, with [`ROLLUP`] run once for the lot.
///
/// The rollup is a read of its own rather than a join in the three statements
/// above, so this is where a `Vec<PgRow>` becomes a `Vec<AssetRow>`: one
/// round trip per read, whatever the column holds.
async fn rows_of(pool: &PgPool, rows: &[sqlx::postgres::PgRow]) -> Result<Vec<AssetRow>, IpcError> {
    let ids = rows
        .iter()
        .map(|row| row.try_get::<String, _>("id"))
        .collect::<Result<Vec<_>, _>>()?;
    let rollups = rollup(pool, &ids).await?;
    let linked = linked_work(pool, &ids).await?;
    rows.iter()
        .map(|row| row_of(row, &rollups, &linked))
        .collect()
}

/// The nearest asset that sets a value: this one, then up the held-by path.
///
/// Pure, and separately tested, because it is stories 8, 9 and 10's whole
/// rule and it needs no database to be got wrong. `held_by` arrives
/// **outermost first** ([`AssetDetail::held_by`]), so the walk is `rev()` --
/// nearest ancestor last in that list, first here. Reversing it the other way
/// would answer with the *outermost* setter, which is the value a child was
/// meant to override.
///
/// A child that sets its own value therefore wins for its whole subtree
/// (nothing above it is ever consulted), and clearing that value falls back to
/// the ancestor again (there is nothing else for the walk to find). Both are
/// consequences of "nearest wins" rather than rules of their own, which is why
/// there is one function here and not three.
fn inherited<T>(
    asset: &AssetRow,
    held_by: &[AssetRow],
    of: impl Fn(&AssetRow) -> Option<T>,
) -> Option<Inherited<T>> {
    std::iter::once(asset)
        .chain(held_by.iter().rev())
        .find_map(|row| {
            of(row).map(|value| Inherited {
                value,
                source_id: row.id.clone(),
                source_name: row.name.clone(),
            })
        })
}

/// The built-in type table, as the create dialog and the pane's editor read
/// it.
///
/// A read with no database under it: the table is a `const` in
/// `knobas_core::asset`, because the half of a type that matters -- the
/// monogram, the *ordered* property schema, and the child types conventionally
/// suggested -- cannot be written in SQL. It is put on the wire rather than
/// copied into TypeScript so that the nineteen types are one list, and because
/// the surface #429 builds needs two things nothing else on this bridge
/// carries:
///
/// * **which types are usual under the asset the plus was pressed on** (story
///   17), which is `AssetType::suggests`; and
/// * **which kind an unfilled typed property takes**. [`AssetProperty`] hands
///   the pane a declared key with `value: null`, and there is no kind in a
///   `null` -- so without the schema a reader filling in a VM's `ip` would be
///   guessing whether the backend wants text or a number.
///
/// `Vec` and not `&'static [AssetType]` because a command's answer is
/// serialized and owned; the copy is nineteen structs of pointers, once per
/// window.
#[must_use]
pub fn types() -> Vec<AssetType> {
    asset::TYPES.to_vec()
}

/// The children of `parent_id`, or the estate's top level when it is `None`.
///
/// The Tree's whole read: one call per column, and a column is one statement.
/// A recursive read of the entire estate would be one round trip and an
/// unbounded answer -- the thing Miller columns exist to avoid.
///
/// # Errors
///
/// [`IpcError`] if the read fails.
pub async fn tree(pool: &PgPool, parent_id: Option<&str>) -> Result<Vec<AssetRow>, IpcError> {
    let rows = sqlx::query(CHILDREN)
        .bind(parent_id)
        .fetch_all(pool)
        .await?;
    rows_of(pool, &rows).await
}

/// One asset, with everything the pane draws.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no asset carries; [`IpcError`] if a read
/// fails.
pub async fn get(pool: &PgPool, id: &str) -> Result<AssetDetail, IpcError> {
    let row = sqlx::query(ONE)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| no_such_asset(id))?;
    let asset = rows_of(pool, std::slice::from_ref(&row))
        .await?
        .pop()
        .ok_or_else(|| no_such_asset(id))?;

    let ancestors = sqlx::query(ANCESTORS).bind(id).fetch_all(pool).await?;
    let held_by = rows_of(pool, &ancestors).await?;
    let holds = tree(pool, Some(id)).await?;

    let row = sqlx::query(PROPERTIES).bind(id).fetch_one(pool).await?;
    let stored: serde_json::Value = row.try_get("properties")?;
    let monitors: Vec<String> = row.try_get("monitors")?;

    let exposes = routes(pool, ROUTES_EXPOSED, id).await?;
    let reachable_via = routes(pool, ROUTES_REACHABLE, id).await?;

    let entity = entity_of(id)?;
    let history = knobas_core::activity::recent(pool, HISTORY_LIMIT, Some(&entity)).await?;
    // The same read a ticket's detail makes, against the same statement: an
    // asset is an entity, so "what is this linked to" has one answer in this
    // app and the pane draws it through the panel the slide-over draws.
    let links = knobas_core::link::entries_of(pool, &entity).await?;
    let monitoring = attached_monitors(pool, &links).await?;

    Ok(AssetDetail {
        properties: properties_of(&asset.type_id, &stored),
        effective_environment: inherited(&asset, &held_by, |row| row.environment),
        effective_owner: inherited(&asset, &held_by, |row| row.owner.clone()),
        asset,
        held_by,
        holds,
        exposes,
        reachable_via,
        history,
        monitoring,
        links,
        monitors,
    })
}

/// The state and the page in Kuma of the monitors the mirror still holds.
///
/// Keyed on entity id, and every column read from `sync.live_item` so a
/// monitor of a source the reader disabled contributes no reading -- the
/// filter every reader in this app inherits (`0012`). The *attachment* is not
/// read here: that is the link, and a link survives both the tombstone and the
/// disabled source.
const MONITOR_READINGS: &str = "select entity_id, payload->>'state' as state, web_url
  from sync.live_item
 where kind = $2 and entity_id = any($1::text[])";

/// The monitors watching an asset, out of the links it already takes part in.
///
/// **The link is the attachment and the mirror is only the reading.** So the
/// walk is over `links` -- narrowed to [`MONITORED_BY`] and to the `monitor`
/// kind, which are two conditions and not one: a `related` link to a monitor
/// is not monitoring, and a `monitored-by` link to a ticket is not a monitor.
/// `entries_of` has already resolved each other end's kind and title, so the
/// name on every row is here whether or not the mirror still answers for it.
///
/// By name, then by id: the section is a list a person reads down, and
/// `entries_of`'s own order is newest-link-first, which is an order about when
/// somebody attached things rather than about what is being watched.
async fn attached_monitors(
    pool: &PgPool,
    links: &[LinkEntry],
) -> Result<Vec<AttachedMonitor>, IpcError> {
    let attached: Vec<&LinkEntry> = links
        .iter()
        .filter(|entry| entry.link.relation == MONITORED_BY && entry.other.kind == MONITOR_KIND)
        .collect();
    if attached.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = attached
        .iter()
        .map(|entry| entry.other.entity_id.clone())
        .collect();
    let mut readings: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for row in sqlx::query(MONITOR_READINGS)
        .bind(&ids)
        .bind(MONITOR_KIND)
        .fetch_all(pool)
        .await?
    {
        readings.insert(
            row.try_get("entity_id")?,
            (row.try_get("state")?, row.try_get("web_url")?),
        );
    }

    let mut out: Vec<AttachedMonitor> = attached
        .iter()
        .map(|entry| {
            let (state, web_url) = readings
                .get(&entry.other.entity_id)
                .cloned()
                .unwrap_or_default();
            AttachedMonitor {
                entity_id: entry.other.entity_id.clone(),
                name: entry.other.title.clone(),
                state,
                web_url,
                tombstoned: entry.other.deleted_at.is_some(),
            }
        })
        .collect();
    out.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
    Ok(out)
}

/// How far back the Monitors tab's bar reaches, in hours (spec #427 story 68:
/// *"a 24-hour bar per monitor drawn from the samples"*).
///
/// Here and not in the tab, because it is what bounds the answer this module
/// hands over: at the sync interval's own floor of 60 s
/// (`knobas_sync::config::check_interval`) it is at most 1 440 samples per
/// monitor, and that bound is the reason the read needs no `limit`.
const BAR_WINDOW_HOURS: i32 = 24;

/// Every monitor the mirror holds, tombstones included.
///
/// **`sync.item` and not `sync.live_item`**, which is the one place in this
/// module that departs from the view every other read goes through. That view
/// drops an entity with a `deleted_at`, and a tombstoned monitor -- one Kuma
/// no longer publishes, which is what *paused* looks like from `/metrics`
/// (#442) -- is a row the roster has to draw: it keeps its samples, so the
/// hours before it went quiet are still there, and a roster that dropped it
/// would say nothing about a monitor somebody deliberately silenced. The
/// **other** half of the view is kept and kept deliberately: a disabled
/// source's items are hidden here as everywhere else (`0012`), because turning
/// a source off is a statement about what the reader wants to see, and it is a
/// different statement from Kuma pausing a monitor.
///
/// **It is `sync.live_item`'s body minus one clause, and this names the drift
/// rather than preventing it**: `coalesce(s.enabled, true)` is now written in
/// two places, here and in `0012`. A shared `sync.item_of_enabled_source` view
/// would keep one spelling and is a migration and a §10.8 conversation;
/// `CONTEXT.md`'s **Live item** entry carries the exception, so the next
/// reader tempted to copy this finds the reason before the statement.
///
/// By name, then by id: the tab is a list a person reads down, and two
/// monitors named the same still draw in a fixed order.
const ROSTER: &str = "select i.entity_id, i.source_id, i.title, i.payload, i.web_url, i.synced_at,
                             e.deleted_at is not null as tombstoned
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config s on s.id = i.source_id
 where i.kind = $1
   and coalesce(s.enabled, true)
 order by i.title asc, i.entity_id asc";

/// Every sample of these monitors inside the bar's window, oldest first.
///
/// One statement for the whole roster rather than one per row: the tab draws
/// every bar at once, and a call per monitor would be a round trip per row of
/// a surface whose point is that the rows are read together.
///
/// `now()` is the **database's** clock and not a parameter, because it is the
/// same clock `monitor_sample.taken_at` defaults to -- so "the last day" is
/// one instant's arithmetic rather than two machines' opinions. The tab
/// buckets against the reader's own clock, which is a different question: what
/// this bounds is how much crosses the bridge.
const ROSTER_SAMPLES: &str = "select entity_id, taken_at, state
  from knobas.monitor_sample
 where entity_id = any($1::text[])
   and taken_at >= now() - make_interval(hours => $2::int)
 order by entity_id asc, taken_at asc";

/// The assets these monitors are attached to, by `monitored-by`.
///
/// **Undirected**, like every other reader of this relation: a link is one row
/// and `0011` made the pair unordered, so which end the import or *Link to…*
/// happened to write it from is not a fact any read is allowed to depend on.
/// `a.id <> m.id` is the guard that keeps a link from an asset to itself out
/// of the join rather than drawing the asset as its own monitor's asset.
///
/// `$1` is the monitor ids and `$2` the relation ([`MONITORED_BY`]), bound
/// rather than written into the text, which is [`MONITORED_ASSETS`]' rule.
const ROSTER_ASSETS: &str = "select m.id as monitor_id, a.id, a.name,
                                    nullif(a.path_text, '') as path
  from unnest($1::text[]) as m(id)
  join knobas.confirmed_link l on m.id in (l.from_id, l.to_id)
  join knobas.asset a on a.id in (l.from_id, l.to_id) and a.id <> m.id
 where l.relation = $2
 order by a.name asc, a.id asc";

/// What the Monitors tab draws: every mirrored monitor with its state, its
/// last day of samples, its last check, its uptime, its certificate days and
/// the assets it watches (spec #427 story 68, issue #448).
///
/// # Three statements, not one and not one per row
///
/// The roster, its samples and its attachments are three different
/// cardinalities over the same set of ids -- one row per monitor, up to 1 440
/// per monitor, and zero-or-more per monitor. Joined into one statement the
/// payload would be multiplied out; asked per monitor they would be a round
/// trip per row. So: read the monitors, then ask each of the other two once,
/// for the ids the first found.
///
/// # The state is the bar's right-hand end
///
/// [`MonitorRow::state`] is the newest sample in the window, and the mirror's
/// own `state` only where the window holds none. That is not a preference
/// between two sources, it is what makes the chip and the bar one statement:
/// a *warn* chip counts a state that exists nowhere in Uptime Kuma -- knobas
/// derives it from the response-time threshold at sample time (`0021`) -- so a
/// roster that took its state from the mirror would count no warns for ever
/// while drawing amber segments in every bar.
///
/// # Errors
///
/// [`IpcError`] if a read fails.
pub async fn monitor_roster(pool: &PgPool) -> Result<Vec<MonitorRow>, IpcError> {
    let rows = sqlx::query(ROSTER)
        .bind(MONITOR_KIND)
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("entity_id"))
        .collect::<Result<_, _>>()?;

    let mut samples: HashMap<String, Vec<MonitorSample>> = HashMap::new();
    for sample in sqlx::query(ROSTER_SAMPLES)
        .bind(&ids)
        .bind(BAR_WINDOW_HOURS)
        .fetch_all(pool)
        .await?
    {
        samples
            .entry(sample.try_get("entity_id")?)
            .or_default()
            .push(MonitorSample {
                taken_at: sample.try_get("taken_at")?,
                state: sample.try_get("state")?,
            });
    }

    let mut attached: HashMap<String, Vec<MonitoredAsset>> = HashMap::new();
    for row in sqlx::query(ROSTER_ASSETS)
        .bind(&ids)
        .bind(MONITORED_BY)
        .fetch_all(pool)
        .await?
    {
        attached
            .entry(row.try_get("monitor_id")?)
            .or_default()
            .push(MonitoredAsset {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
            });
    }

    rows.iter()
        .map(|row| {
            let entity_id: String = row.try_get("entity_id")?;
            let payload: serde_json::Value = row.try_get("payload")?;
            let reading = reading_of(&payload);
            let samples = samples.remove(&entity_id).unwrap_or_default();
            let state = match samples.last() {
                Some(newest) => newest.state.clone(),
                None => text_at(&payload, "state"),
            };
            Ok(MonitorRow {
                source_id: row.try_get("source_id")?,
                name: row.try_get("title")?,
                state,
                monitor_type: reading.monitor_type,
                target: reading.target,
                response_time_ms: reading.response_time_ms,
                checked_at: row.try_get("synced_at")?,
                uptime: reading.uptime,
                cert_days_remaining: reading.cert_days_remaining,
                web_url: row.try_get("web_url")?,
                tombstoned: row.try_get("tombstoned")?,
                assets: attached.remove(&entity_id).unwrap_or_default(),
                samples,
                entity_id,
            })
        })
        .collect()
}

/// The half of a [`MonitorRow`] that is read out of the mirrored payload.
///
/// A struct rather than five returns, so [`reading_of`] is one named place and
/// not five call sites that could each drift.
struct Reading {
    monitor_type: Option<String>,
    target: Option<String>,
    response_time_ms: Option<f64>,
    uptime: Vec<UptimeRatio>,
    cert_days_remaining: Option<f64>,
}

/// Read a monitor's payload for the columns the roster draws.
///
/// # This is a payload read outside an adapter (ADR-0007, #277)
///
/// `KindPaths` has no slot shaped like any of these -- no "what kind of check",
/// no "what it watches", no "a ratio per window" -- so declaring them would
/// itself be a `crates/knobas-source/src/**` change and a §10.8 conversation of
/// its own. Until there is such a slot the interim discipline applies in full,
/// and this function meets all three requirements the way
/// `knobas_sync::samples::sample_of` does:
///
/// 1. **It misses, never guesses.** Every field is `None` (or, for the ratios,
///    absent from the list) unless the payload holds the shape it expects: a
///    non-string type, a non-number reading, a `uptime` that is not an object,
///    a ratio that is not a number -- each one is left out rather than
///    coerced. A blank string is a miss too, for `estate_file.rs`' reason:
///    *nothing here* and *somebody meant to fill this in* must not read alike.
/// 2. **One named place**, this function, reached from [`monitor_roster`] and
///    from nowhere else. The one read outside it is `state`, which
///    [`monitor_roster`] takes with [`text_at`] because it is the *fallback*
///    for a value the samples normally supply, and the pane's own monitoring
///    section already reads it that way (#445, [`MONITOR_READINGS`]).
/// 3. **The failure direction is absence.** A drifted key draws an empty cell
///    in the roster -- never a target that is somebody else's host, never a
///    ratio invented from a string.
///
/// # The target is one column
///
/// The URL where there is one, else the hostname, with `:port` after it where
/// there is a port. Kuma gives an HTTP monitor a URL and no hostname and a
/// ping a hostname and no URL, so the three keys are one fact under three
/// spellings and the roster draws it in one column.
fn reading_of(payload: &serde_json::Value) -> Reading {
    let hostname = text_at(payload, "hostname");
    let target = text_at(payload, "url").or_else(|| match (hostname, text_at(payload, "port")) {
        (Some(host), Some(port)) => Some(format!("{host}:{port}")),
        (host, _) => host,
    });
    Reading {
        monitor_type: text_at(payload, "type"),
        target,
        response_time_ms: number_at(payload, "response_time_ms"),
        uptime: payload
            .get("uptime")
            .and_then(serde_json::Value::as_object)
            .map(|windows| {
                windows
                    .iter()
                    .filter_map(|(window, ratio)| {
                        Some(UptimeRatio {
                            window: window.clone(),
                            ratio: ratio.as_f64()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        cert_days_remaining: number_at(payload, "cert_days_remaining"),
    }
}

/// One string off a payload, or nothing -- including for a blank one.
fn text_at(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// One number off a payload, or nothing.
fn number_at(payload: &serde_json::Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(serde_json::Value::as_f64)
}

/// One route, with its own history -- what `#/route/<id>` opens on.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no route carries; [`IpcError`] if a read
/// fails.
pub async fn get_route(pool: &PgPool, id: &str) -> Result<RouteDetail, IpcError> {
    let route = one_route(pool, id).await?;
    let entity = entity_of(id)?;
    let history = knobas_core::activity::recent(pool, HISTORY_LIMIT, Some(&entity)).await?;
    Ok(RouteDetail { route, history })
}

/// [`ROUTES_EXPOSED`] or [`ROUTES_REACHABLE`] for one asset.
///
/// The statement is a `&'static str` the caller picks from the two above --
/// never anything composed -- which is [`set_column`]'s arrangement applied to
/// the two reads that differ in nothing but their `where` clause.
async fn routes(
    pool: &PgPool,
    statement: &'static str,
    asset_id: &str,
) -> Result<Vec<RouteRow>, IpcError> {
    sqlx::query(statement)
        .bind(asset_id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(route_row_of)
        .collect()
}

fn route_row_of(row: &sqlx::postgres::PgRow) -> Result<RouteRow, IpcError> {
    let stored: serde_json::Value = row.try_get("properties")?;
    Ok(RouteRow {
        id: row.try_get("id")?,
        asset_id: row.try_get("asset_id")?,
        asset_name: row.try_get("asset_name")?,
        target_id: row.try_get("target_id")?,
        target_name: row.try_get("target_name")?,
        name: row.try_get("name")?,
        url: row.try_get("url")?,
        visibility: Visibility::parse(&row.try_get::<String, _>("visibility")?)?,
        properties: custom_properties(&stored),
    })
}

/// The pane's property list: declared keys in the type's order, then whatever
/// else the bag holds, by key.
///
/// Pure, and separately tested, because it is the one rule in this module a
/// reader can get wrong without any database saying so.
#[must_use]
pub fn properties_of(type_id: &str, stored: &serde_json::Value) -> Vec<AssetProperty> {
    let bag = stored.as_object();
    let declared = asset::find(type_id);
    let mut out = Vec::new();
    let mut claimed: Vec<&str> = Vec::new();

    if let Some(declared) = declared {
        for property in declared.properties {
            claimed.push(property.key);
            out.push(AssetProperty {
                key: property.key.to_owned(),
                label: property.label.to_owned(),
                value: bag
                    .and_then(|bag| bag.get(property.key))
                    .and_then(|raw| serde_json::from_value(raw.clone()).ok()),
                custom: false,
            });
        }
    }

    out.extend(unclaimed(stored, &claimed));
    out
}

/// One asset in a room's Assets tile: the row every other read answers with,
/// plus where it sits (#434).
///
/// **A carrier rather than a field on [`AssetRow`]**, and the reason is the
/// rule the three column statements above state: `path_text` is left out of a
/// Miller walk because no column draws it, and putting it on the row would put
/// it on every column of every walk to serve one tile. The pair here is the
/// same move [`Written`] makes -- the thing, and the one extra fact this
/// caller needs about it -- so the tile reads `asset.name` and `path` and
/// nothing in this module keeps a second copy of a type table or a rollup.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MemberAsset {
    pub asset: AssetRow,
    /// The ancestors' names, outermost first, `" / "` between them; `null` for
    /// an asset at the top of the estate.
    ///
    /// Read off `knobas.asset.path_text`, the column the store maintains on
    /// create, rename and move -- not a recursive read per row, which is what
    /// a tile of twenty assets would otherwise cost.
    pub path: Option<String>,
}

/// The member assets themselves, by id.
///
/// Which ids those are is [`knobas_core::context::member_ids`]'s answer and is
/// **not re-derived here** -- see [`in_context`]. The `order by` is a stable
/// floor under the sort that follows it, not the order the tile draws.
const MEMBER_ASSETS: &str = "select a.id, a.parent_id, a.type_id, a.name, a.status, a.environment,
            a.owner, nullif(a.path_text, '') as path,
            exists (select 1 from knobas.asset c where c.parent_id = a.id) as has_children
       from knobas.asset a
      where a.id = any($1::text[])
      order by a.name asc, a.id asc";

/// The assets a **source**'s monitors are attached to (spec #427 story 44).
///
/// [`MEMBER_ASSETS`]' columns over a different `where`, and a statement of its
/// own for the reason all five are: a whole literal a reader checks by
/// reading.
/// `$1` is the source id, `$2` the relation ([`MONITORED_BY`]) and `$3` the
/// kind ([`MONITOR_KIND`]) -- bound rather than written into the text, so each
/// word has one spelling in this crate.
///
/// An `exists` rather than a join: an asset carrying two monitors is one row
/// of a tile, and a join would draw it twice.
const MONITORED_ASSETS: &str =
    "select a.id, a.parent_id, a.type_id, a.name, a.status, a.environment,
            a.owner, nullif(a.path_text, '') as path,
            exists (select 1 from knobas.asset c where c.parent_id = a.id) as has_children
       from knobas.asset a
      where exists (
            select 1 from knobas.confirmed_link l
              join sync.live_item i
                on i.entity_id = case when l.from_id = a.id then l.to_id else l.from_id end
             where (l.from_id = a.id or l.to_id = a.id)
               and l.relation = $2
               and i.source_id = $1
               and i.kind = $3
      )
      order by a.name asc, a.id asc";

/// The assets a stored room's Assets tile draws: the context's member assets,
/// worst health first (spec #427 story 41, issue #434).
///
/// **One walk, not two.** The membership rule is
/// [`knobas_core::context::member_ids`]'s and this read asks it for the
/// answer, then reads the assets among the ids it gave back. Nothing here
/// re-states seed / direct / hop / held: a second copy of that statement is
/// exactly what would let the tile, the per-context inbox filter and the
/// tray's proposal scope come to different answers about who is here, and
/// ADR-0008's "one rule, one statement, one battery" is the line this keeps.
/// Assets reach that answer through their ancestors, so an asset's whole
/// subtree is in the list when the asset is.
///
/// **Worst first** (the mockup's `signal-miller.html:1832`, `rank`): a tile is
/// twenty rows in a box eight rows tall, and a `down` asset fifteenth is a
/// tile that has not said the one thing it exists to say. The order is
/// `health` -- the *effective* health, so a healthy VM holding a dead
/// container sorts up with the dead one -- then name, then id, so two assets
/// that are equally well and equally named still draw in a fixed order.
///
/// **No `limit`, deliberately**, where `Tile.svelte`'s list tiles window at
/// fifty and print the unpaged total beside them. There is no total to print
/// here: `member_ids` answers with a whole set and counting it is reading it,
/// so a windowed read would either say "50" and be wrong or cost a second
/// statement to say otherwise. ADR-0008 grants that *"membership can be
/// wide"*, and the honest shape for a wide answer in a box eight rows tall is
/// the whole list, worst first, in a tile that scrolls.
///
/// An unknown or archived `ctx_id` answers with no assets rather than an
/// error, which is [`knobas_core::context::member_ids`]' own behaviour and the
/// honest answer for a reader scoping a view.
///
/// # Errors
///
/// [`IpcError`] if a read fails.
pub async fn in_context(pool: &PgPool, ctx_id: &str) -> Result<Vec<MemberAsset>, IpcError> {
    let members = knobas_core::context::member_ids(pool, ctx_id).await?;
    if members.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(MEMBER_ASSETS)
        .bind(&members)
        .fetch_all(pool)
        .await?;
    tile_rows(pool, &rows).await
}

/// The assets a **source** room's Assets tile draws: everything this source's
/// monitors are attached to, worst health first (spec #427 story 44, issue
/// #435).
///
/// **A `monitored-by` link to an item of this source, and nothing else.** The
/// rule spec #427 states is *"a source room lists assets with a monitored-by
/// link to that source's monitors"*, and every clause of it is in
/// [`MONITORED_ASSETS`]: the relation, the source, and `kind = 'monitor'` --
/// which is what keeps a Jira room from listing the assets its *tickets*
/// happen to be linked to.
///
/// **Written before there was a monitor to find**, as the real statement
/// rather than an empty `Vec`, on the argument that the difference is not
/// visible from the tile and is the whole difference between a room that fills
/// itself the day the adapter lands and a room somebody has to remember to
/// come back to. #442's adapter landed and it did.
///
/// **Not membership's rule.** A monitor watches the thing it was pointed at;
/// that a VM holds the container somebody is monitoring does not make the VM
/// monitored, so there is no ancestor expansion here and
/// [`knobas_core::context::member_ids`] is not consulted. What rolls up the
/// tree is *health*, which every row carries already.
///
/// # Errors
///
/// [`IpcError`] if the read fails.
pub async fn monitored_by(pool: &PgPool, source_id: &str) -> Result<Vec<MemberAsset>, IpcError> {
    let rows = sqlx::query(MONITORED_ASSETS)
        .bind(source_id)
        .bind(MONITORED_BY)
        .bind(MONITOR_KIND)
        .fetch_all(pool)
        .await?;
    tile_rows(pool, &rows).await
}

/// The rows of a tile's read, hydrated and put in the order a tile draws them.
///
/// One helper for both tile reads rather than a copy each: the *ordering* is
/// the tile's and not one room kind's, and two copies of a three-key sort are
/// two chances for a room to be ordered differently from the room beside it.
///
/// `severity` is smaller-is-worse, so this is an ascending sort and the worst
/// row is first. The statement cannot do it: `health` is the rollup's answer
/// and the rollup is a read of its own.
async fn tile_rows(
    pool: &PgPool,
    rows: &[sqlx::postgres::PgRow],
) -> Result<Vec<MemberAsset>, IpcError> {
    let assets = rows_of(pool, rows).await?;
    let mut out: Vec<MemberAsset> = rows
        .iter()
        .zip(assets)
        .map(|(row, asset)| {
            Ok(MemberAsset {
                asset,
                path: row.try_get("path")?,
            })
        })
        .collect::<Result<Vec<_>, IpcError>>()?;
    out.sort_by(|left, right| {
        left.asset
            .health
            .severity()
            .cmp(&right.asset.health.severity())
            .then_with(|| left.asset.name.cmp(&right.asset.name))
            .then_with(|| left.asset.id.cmp(&right.asset.id))
    });
    Ok(out)
}

/// The properties of a thing that declares none: every key in the bag, by key.
///
/// A route's whole property list (#432). It is [`properties_of`] with the
/// schema half missing rather than a second walk of the bag, because "custom
/// keys, in key order, labelled by themselves" is one rule and the pane draws
/// both lists with one component.
#[must_use]
pub fn custom_properties(stored: &serde_json::Value) -> Vec<AssetProperty> {
    unclaimed(stored, &[])
}

/// The bag's keys that no schema claimed, in key order.
fn unclaimed(stored: &serde_json::Value, claimed: &[&str]) -> Vec<AssetProperty> {
    let Some(bag) = stored.as_object() else {
        return Vec::new();
    };
    // `serde_json::Map` iterates in key order under the default feature set,
    // and the sort below does not depend on that: a custom list whose order
    // moved between reads would make the pane flicker.
    let mut custom: Vec<(&String, &serde_json::Value)> = bag
        .iter()
        .filter(|(key, _)| !claimed.contains(&key.as_str()))
        .collect();
    custom.sort_by(|left, right| left.0.cmp(right.0));
    custom
        .into_iter()
        .map(|(key, raw)| AssetProperty {
            key: key.clone(),
            label: key.clone(),
            value: serde_json::from_value(raw.clone()).ok(),
            custom: true,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// What a mutation wrote, and the line that says so.
///
/// The line is handed back rather than only written, for `commands::assets`'
/// benefit: the shim announces it on `activity:new`, the same signal every
/// other mutation in this app is announced on, so the status bar's
/// latest-change line hears about an estate edit without a channel of its own.
#[derive(Debug)]
pub struct Written<T> {
    pub value: T,
    pub activity: Vec<ActivityRow>,
}

/// Create an asset under `parent_id`, or at the top of the estate.
///
/// The id is minted here and nobody types it (story 18): creating is not
/// naming. An import keeps the estate file's id instead, which is #439's, and
/// is why this is `create` rather than `create_with_id`.
///
/// # Errors
///
/// [`IpcError::invalid`] for a blank name, a type no built-in table declares,
/// or a property whose value does not fit its declared kind;
/// [`IpcError::not_found`] for a parent that is not there.
pub async fn create(
    pool: &PgPool,
    parent_id: Option<&str>,
    type_id: &str,
    name: &str,
    properties: &[(String, PropertyValue)],
) -> Result<Written<AssetRow>, IpcError> {
    let declared = vet_type(type_id)?;
    let name = vet_name(name)?;
    let bag = vet_properties(declared, properties)?;

    let id = EntityRef::new(NAMESPACE, &Uuid::new_v4().to_string()).to_string();
    let mut tx = pool.begin().await?;

    let path_text = match parent_id {
        None => String::new(),
        Some(parent) => path_below(&mut tx, parent).await?,
    };

    insert_asset(
        &mut tx,
        AssetRowInsert {
            id: &id,
            parent_id,
            type_id,
            name: &name,
            properties: &bag,
            // A hand-created asset sets neither: it inherits the environment
            // and the owner in force above it (#431), and it watches nothing
            // until a monitor is attached to it. Only the import (#439) has
            // values for the three, because only a file states them.
            environment: None,
            owner: None,
            monitors: &[],
            path_text: &path_text,
        },
    )
    .await?;

    let entity = entity_of(&id)?;
    let line = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        "created",
        Some(&entity),
        serde_json::json!({ "asset": { "type": type_id, "name": name, "parent": parent_id } }),
    )
    .await?;
    tx.commit().await?;

    let value = one(pool, &id).await?;
    Ok(Written {
        value,
        activity: vec![line],
    })
}

/// Apply `edits`, one history line each.
///
/// The edits are applied **in order and in one transaction**: a list that sets
/// two properties either sets both or sets neither, and the history reads in
/// the order the reader made them. An edit that changes nothing writes no line
/// -- a history of "set the IP to what it already was" is noise, and story 11
/// is about what changed.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no asset carries; [`IpcError::invalid`]
/// for a blank name or a property value that does not fit its declared kind.
pub async fn edit(
    pool: &PgPool,
    id: &str,
    edits: &[AssetEdit],
) -> Result<Written<AssetRow>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked(&mut tx, id).await?;
    let declared = asset::find(&current.type_id);
    let mut lines = Vec::new();
    let mut renamed = false;

    for change in edits {
        let line = match change {
            AssetEdit::Name { value } => {
                let name = vet_name(value)?;
                if name == current.name {
                    continue;
                }
                sqlx::query("update knobas.asset set name = $2, updated_at = now() where id = $1")
                    .bind(id)
                    .bind(&name)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "update knobas.entity set title = $2, updated_at = now() where id = $1",
                )
                .bind(id)
                .bind(&name)
                .execute(&mut *tx)
                .await?;
                renamed = true;
                Some((
                    "renamed",
                    serde_json::json!({ "field": "name", "from": current.name, "to": name }),
                ))
            }
            AssetEdit::Status { value } => {
                if *value == current.status {
                    continue;
                }
                set_column(&mut tx, SET_STATUS, id, Some(value.as_str())).await?;
                Some((
                    "edited",
                    serde_json::json!({
                        "field": "status",
                        "from": current.status.as_str(),
                        "to": value.as_str(),
                    }),
                ))
            }
            AssetEdit::Environment { value } => {
                if *value == current.environment {
                    continue;
                }
                set_column(&mut tx, SET_ENVIRONMENT, id, value.map(Environment::as_str)).await?;
                Some((
                    "edited",
                    serde_json::json!({
                        "field": "environment",
                        "from": current.environment.map(Environment::as_str),
                        "to": value.map(Environment::as_str),
                    }),
                ))
            }
            AssetEdit::Owner { value } => {
                let value = value.as_deref().map(str::trim).filter(|v| !v.is_empty());
                if value == current.owner.as_deref() {
                    continue;
                }
                set_column(&mut tx, SET_OWNER, id, value).await?;
                Some((
                    "edited",
                    serde_json::json!({
                        "field": "owner", "from": current.owner, "to": value,
                    }),
                ))
            }
            AssetEdit::Property { key, value } => {
                let key = key.trim();
                if key.is_empty() {
                    return Err(IpcError::invalid("a property needs a key"));
                }
                if let Some(value) = value {
                    value.vet(key)?;
                    vet_against_schema(declared, key, value)?;
                }
                let before = current.properties.get(key).cloned();
                let after = value.as_ref().map(stored_value).transpose()?;
                if before == after {
                    continue;
                }
                match &after {
                    Some(after) => {
                        sqlx::query(
                            "update knobas.asset
                                set properties = properties || jsonb_build_object($2::text, $3::jsonb),
                                    updated_at = now()
                              where id = $1",
                        )
                        .bind(id)
                        .bind(key)
                        .bind(after)
                        .execute(&mut *tx)
                        .await?;
                    }
                    None => {
                        sqlx::query(
                            "update knobas.asset set properties = properties - $2::text,
                                    updated_at = now()
                              where id = $1",
                        )
                        .bind(id)
                        .bind(key)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                Some((
                    "edited",
                    serde_json::json!({ "field": "property", "key": key, "from": before, "to": after }),
                ))
            }
        };

        if let Some((verb, detail)) = line {
            let entity = entity_of(id)?;
            lines.push(
                knobas_core::activity::record_with(&mut *tx, ACTOR, verb, Some(&entity), detail)
                    .await?,
            );
        }
    }

    // A rename changes what every descendant's `path_text` says, and that
    // column is what the launcher matches ancestor names against. Recomputing
    // the subtree here rather than in `move_to` alone is what keeps the two
    // from disagreeing.
    if renamed {
        recompute_paths(&mut tx, id).await?;
    }
    tx.commit().await?;

    Ok(Written {
        value: one(pool, id).await?,
        activity: lines,
    })
}

/// Move an asset under `new_parent_id`, or to the top of the estate.
///
/// **A move that would make a cycle is refused by name.** The proposed
/// parent's ancestors are walked first, and the refusal says which asset the
/// move would have run into -- "under itself" is not a sentence a reader can
/// act on when the loop is four levels long.
///
/// # Errors
///
/// [`IpcError::not_found`] for an asset or a parent that is not there;
/// [`IpcError::invalid`] for a move that would make a cycle.
pub async fn move_to(
    pool: &PgPool,
    id: &str,
    new_parent_id: Option<&str>,
) -> Result<Written<AssetRow>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked(&mut tx, id).await?;

    if let Some(parent) = new_parent_id {
        let parent_row = locked(&mut tx, parent).await.map_err(|error| {
            if error.code == crate::IpcErrorCode::NotFound {
                IpcError::not_found(format!("no asset {parent} to move {id} under"))
            } else {
                error
            }
        })?;
        if parent == id {
            return Err(IpcError::invalid(format!(
                "{:?} cannot hold itself",
                current.name
            )));
        }
        if let Some(offender) = cycle_through(&mut tx, id, parent).await? {
            return Err(IpcError::invalid(format!(
                "moving {:?} under {:?} would make a cycle: {:?} is already held by {:?}",
                current.name, parent_row.name, offender, current.name
            )));
        }
    }

    if current.parent_id.as_deref() == new_parent_id {
        tx.commit().await?;
        return Ok(Written {
            value: one(pool, id).await?,
            activity: Vec::new(),
        });
    }

    sqlx::query("update knobas.asset set parent_id = $2, updated_at = now() where id = $1")
        .bind(id)
        .bind(new_parent_id)
        .execute(&mut *tx)
        .await?;
    recompute_paths(&mut tx, id).await?;

    let entity = entity_of(id)?;
    let line = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        "moved",
        Some(&entity),
        serde_json::json!({ "field": "parent", "from": current.parent_id, "to": new_parent_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Written {
        value: one(pool, id).await?,
        activity: vec![line],
    })
}

/// Delete a **leaf** that exposes nothing.
///
/// An asset that holds anything is refused with a `conflict` naming what it
/// holds: deleting a subtree by deleting its root is the one destructive
/// action nobody asks for twice, and `0017` leaves the foreign key with no
/// cascade as the floor under this refusal. An asset that still **exposes
/// routes** is refused the same way and for the same reason, with `0018`'s
/// own cascade-less foreign key underneath it -- a route whose exposing asset
/// is gone is not a row this model can hold, and the pane's *Exposes* list is
/// where the reader sees what to delete first.
///
/// What is **not** refused is an asset some route **lands on**. That route is
/// somebody else's row, the model says an endpoint landing on nothing knobas
/// knows is still a route, and refusing here would make a container
/// undeletable because a proxy three branches away points at it. Those targets
/// are cleared in this transaction, each with a history line of its own on the
/// route -- `0018`'s `on delete set null` is the floor under that rather than
/// the road to it, because a constraint cannot write the line.
///
/// The `knobas.asset` row goes and the **entity row is tombstoned**, the
/// treatment `knobas_core::note::delete` gives a note: links drawn to this
/// asset stay visible and marked rather than dangling, and the launcher stops
/// finding it because `knobas_search::corpus::ASSET` reads the asset table.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no asset carries; [`IpcError::conflict`]
/// for an asset that still holds something.
pub async fn delete(pool: &PgPool, id: &str) -> Result<Written<()>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked(&mut tx, id).await?;

    let held: i64 = sqlx::query("select count(*) as n from knobas.asset where parent_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?
        .try_get("n")?;
    if held > 0 {
        return Err(IpcError::conflict(format!(
            "{:?} still holds {held} asset(s) -- move or delete them first",
            current.name
        )));
    }

    let exposed: i64 = sqlx::query("select count(*) as n from knobas.route where asset_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?
        .try_get("n")?;
    if exposed > 0 {
        return Err(IpcError::conflict(format!(
            "{:?} still exposes {exposed} route(s) -- delete them first",
            current.name
        )));
    }

    let mut lines = Vec::new();
    // The routes that land on it lose their target rather than blocking the
    // delete, and each one says so in its own history.
    let orphaned = sqlx::query(
        "update knobas.route set target_id = null, updated_at = now()
          where target_id = $1 returning id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for row in &orphaned {
        let route = entity_of(&row.try_get::<String, _>("id")?)?;
        lines.push(
            knobas_core::activity::record_with(
                &mut *tx,
                ACTOR,
                "edited",
                Some(&route),
                serde_json::json!({ "field": "target", "from": id, "to": null }),
            )
            .await?,
        );
    }

    sqlx::query("delete from knobas.asset where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update knobas.entity set deleted_at = now(), updated_at = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let entity = entity_of(id)?;
    lines.push(
        knobas_core::activity::record_with(
            &mut *tx,
            ACTOR,
            "deleted",
            Some(&entity),
            serde_json::json!({ "asset": { "name": current.name, "type": current.type_id } }),
        )
        .await?,
    );
    tx.commit().await?;

    Ok(Written {
        value: (),
        activity: lines,
    })
}

// ---------------------------------------------------------------------------
// Routes: writing (#432)
// ---------------------------------------------------------------------------

/// Expose a route on `asset_id`.
///
/// The id is minted here, [`create`]'s rule for [`create`]'s reason: creating
/// is not naming, and an import keeps the estate file's id instead (#439).
///
/// **A route may land on the asset that exposes it.** A reverse proxy's own
/// dashboard is exposed by the proxy and reached at the proxy, and the estate
/// this model answers to has one; so there is no self-target refusal, and the
/// pane draws that route under both *Exposes* and *Reachable via*, which is
/// what it is.
///
/// # Errors
///
/// [`IpcError::invalid`] for a blank name, a URL with no scheme, or a property
/// value nothing could read back; [`IpcError::not_found`] for an exposing
/// asset or a target that is not there.
pub async fn create_route(
    pool: &PgPool,
    asset_id: &str,
    name: &str,
    url: &str,
    target_id: Option<&str>,
    visibility: Visibility,
    properties: &[(String, PropertyValue)],
) -> Result<Written<RouteRow>, IpcError> {
    let name = vet_name_of("a route", name)?;
    let url = vet_url(url)?;
    let bag = vet_custom_properties(properties)?;

    let id = EntityRef::new(ROUTE_NAMESPACE, &Uuid::new_v4().to_string()).to_string();
    let mut tx = pool.begin().await?;

    // Normalised the way `RouteEdit::Target` normalises it: a blank string is
    // the wire's other spelling of "no target", and the two commands reading
    // one value two ways -- a clear on the edit, `no asset  for a route to
    // land on` on the create -- is the kind of disagreement nobody finds
    // until a dialog sends an empty field.
    let target_id = target_id.map(str::trim).filter(|value| !value.is_empty());

    must_exist(&mut tx, asset_id, "to expose a route on").await?;
    if let Some(target) = target_id {
        must_exist(&mut tx, target, "for a route to land on").await?;
    }

    insert_route(
        &mut tx,
        RouteRowInsert {
            id: &id,
            asset_id,
            target_id,
            name: &name,
            url: &url,
            visibility,
            properties: &bag,
        },
    )
    .await?;

    let entity = entity_of(&id)?;
    let line = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        "created",
        Some(&entity),
        serde_json::json!({
            "route": { "name": name, "url": url, "asset": asset_id, "target": target_id },
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Written {
        value: one_route(pool, &id).await?,
        activity: vec![line],
    })
}

/// Apply `edits` to a route, one history line each.
///
/// [`edit`]'s rules, applied to the other entity this module writes: in order,
/// in one transaction, and an edit that changes nothing writes no line.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no route carries or a target that is not
/// there; [`IpcError::invalid`] for a blank name, a URL with no scheme or a
/// property value nothing could read back.
pub async fn edit_route(
    pool: &PgPool,
    id: &str,
    edits: &[RouteEdit],
) -> Result<Written<RouteRow>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked_route(&mut tx, id).await?;
    let mut lines = Vec::new();

    for change in edits {
        let line = match change {
            RouteEdit::Name { value } => {
                let name = vet_name_of("a route", value)?;
                if name == current.name {
                    continue;
                }
                set_route_column(&mut tx, SET_ROUTE_NAME, id, Some(&name)).await?;
                sqlx::query(
                    "update knobas.entity set title = $2, updated_at = now() where id = $1",
                )
                .bind(id)
                .bind(&name)
                .execute(&mut *tx)
                .await?;
                Some((
                    "renamed",
                    serde_json::json!({ "field": "name", "from": current.name, "to": name }),
                ))
            }
            RouteEdit::Url { value } => {
                let url = vet_url(value)?;
                if url == current.url {
                    continue;
                }
                set_route_column(&mut tx, SET_ROUTE_URL, id, Some(&url)).await?;
                Some((
                    "edited",
                    serde_json::json!({ "field": "url", "from": current.url, "to": url }),
                ))
            }
            RouteEdit::Target { value } => {
                let target = value.as_deref().map(str::trim).filter(|v| !v.is_empty());
                if target == current.target_id.as_deref() {
                    continue;
                }
                if let Some(target) = target {
                    must_exist(&mut tx, target, "for a route to land on").await?;
                }
                set_route_column(&mut tx, SET_ROUTE_TARGET, id, target).await?;
                Some((
                    "edited",
                    serde_json::json!({
                        "field": "target", "from": current.target_id, "to": target,
                    }),
                ))
            }
            RouteEdit::Visibility { value } => {
                if *value == current.visibility {
                    continue;
                }
                set_route_column(&mut tx, SET_ROUTE_VISIBILITY, id, Some(value.as_str())).await?;
                Some((
                    "edited",
                    serde_json::json!({
                        "field": "visibility",
                        "from": current.visibility.as_str(),
                        "to": value.as_str(),
                    }),
                ))
            }
            RouteEdit::Property { key, value } => {
                let key = key.trim();
                if key.is_empty() {
                    return Err(IpcError::invalid("a property needs a key"));
                }
                if let Some(value) = value {
                    value.vet(key)?;
                }
                let before = current.properties.get(key).cloned();
                let after = value.as_ref().map(stored_value).transpose()?;
                if before == after {
                    continue;
                }
                match &after {
                    Some(after) => {
                        sqlx::query(
                            "update knobas.route
                                set properties = properties || jsonb_build_object($2::text, $3::jsonb),
                                    updated_at = now()
                              where id = $1",
                        )
                        .bind(id)
                        .bind(key)
                        .bind(after)
                        .execute(&mut *tx)
                        .await?;
                    }
                    None => {
                        sqlx::query(
                            "update knobas.route set properties = properties - $2::text,
                                    updated_at = now()
                              where id = $1",
                        )
                        .bind(id)
                        .bind(key)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
                Some((
                    "edited",
                    serde_json::json!({ "field": "property", "key": key, "from": before, "to": after }),
                ))
            }
        };

        if let Some((verb, detail)) = line {
            let entity = entity_of(id)?;
            lines.push(
                knobas_core::activity::record_with(&mut *tx, ACTOR, verb, Some(&entity), detail)
                    .await?,
            );
        }
    }
    tx.commit().await?;

    Ok(Written {
        value: one_route(pool, id).await?,
        activity: lines,
    })
}

/// Delete a route.
///
/// No `conflict` to raise: nothing hangs off a route, which is what makes it
/// the one thing in the estate that deletes without a question. The
/// `knobas.route` row goes and the **entity row is tombstoned**, the treatment
/// [`delete`] gives an asset and `knobas_core::note::delete` gives a note, so
/// links drawn to the route stay visible and marked rather than dangling.
///
/// # Errors
///
/// [`IpcError::not_found`] for an id no route carries.
pub async fn delete_route(pool: &PgPool, id: &str) -> Result<Written<()>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked_route(&mut tx, id).await?;

    sqlx::query("delete from knobas.route where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update knobas.entity set deleted_at = now(), updated_at = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let entity = entity_of(id)?;
    let line = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        "deleted",
        Some(&entity),
        serde_json::json!({ "route": { "name": current.name, "url": current.url } }),
    )
    .await?;
    tx.commit().await?;

    Ok(Written {
        value: (),
        activity: vec![line],
    })
}

// ---------------------------------------------------------------------------
// The Import (#439)
// ---------------------------------------------------------------------------

/// The actor every line the Import writes carries.
///
/// **Not [`ACTOR`], and that is the whole merge rule.** Spec #427 settles what
/// survives a second import in one sentence: the apply "sets properties the
/// file names on assets whose value was never edited by hand (*a hand edit is
/// any activity line by the user on that property*)". So the question the
/// merge asks is *who wrote this line*, and it is only answerable if the
/// import's own lines are not the person's. `knobas.activity.actor` is open
/// text -- `0001` documents `user` and `sync:<source_id>` and constrains
/// neither -- and this is the fourth spelling in it, beside `knobas`, which is
/// the app acting on its own behalf rather than on a file's.
///
/// One consequence worth stating: a property this import set is claimed by
/// nobody, so a later import of a file that changed it changes it again. That
/// is the point -- the file stays authoritative over everything no person has
/// touched.
const ACTOR_IMPORT: &str = "import";

/// The only format version there has ever been.
///
/// A file claiming another number is refused rather than read on the
/// assumption that a bigger version is a superset of this one: the fields this
/// module reads are the fields `deny_unknown_fields` insists on, so a version 2
/// that moved one would import a silently emptier estate.
const FILE_VERSION: i64 = 1;

/// The `properties` key the file's `description` lands in.
///
/// An asset has no description column: `0017` gives it a name, a type, a
/// status, an environment, an owner and a bag. The estate file gives most of
/// its entries a sentence saying what the thing is *for*, which is the text a
/// reader coming to a strange asset most wants -- so it goes in the bag, as an
/// ordinary custom text property, drawn and editable in the pane like every
/// other one. No built-in type declares the key, so it never collides with a
/// schema; an entry that spells it **both** ways at once is refused rather
/// than resolved, because there is no honest answer to which of the two the
/// author meant.
const DESCRIPTION_KEY: &str = "description";

/// The estate file, as the Import reads it.
///
/// `deny_unknown_fields` on all three shapes, and it is the decision
/// `knobas-core`'s `tests/estate_file.rs` made when it closed the file's key
/// vocabulary: *"a misspelled key reads as an absent optional field, and an
/// absent optional field is legal everywhere it appears"*. A `parnet` would
/// hang the asset off the root, a `targets` would land the route on nothing
/// and a `monitor` would drop the name -- all silently, and all in a file
/// somebody wrote by hand. `ASSET_KEYS` and `ROUTE_KEYS` over there are these
/// field lists, and
/// [`tests::the_file_shapes_read_the_keys_the_estate_files_own_check_allows`]
/// is the pin that keeps the two from drifting.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EstateFile {
    /// [`FILE_VERSION`], or absent.
    #[serde(default)]
    version: Option<i64>,
    /// What the file calls the estate it describes -- the preview's heading,
    /// and the estate every origin line names.
    #[serde(default)]
    name: Option<String>,
    /// The file's prose about itself. Read so that a real estate file parses,
    /// and deliberately not stored: it is a fact about the file, and there is
    /// nothing in the estate it describes to hang it on.
    #[serde(default, rename = "description")]
    _description: Option<String>,
    #[serde(default)]
    assets: Vec<FileAsset>,
    #[serde(default)]
    routes: Vec<FileRoute>,
}

/// One asset entry of the file.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAsset {
    /// Kept as the asset's id (story 22), which is what makes a second import
    /// recognise it.
    id: String,
    #[serde(rename = "type")]
    type_id: String,
    name: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    /// **Plain scalars**, translated into the tagged wire shape by
    /// [`property_of`] -- #428 chose that shape and left this translation to
    /// the import in as many words.
    #[serde(default)]
    properties: serde_json::Map<String, serde_json::Value>,
    /// The Uptime Kuma names of the monitors watching this asset (story 25).
    #[serde(default)]
    monitors: Vec<String>,
}

/// One route entry of the file.
///
/// No `visibility`: the file's key vocabulary has never carried one, and
/// `0018`'s default -- `internal`, the safe reading of a route nobody has
/// classified -- is the honest value for a route whose author did not say.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRoute {
    id: String,
    /// The asset that exposes it.
    asset: String,
    /// The asset it lands on, when it lands on one knobas holds.
    #[serde(default)]
    target: Option<String>,
    name: String,
    url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    properties: serde_json::Map<String, serde_json::Value>,
}

/// One entry of the preview's *already in the tree* or *new* group.
///
/// Assets and routes in one shape and one list per group, because the question
/// a group answers is about the **file** -- what of it does knobas already
/// hold -- and a reader counting what is new counts entries rather than two
/// populations. Which of the two an entry is, is [`kind`](Self::kind).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ImportEntry {
    /// The file's own id, which is the id the asset or route carries.
    pub id: String,
    /// [`NAMESPACE`] or [`ROUTE_NAMESPACE`] -- the word `knobas.entity.kind`
    /// holds, so the preview carries no vocabulary of its own.
    pub kind: String,
    pub name: String,
    /// The type's label for an asset; `null` for a route, which has no type.
    pub type_label: Option<String>,
    /// Where the file puts it: the parent for an asset, the exposing asset for
    /// a route. `null` only for an asset at the top of the estate.
    pub parent_id: Option<String>,
}

/// What an apply would do with one property the file names.
///
/// Two values and no third, because there are two ways a file's value can meet
/// a stored one: nobody has claimed the property, so the file wins, or
/// somebody has, so they do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyPlan {
    /// The stored value is replaced by the file's.
    Set,
    /// The stored value stays, because a person edited this property by hand
    /// and the file never silently undoes that (story 24).
    Kept,
}

/// One property of one asset already in the tree whose file value differs from
/// the stored one.
///
/// Only the ones that **differ**: a preview listing every property of every
/// known asset is a preview nobody reads, and *what would change* is the
/// group's own name. A key the file does not mention is not here either -- the
/// file is authoritative over what it says and silent about the rest, so an
/// import never removes a property.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PropertyChange {
    pub key: String,
    /// The type's label for a declared key, the key itself for a custom one --
    /// [`AssetProperty::label`]'s rule, so the two lists read alike.
    pub label: String,
    /// What the asset carries now; `null` for a key it does not carry yet.
    pub from: Option<PropertyValue>,
    /// What the file says.
    pub to: PropertyValue,
    pub plan: PropertyPlan,
}

/// One asset already in the tree that an apply would change.
///
/// An asset with nothing to change is **not** here -- it is in
/// [`ImportPreview::known`] with the rest -- so this list is the third group
/// the Import dialog draws and its length is the honest answer to *what would
/// this do*.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AssetChange {
    pub id: String,
    /// The name the asset carries **in the tree**, which is what the reader is
    /// looking at. The import does not rename: a name is the one thing about
    /// an asset a person is certain to have chosen deliberately, and a file
    /// re-titling twenty-three of them is not a change anybody asked for by
    /// pressing *Import*.
    pub name: String,
    /// Every property whose file value differs, whether it would be set or
    /// kept. Both, in one list, because *"the file says `cx23` and it stays
    /// `cx33` because you typed that"* is the sentence story 24 is about, and
    /// a list carrying only what changes cannot say it.
    pub properties: Vec<PropertyChange>,
    /// Monitor names the file lists that this asset does not carry yet.
    ///
    /// Additive: an import never removes a name, because a file is one of the
    /// two things that puts one there and the M4.1 sync is the other.
    pub monitors: Vec<String>,
}

/// One `monitored-by` link an apply would draw.
///
/// A row per link rather than a count, because the reader's question is *which
/// of my monitors did it find* -- and because a name that resolves and a name
/// that does not look identical in the file. A name the mirror does not hold
/// is not here: it is kept on the asset (`0020`) and resolved by the next
/// import or by the M4.1 sync, which is spec #427's own sentence.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MonitorLink {
    pub asset_id: String,
    pub asset_name: String,
    /// The Uptime Kuma name, as the file spells it and as the mirror holds it.
    pub monitor_name: String,
    /// The mirrored monitor's entity id -- the other end of the link.
    pub monitor_id: String,
}

/// One monitor name on one asset that answers to nothing in the mirror.
///
/// [`MonitorLink`]'s negative, and a group of its own because the reader's
/// second question about the file's monitor names is *which of them found
/// nothing* -- the names spec #427 keeps on the asset for the next import to
/// resolve.
///
/// **Neither [`ImportPreview::changes`] nor [`ImportPreview::monitor_links`]
/// can answer it.** `changes` lists what an apply would *write*, so a name
/// already on the asset is absent from it by construction; the second preview
/// of an unchanged file has an empty `changes`, an empty `monitor_links` and
/// every unresolved name still unresolved. Reported on every preview, then,
/// and not only on the one that first kept the name.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct UnresolvedMonitor {
    pub asset_id: String,
    pub asset_name: String,
    /// The Uptime Kuma name, as the file spells it. Nothing in the mirror
    /// carries it -- which is a fact about *this moment*: the M4.1 sync or the
    /// next import resolves it the instant Kuma publishes a monitor by that
    /// name.
    pub monitor_name: String,
}

/// What an import would do, before it has done any of it.
///
/// The three groups the dialog draws -- already in the tree, new, and what
/// would change -- plus the monitor links, which are a write and therefore
/// have to be announced by the same read that announces the others, and the
/// names that found no monitor, which are the same announcement's other half.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ImportPreview {
    /// What the file calls the estate.
    pub name: String,
    /// Entries whose id knobas already holds. On a second import of an
    /// unchanged file this is the whole file and every other list is empty.
    pub known: Vec<ImportEntry>,
    /// Entries an apply would create, **in the order it would create them**:
    /// an asset before the assets it holds, then the routes.
    pub new: Vec<ImportEntry>,
    /// The known assets with something to change, and what.
    pub changes: Vec<AssetChange>,
    /// The `monitored-by` links an apply would draw.
    pub monitor_links: Vec<MonitorLink>,
    /// The monitor names the file's assets carry that the mirror does not
    /// hold, by asset and then by name. See [`UnresolvedMonitor`].
    ///
    /// **Over the assets the file names, and no others.** A preview is a
    /// sentence about a file, so a name kept on an asset the file has since
    /// stopped listing is not reported here -- it is on the asset, where the
    /// pane draws it, and the next file that names that asset again picks it
    /// up.
    pub unresolved: Vec<UnresolvedMonitor>,
}

/// What an import did.
///
/// Counted rather than listed: the preview is where a reader looks at the
/// detail, and this is what the summary line says and what the dialog reports
/// when it is over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportOutcome {
    pub assets_created: i64,
    pub routes_created: i64,
    /// Properties the file's value was written to.
    pub properties_set: i64,
    /// Properties left as they were because a hand edit claimed them.
    pub properties_kept: i64,
    /// Monitor names newly kept on an asset.
    pub monitors_kept: i64,
    /// `monitored-by` links drawn.
    pub monitors_linked: i64,
}

/// Everything an apply would write, decided before anything is written.
///
/// **The preview is the plan.** [`apply_import`] reads
/// [`ImportPreview::changes`] and [`ImportPreview::monitor_links`] to know what
/// to write, so a property the preview did not mention cannot be set and a link
/// it did not list cannot be drawn. That is not a note about discipline: it is
/// the only place either instruction exists, which is what makes *"the preview
/// said it would"* structural rather than two rules somebody keeps in step.
///
/// The two lists below are the part a preview entry cannot carry, because a
/// row needs its type, its properties and its owner and an entry is a line in
/// a dialog.
struct Plan {
    preview: ImportPreview,
    assets: Vec<AssetInsert>,
    routes: Vec<RouteInsert>,
}

/// One new asset an apply writes, with the id the file gave it.
struct AssetInsert {
    id: String,
    parent_id: Option<String>,
    type_id: String,
    name: String,
    properties: serde_json::Map<String, serde_json::Value>,
    environment: Option<Environment>,
    owner: Option<String>,
    monitors: Vec<String>,
}

/// One new route an apply writes, with the id the file gave it.
struct RouteInsert {
    id: String,
    asset_id: String,
    target_id: Option<String>,
    name: String,
    url: String,
    properties: serde_json::Map<String, serde_json::Value>,
}

/// What an import of `file` would do, having written nothing.
///
/// **The read runs in a transaction that is rolled back**, which is how
/// *preview writes nothing* is made structural rather than merely asserted:
/// every statement it runs is a `select`, and the one thing that could make
/// that untrue -- a write added here by somebody reaching for the plan's own
/// helpers -- is undone on the way out. `tests/assets_ipc.rs`'s
/// `a_preview_writes_nothing_at_all` counts the tables either side of it.
///
/// # Errors
///
/// [`IpcError::invalid`] for a file that is not JSON, carries a key the format
/// does not define, names a type nobody declares, names a parent or a target
/// that is neither in the file nor in the estate, or whose assets hold each
/// other in a cycle; [`IpcError`] if a read fails.
pub async fn preview_import(pool: &PgPool, file: &str) -> Result<ImportPreview, IpcError> {
    let mut tx = pool.begin().await?;
    let plan = plan(&mut tx, file).await?;
    tx.rollback().await?;
    Ok(plan.preview)
}

/// Apply `file`: create what is new, set what nobody has claimed, keep what
/// somebody has.
///
/// **One transaction**, so a file refused halfway leaves an estate that never
/// heard of it. An import that got as far as the third server and gave up
/// would otherwise leave a reader guessing which half of their infrastructure
/// is in the tree.
///
/// **The plan is computed inside that transaction**, not handed in from the
/// preview: an argument carrying a plan would be a caller's chance to send one
/// the reader never saw, and a plan computed before the transaction opened
/// could be stale by a hand edit. What the dialog showed and what this writes
/// are two runs of one function over one file, which is what makes them agree.
///
/// **Nothing already in the tree is re-parented**, and that is what keeps the
/// uncapped recursive CTEs behind [`recompute_paths`] and [`ROLLUP`] safe. Its
/// name, its environment and its owner are left alone for the neighbouring
/// reason -- see the module docs. The
/// only `parent_id` this writes is on a row it is inserting -- a row with no
/// children yet, whose parent is already there ([`ordered`]) -- so no cycle can
/// reach the table however the file is shaped, and a file whose own assets hold
/// each other is refused by name before the first insert. [`move_to`] remains
/// the only writer that can put an existing asset under an existing asset, and
/// it walks the ancestors first.
///
/// **Only the summary line is announced.** Every created asset gets its own
/// origin line and every property set gets its own edited line -- story 11
/// applies to an import like any other mutation -- but the first import of the
/// checked-in estate writes thirty-three of them, and the status bar's
/// latest-change line wants *one* sentence about what just happened. The lines
/// are all in the stream and all in each entity's own history; what
/// [`Written::activity`] carries is the one the shell announces.
///
/// # Errors
///
/// [`preview_import`]'s, plus [`IpcError`] if a write fails.
pub async fn apply_import(pool: &PgPool, file: &str) -> Result<Written<ImportOutcome>, IpcError> {
    let mut tx = pool.begin().await?;
    let Plan {
        preview,
        assets,
        routes,
    } = plan(&mut tx, file).await?;
    let from = preview.name.clone();
    let mut outcome = ImportOutcome::default();

    // Assets first and in the plan's order, which is a parent before what it
    // holds: `parent_id` is a foreign key into this same table, and
    // `path_below` reads the parent's own path to build the child's.
    for insert in &assets {
        let path_text = match &insert.parent_id {
            None => String::new(),
            Some(parent) => path_below(&mut tx, parent).await?,
        };
        insert_asset(
            &mut tx,
            AssetRowInsert {
                id: &insert.id,
                parent_id: insert.parent_id.as_deref(),
                type_id: &insert.type_id,
                name: &insert.name,
                properties: &insert.properties,
                environment: insert.environment,
                owner: insert.owner.as_deref(),
                monitors: &insert.monitors,
                path_text: &path_text,
            },
        )
        .await?;
        outcome.assets_created += 1;
        outcome.monitors_kept += count(insert.monitors.len());
        origin_line(&mut tx, &insert.id, &from).await?;
    }

    for insert in &routes {
        insert_route(
            &mut tx,
            RouteRowInsert {
                id: &insert.id,
                asset_id: &insert.asset_id,
                target_id: insert.target_id.as_deref(),
                name: &insert.name,
                url: &insert.url,
                visibility: Visibility::default(),
                properties: &insert.properties,
            },
        )
        .await?;
        outcome.routes_created += 1;
        origin_line(&mut tx, &insert.id, &from).await?;
    }

    for change in &preview.changes {
        let entity = entity_of(&change.id)?;
        for property in &change.properties {
            if property.plan == PropertyPlan::Kept {
                outcome.properties_kept += 1;
                continue;
            }
            sqlx::query(SET_PROPERTY)
                .bind(&change.id)
                .bind(&property.key)
                .bind(stored_value(&property.to)?)
                .execute(&mut *tx)
                .await?;
            knobas_core::activity::record_with(
                &mut *tx,
                ACTOR_IMPORT,
                "edited",
                Some(&entity),
                serde_json::json!({
                    "field": "property", "key": property.key,
                    "from": property.from, "to": property.to, "estate": from,
                }),
            )
            .await?;
            outcome.properties_set += 1;
        }
        if !change.monitors.is_empty() {
            sqlx::query(ADD_MONITORS)
                .bind(&change.id)
                .bind(&change.monitors)
                .execute(&mut *tx)
                .await?;
            outcome.monitors_kept += count(change.monitors.len());
            knobas_core::activity::record_with(
                &mut *tx,
                ACTOR_IMPORT,
                "edited",
                Some(&entity),
                serde_json::json!({
                    "field": "monitors", "added": change.monitors, "estate": from,
                }),
            )
            .await?;
        }
    }

    for link in &preview.monitor_links {
        knobas_core::link::create_with(
            &mut *tx,
            &entity_of(&link.asset_id)?,
            &entity_of(&link.monitor_id)?,
            MONITORED_BY,
            knobas_core::link::Origin::Imported,
            None,
            ACTOR_IMPORT,
        )
        .await?;
        outcome.monitors_linked += 1;
    }

    // The per-run summary (story 23's other half): one line, no entity, and
    // the counts the dialog reports. Written even when every count is zero --
    // *"I imported that file again and it changed nothing"* is a fact about the
    // estate, and a log that only records the imports that did something cannot
    // answer when the last one ran.
    let summary = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR_IMPORT,
        "imported",
        None,
        serde_json::json!({ "estate": from, "outcome": outcome }),
    )
    .await?;
    tx.commit().await?;

    Ok(Written {
        value: outcome,
        activity: vec![summary],
    })
}

/// A length as the outcome counts it.
///
/// `i64` because that is what every other count on this bridge is, and
/// saturating because the alternative is an `unwrap` on a conversion that
/// cannot fail for any file a person could write.
fn count(len: usize) -> i64 {
    i64::try_from(len).unwrap_or(i64::MAX)
}

/// The origin line one created asset or route carries (story 23).
///
/// `"imported"` **with** the entity, against the summary's `"imported"` with
/// none: the two are told apart by whether they name something, which is the
/// distinction a reader makes anyway -- a line in an asset's own history is
/// about that asset, and the line in the stream with no entity is about the
/// run.
async fn origin_line(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    from: &str,
) -> Result<(), IpcError> {
    let entity = entity_of(id)?;
    knobas_core::activity::record_with(
        &mut **tx,
        ACTOR_IMPORT,
        "imported",
        Some(&entity),
        serde_json::json!({ "estate": from }),
    )
    .await?;
    Ok(())
}

/// Set one key of an asset's property bag -- the statement [`edit`] runs for a
/// hand edit, run here for a file's value.
const SET_PROPERTY: &str = "update knobas.asset
    set properties = properties || jsonb_build_object($2::text, $3::jsonb),
        updated_at = now()
  where id = $1";

/// Add monitor names to an asset, keeping the ones it has.
///
/// An append rather than an assignment: an import never removes a monitor
/// name. What it appends is what the plan found missing, so the `array_cat`
/// cannot introduce a duplicate.
const ADD_MONITORS: &str = "update knobas.asset
    set monitors = array_cat(monitors, $2::text[]), updated_at = now()
  where id = $1";

/// The assets the file names that the estate already holds, with what they
/// carry.
const KNOWN_ASSETS: &str =
    "select id, name, properties, monitors from knobas.asset where id = any($1::text[])";

/// The routes the file names that the estate already holds.
const KNOWN_ROUTES: &str = "select id from knobas.route where id = any($1::text[])";

/// The assets the file *refers to* without describing, that the estate holds.
///
/// A file may hang a new subtree under something a person created by hand, and
/// a route may land on it. Those ids are not in the file's own `assets` list,
/// so [`KNOWN_ASSETS`] never reads them -- this is the narrow second read that
/// tells *a parent elsewhere in the estate* from *a parent that is nowhere at
/// all*.
const REFERENCED_ASSETS: &str = "select id from knobas.asset where id = any($1::text[])";

/// The properties **a person** has edited on the assets the file names.
///
/// Spec #427's rule, as one statement: *"a hand edit is any activity line by
/// the user on that property"*. Not the newest line, not a line since the last
/// import -- any line, ever, by `user`. The consequence is deliberate and worth
/// stating: a reader who edits a property and then types the file's own value
/// back has still claimed it, and no import moves it again. The alternative is
/// a stored copy of what the last import wrote, which is a second writer of
/// every property and a merge base to keep in step with the properties
/// themselves.
///
/// `detail ? 'key'` is what narrows it to the property edits: [`edit`] writes
/// `{"field":"property","key":…}` and every other line it writes carries no
/// `key` at all.
const HAND_EDITED: &str = "select distinct entity_id, detail->>'key' as key
  from knobas.activity
 where actor = $2
   and entity_id = any($1::text[])
   and detail->>'field' = 'property'
   and detail ? 'key'";

/// The mirrored monitors whose name the file uses.
///
/// Through `sync.live_item` rather than `sync.item`, so a monitor of a disabled
/// source or a tombstoned one draws no link -- the filter every other reader in
/// this app inherits (`0012`).
///
/// What the import does with a name that resolves to nothing is keep it
/// (`0020`) and report it ([`UnresolvedMonitor`]), which is spec #427's own
/// sentence and issue #445's.
const LIVE_MONITORS: &str =
    "select entity_id, title from sync.live_item where kind = $2 and title = any($1::text[])";

/// The `monitored-by` links the assets the file names already take part in.
///
/// Both directions, because `knobas.link` is unique on the **unordered** pair
/// (`link_pair_active_idx`, `0011`): a link drawn monitor-to-asset by another
/// surface is the same link, and drawing it again would be refused as a
/// duplicate in the middle of a transaction that had already created twenty
/// assets.
const MONITOR_LINKS: &str = "select from_id, to_id from knobas.confirmed_link
 where relation = $2 and (from_id = any($1::text[]) or to_id = any($1::text[]))";

/// One asset the estate already holds, as the plan needs to read it.
struct StoredAsset {
    name: String,
    properties: serde_json::Map<String, serde_json::Value>,
    monitors: Vec<String>,
}

/// Read `file`, decide everything, write nothing.
///
/// Runs against the caller's transaction, so that the plan and whatever is done
/// with it see one snapshot of the estate.
async fn plan(tx: &mut Transaction<'_, Postgres>, file: &str) -> Result<Plan, IpcError> {
    let parsed: EstateFile = serde_json::from_str(file).map_err(|error| {
        IpcError::invalid(format!(
            "this is not an estate file: {error}. An estate file is JSON with an \
             `assets` list and a `routes` list."
        ))
    })?;
    if let Some(version) = parsed.version.filter(|version| *version != FILE_VERSION) {
        return Err(IpcError::invalid(format!(
            "this file says it is version {version} and this build reads \
             version {FILE_VERSION}"
        )));
    }
    let name = parsed
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("an estate file")
        .to_owned();

    // Every id in its own namespace, and no id twice: an id is an address, and
    // a file giving two entries one would create the first and then update it
    // with the second.
    let mut seen: HashSet<&str> = HashSet::new();
    for asset in &parsed.assets {
        vet_file_id(&asset.id, NAMESPACE)?;
        if !seen.insert(&asset.id) {
            return Err(duplicate_file_id(&asset.id));
        }
    }
    for route in &parsed.routes {
        vet_file_id(&route.id, ROUTE_NAMESPACE)?;
        if !seen.insert(&route.id) {
            return Err(duplicate_file_id(&route.id));
        }
    }

    let asset_ids: Vec<String> = parsed.assets.iter().map(|a| a.id.clone()).collect();
    let route_ids: Vec<String> = parsed.routes.iter().map(|r| r.id.clone()).collect();

    let mut stored: HashMap<String, StoredAsset> = HashMap::new();
    for row in sqlx::query(KNOWN_ASSETS)
        .bind(&asset_ids)
        .fetch_all(&mut **tx)
        .await?
    {
        let properties: serde_json::Value = row.try_get("properties")?;
        stored.insert(
            row.try_get("id")?,
            StoredAsset {
                name: row.try_get("name")?,
                properties: match properties {
                    serde_json::Value::Object(map) => map,
                    _ => serde_json::Map::new(),
                },
                monitors: row.try_get("monitors")?,
            },
        );
    }

    let mut known_routes: HashSet<String> = HashSet::new();
    for row in sqlx::query(KNOWN_ROUTES)
        .bind(&route_ids)
        .fetch_all(&mut **tx)
        .await?
    {
        known_routes.insert(row.try_get("id")?);
    }

    let mut hand_edited: HashSet<(String, String)> = HashSet::new();
    for row in sqlx::query(HAND_EDITED)
        .bind(&asset_ids)
        .bind(ACTOR)
        .fetch_all(&mut **tx)
        .await?
    {
        if let (Some(entity), Some(key)) = (
            row.try_get::<Option<String>, _>("entity_id")?,
            row.try_get::<Option<String>, _>("key")?,
        ) {
            hand_edited.insert((entity, key));
        }
    }

    // Every reference the file makes has to resolve -- to something in the
    // file, or to something the estate already holds. Checked before anything
    // below reads a parent, so a dangling one reads as a sentence about the
    // file rather than as a foreign-key violation in Postgres' own words.
    let file_assets: HashSet<&str> = parsed.assets.iter().map(|a| a.id.as_str()).collect();
    let referenced = referenced_assets(tx, &parsed, &file_assets).await?;
    let mut in_estate: HashSet<&str> = stored.keys().map(String::as_str).collect();
    in_estate.extend(referenced.iter().map(String::as_str));
    for asset in &parsed.assets {
        if let Some(parent) = asset.parent.as_deref() {
            resolves(parent, &file_assets, &in_estate, &asset.id, "as its parent")?;
        }
    }
    for route in &parsed.routes {
        resolves(
            &route.asset,
            &file_assets,
            &in_estate,
            &route.id,
            "as the asset exposing it",
        )?;
        if let Some(target) = route.target.as_deref() {
            resolves(
                target,
                &file_assets,
                &in_estate,
                &route.id,
                "as the asset it lands on",
            )?;
        }
    }

    let mut known: Vec<ImportEntry> = Vec::new();
    let mut new: Vec<ImportEntry> = Vec::new();
    let mut changes: Vec<AssetChange> = Vec::new();
    let mut inserts: Vec<AssetInsert> = Vec::new();
    // Every monitor name each asset would carry after this import: what it has
    // plus what the file adds. The link half reads this, so a name kept by an
    // earlier import is resolved by this one -- spec #427's *"resolved by the
    // next import"*.
    let mut wanted: HashMap<String, Vec<String>> = HashMap::new();

    for asset in ordered(&parsed.assets, &in_estate)? {
        let declared = vet_type(&asset.type_id)?;
        let file_name = vet_name(&asset.name)?;
        let properties = bag_of(
            Some(declared),
            &asset.properties,
            asset.description.as_deref(),
        )?;
        let monitors = vet_monitors(&asset.monitors, &asset.id)?;
        let entry = ImportEntry {
            id: asset.id.clone(),
            kind: NAMESPACE.to_owned(),
            name: file_name.clone(),
            type_label: Some(declared.label.to_owned()),
            parent_id: asset.parent.clone(),
        };

        match stored.get(&asset.id) {
            None => {
                new.push(entry);
                wanted.insert(asset.id.clone(), monitors.clone());
                inserts.push(AssetInsert {
                    id: asset.id.clone(),
                    parent_id: asset.parent.clone(),
                    type_id: asset.type_id.clone(),
                    name: file_name,
                    properties,
                    environment: asset
                        .environment
                        .as_deref()
                        .map(|value| vet_environment(value, &asset.id))
                        .transpose()?,
                    owner: asset
                        .owner
                        .as_deref()
                        .map(str::trim)
                        .filter(|owner| !owner.is_empty())
                        .map(str::to_owned),
                    monitors,
                });
            }
            Some(current) => {
                known.push(entry);
                let mut differing: Vec<PropertyChange> = Vec::new();
                for (key, value) in &properties {
                    let before = current.properties.get(key);
                    if before == Some(value) {
                        continue;
                    }
                    differing.push(PropertyChange {
                        key: key.clone(),
                        label: label_of(Some(declared), key),
                        from: before.and_then(|raw| serde_json::from_value(raw.clone()).ok()),
                        to: serde_json::from_value(value.clone()).map_err(IpcError::internal)?,
                        plan: if hand_edited.contains(&(asset.id.clone(), key.clone())) {
                            PropertyPlan::Kept
                        } else {
                            PropertyPlan::Set
                        },
                    });
                }
                let missing: Vec<String> = monitors
                    .iter()
                    .filter(|name| !current.monitors.contains(name))
                    .cloned()
                    .collect();
                let mut all = current.monitors.clone();
                all.extend(missing.iter().cloned());
                wanted.insert(asset.id.clone(), all);
                if !differing.is_empty() || !missing.is_empty() {
                    changes.push(AssetChange {
                        id: asset.id.clone(),
                        name: current.name.clone(),
                        properties: differing,
                        monitors: missing,
                    });
                }
            }
        }
    }

    let mut route_inserts: Vec<RouteInsert> = Vec::new();
    for route in &parsed.routes {
        let file_name = vet_name_of("a route", &route.name)?;
        let url = vet_url(&route.url)?;
        let entry = ImportEntry {
            id: route.id.clone(),
            kind: ROUTE_NAMESPACE.to_owned(),
            name: file_name.clone(),
            type_label: None,
            parent_id: Some(route.asset.clone()),
        };
        if known_routes.contains(&route.id) {
            // A route already in the tree is left exactly as it is. Its own
            // facts are a name, a URL and a target, all three of them things a
            // person may have corrected in the pane, and the model has no way
            // to tell a correction from a stale file -- which is the question
            // story 24 answers for an asset's *properties* and for nothing
            // else. So a known route is known, and re-exposing one elsewhere
            // stays a delete and a create (see [`create_route`]).
            known.push(entry);
            continue;
        }
        new.push(entry);
        route_inserts.push(RouteInsert {
            id: route.id.clone(),
            asset_id: route.asset.clone(),
            target_id: route.target.clone(),
            name: file_name,
            url,
            properties: bag_of(None, &route.properties, route.description.as_deref())?,
        });
    }

    let MonitorPlan { links, unresolved } = monitor_plan(tx, &wanted, &stored, &inserts).await?;

    Ok(Plan {
        preview: ImportPreview {
            name,
            known,
            new,
            changes,
            monitor_links: links,
            unresolved,
        },
        assets: inserts,
        routes: route_inserts,
    })
}

/// The ids the file points at without describing, that the estate holds.
async fn referenced_assets(
    tx: &mut Transaction<'_, Postgres>,
    parsed: &EstateFile,
    file_assets: &HashSet<&str>,
) -> Result<Vec<String>, IpcError> {
    let mut wanted: Vec<String> = Vec::new();
    let mut want = |id: Option<&str>| {
        if let Some(id) = id.filter(|id| !file_assets.contains(id)) {
            wanted.push(id.to_owned());
        }
    };
    for asset in &parsed.assets {
        want(asset.parent.as_deref());
    }
    for route in &parsed.routes {
        want(Some(route.asset.as_str()));
        want(route.target.as_deref());
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let mut found: Vec<String> = Vec::new();
    for row in sqlx::query(REFERENCED_ASSETS)
        .bind(&wanted)
        .fetch_all(&mut **tx)
        .await?
    {
        found.push(row.try_get("id")?);
    }
    Ok(found)
}

/// Refuse a reference that names nothing.
fn resolves(
    id: &str,
    file_assets: &HashSet<&str>,
    in_estate: &HashSet<&str>,
    entry: &str,
    wanted_for: &str,
) -> Result<(), IpcError> {
    if file_assets.contains(id) || in_estate.contains(id) {
        return Ok(());
    }
    Err(IpcError::invalid(format!(
        "`{entry}` names `{id}` {wanted_for}, and there is no such asset in the \
         file or in the estate"
    )))
}

/// The file's assets, **a parent before what it holds**.
///
/// The order the apply inserts in, and the reason it can write `parent_id`
/// straight into the insert: every row it writes is a row with no children yet
/// whose parent is already there.
///
/// **A file whose assets hold each other is refused here, by name, before a
/// row is written.** A cycle reaching `knobas.asset` would make
/// [`recompute_paths`] run forever, which no gate can wait out; the ordering
/// and this refusal are two readings of one pass, because a set of assets that
/// cannot be ordered parent-first is exactly a set that holds itself.
///
/// `in_estate` is **every** id the estate already holds among the ones the
/// file names *or points at* -- not just the file's own entries. A file may
/// hang a new subtree under something a person created by hand
/// ([`REFERENCED_ASSETS`]), and a parent that is already there is a parent
/// nothing waits for; reading only the file's own known ids here would leave
/// such an asset forever unready and refuse a legal file with a sentence about
/// a cycle it does not have.
fn ordered<'a>(
    assets: &'a [FileAsset],
    in_estate: &HashSet<&str>,
) -> Result<Vec<&'a FileAsset>, IpcError> {
    let mut out: Vec<&'a FileAsset> = Vec::with_capacity(assets.len());
    let mut placed: HashSet<&'a str> = HashSet::new();
    let mut left: Vec<&'a FileAsset> = assets.iter().collect();

    while !left.is_empty() {
        let mut ready: Vec<&'a FileAsset> = Vec::new();
        let mut waiting: Vec<&'a FileAsset> = Vec::new();
        for asset in left {
            // An asset the estate already holds is not being inserted, so
            // nothing waits for it and it waits for nothing.
            let is_ready = in_estate.contains(asset.id.as_str())
                || match asset.parent.as_deref() {
                    // At the top of the estate, and nothing to wait for.
                    None => true,
                    Some(parent) => placed.contains(parent) || in_estate.contains(parent),
                };
            if is_ready {
                ready.push(asset);
            } else {
                waiting.push(asset);
            }
        }
        if ready.is_empty() {
            let mut names: Vec<&str> = waiting.iter().map(|asset| asset.id.as_str()).collect();
            names.sort_unstable();
            return Err(IpcError::invalid(format!(
                "these assets hold each other, so there is no order to write \
                 them in: {}",
                names.join(", ")
            )));
        }
        for asset in &ready {
            placed.insert(asset.id.as_str());
        }
        out.extend(ready);
        left = waiting;
    }
    Ok(out)
}

/// What the monitor half of one import comes to: the links it would draw, and
/// the names that found nothing.
///
/// One walk answers both, because they are one question asked of each name --
/// *does the mirror hold a monitor called this* -- and two walks would be two
/// chances for a name to be in neither list or in both.
struct MonitorPlan {
    links: Vec<MonitorLink>,
    unresolved: Vec<UnresolvedMonitor>,
}

/// The `monitored-by` links an apply would draw, and the names it would leave
/// waiting.
///
/// Over every asset the file names -- the new ones as well as the known ones,
/// because a monitor the mirror already holds should be joined to the asset the
/// same import created a moment earlier.
///
/// `wanted` is every name each asset would carry *after* this import, so a
/// name an earlier import kept is resolved by this one and reported by this one
/// -- spec #427's *"resolved by the next import"* and issue #445's *"kept and
/// reported in the preview"* are two readings of that same map.
async fn monitor_plan(
    tx: &mut Transaction<'_, Postgres>,
    wanted: &HashMap<String, Vec<String>>,
    stored: &HashMap<String, StoredAsset>,
    inserts: &[AssetInsert],
) -> Result<MonitorPlan, IpcError> {
    let mut names: Vec<String> = wanted.values().flatten().cloned().collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        return Ok(MonitorPlan {
            links: Vec::new(),
            unresolved: Vec::new(),
        });
    }

    let mut mirrored: HashMap<String, Vec<String>> = HashMap::new();
    for row in sqlx::query(LIVE_MONITORS)
        .bind(&names)
        .bind(MONITOR_KIND)
        .fetch_all(&mut **tx)
        .await?
    {
        mirrored
            .entry(row.try_get("title")?)
            .or_default()
            .push(row.try_get("entity_id")?);
    }

    // **No early return on an empty mirror.** Before the Kuma source is
    // configured that is every file's ordinary state, and it is exactly the
    // state the unresolved list exists to report; returning here would make
    // the report empty precisely when it has the most to say.
    let ids: Vec<String> = wanted.keys().cloned().collect();
    let mut drawn: HashSet<(String, String)> = HashSet::new();
    for row in sqlx::query(MONITOR_LINKS)
        .bind(&ids)
        .bind(MONITORED_BY)
        .fetch_all(&mut **tx)
        .await?
    {
        let from: String = row.try_get("from_id")?;
        let to: String = row.try_get("to_id")?;
        drawn.insert((from.clone(), to.clone()));
        drawn.insert((to, from));
    }

    let name_of = |id: &str| {
        stored
            .get(id)
            .map(|asset| asset.name.clone())
            .or_else(|| {
                inserts
                    .iter()
                    .find(|insert| insert.id == id)
                    .map(|insert| insert.name.clone())
            })
            .unwrap_or_else(|| id.to_owned())
    };

    // By asset id and then by monitor name, so two runs over one file answer
    // in one order: a `HashMap`'s own is not an order a reader can rely on.
    let mut assets: Vec<&String> = wanted.keys().collect();
    assets.sort();
    let mut links: Vec<MonitorLink> = Vec::new();
    let mut unresolved: Vec<UnresolvedMonitor> = Vec::new();
    for asset_id in assets {
        for monitor_name in &wanted[asset_id] {
            let Some(monitor_ids) = mirrored.get(monitor_name) else {
                unresolved.push(UnresolvedMonitor {
                    asset_id: asset_id.clone(),
                    asset_name: name_of(asset_id),
                    monitor_name: monitor_name.clone(),
                });
                continue;
            };
            for monitor_id in monitor_ids {
                if drawn.contains(&(asset_id.clone(), monitor_id.clone())) {
                    continue;
                }
                links.push(MonitorLink {
                    asset_id: asset_id.clone(),
                    asset_name: name_of(asset_id),
                    monitor_name: monitor_name.clone(),
                    monitor_id: monitor_id.clone(),
                });
            }
        }
    }
    Ok(MonitorPlan { links, unresolved })
}

/// The file's plain scalars as the tagged values everything else in this module
/// carries, plus the entry's `description` as a property of its own.
fn bag_of(
    declared: Option<&'static AssetType>,
    properties: &serde_json::Map<String, serde_json::Value>,
    description: Option<&str>,
) -> Result<serde_json::Map<String, serde_json::Value>, IpcError> {
    let mut bag = serde_json::Map::new();
    for (key, raw) in properties {
        let key = key.trim();
        if key.is_empty() {
            return Err(IpcError::invalid("a property needs a key"));
        }
        let value = property_of(declared, key, raw)?;
        value.vet(key)?;
        bag.insert(key.to_owned(), stored_value(&value)?);
    }
    if let Some(description) = description.map(str::trim).filter(|text| !text.is_empty()) {
        if bag.contains_key(DESCRIPTION_KEY) {
            return Err(IpcError::invalid(format!(
                "this entry carries a `{DESCRIPTION_KEY}` of its own and a \
                 `{DESCRIPTION_KEY}` property, and they say different things"
            )));
        }
        bag.insert(
            DESCRIPTION_KEY.to_owned(),
            stored_value(&PropertyValue::Text {
                value: description.to_owned(),
            })?,
        );
    }
    Ok(bag)
}

/// One plain scalar, given the kind the type declares for its key.
///
/// The file writes `"cx23"` and `2`; everything downstream of this module
/// carries `{"kind":"text","value":"cx23"}`. #428 chose the tagged shape and
/// left this translation to the import in as many words, and the rule is the
/// one that loses nothing:
///
/// * a **declared** key takes the kind its type declares, so a `port` the
///   schema calls a number is a number and a `last_run` it calls a date is a
///   date -- and [`PropertyValue::vet`] refuses one that is not really either;
/// * an **undeclared** key takes the kind JSON already gave it: a string is
///   text, a number is a number.
///
/// The one inference deliberately not made is a URL out of a string beginning
/// `https://`. `jdbc_url` and a service's `url` are both in the real estate and
/// only one of them is openable, and the *kind* is what the open-URL action
/// reads -- so a custom key stays text until somebody says otherwise in the
/// pane.
///
/// A shape that is neither string nor number -- a bool, a list, an object, a
/// null -- is refused rather than stringified, because there is no reading of
/// `["a","b"]` as a property that a reader would recognise on the other side.
fn property_of(
    declared: Option<&'static AssetType>,
    key: &str,
    raw: &serde_json::Value,
) -> Result<PropertyValue, IpcError> {
    let kind = declared
        .and_then(|declared| {
            declared
                .properties
                .iter()
                .find(|property| property.key == key)
        })
        .map(|property| property.kind);
    match raw {
        serde_json::Value::String(value) => match kind {
            None | Some(PropertyKind::Text) => Ok(PropertyValue::Text {
                value: value.clone(),
            }),
            Some(PropertyKind::Date) => Ok(PropertyValue::Date {
                value: value.clone(),
            }),
            Some(PropertyKind::Url) => Ok(PropertyValue::Url {
                value: value.clone(),
            }),
            Some(PropertyKind::Number) => Err(kind_mismatch(key, kind, "string")),
        },
        serde_json::Value::Number(value) => match kind {
            None | Some(PropertyKind::Number) => Ok(PropertyValue::Number {
                value: value.as_f64().ok_or_else(|| {
                    IpcError::invalid(format!("`{key}` is a number JSON cannot carry"))
                })?,
            }),
            Some(_) => Err(kind_mismatch(key, kind, "number")),
        },
        other => Err(IpcError::invalid(format!(
            "`{key}` is {other}, and an estate file's properties are plain \
             strings and numbers"
        ))),
    }
}

/// A file value whose JSON shape is not the kind its type declares.
fn kind_mismatch(key: &str, kind: Option<PropertyKind>, was: &str) -> IpcError {
    IpcError::invalid(format!(
        "`{key}` is a {} property and the file gives it a {was}",
        kind.map_or("custom", PropertyKind::as_str)
    ))
}

/// What the pane calls a key: the type's label, or the key itself.
fn label_of(declared: Option<&'static AssetType>, key: &str) -> String {
    declared
        .and_then(|declared| {
            declared
                .properties
                .iter()
                .find(|property| property.key == key)
        })
        .map_or_else(|| key.to_owned(), |property| property.label.to_owned())
}

/// An id the file gave, in the namespace its entry belongs to.
///
/// The namespace is not decoration: `0017`'s `asset_id_ns_chk` and `0018`'s
/// `route_id_ns_chk` refuse anything else, so a file calling a route
/// `asset:tunnel-jira` would fail on a constraint halfway through the write
/// rather than on a sentence about the file.
fn vet_file_id(id: &str, namespace: &str) -> Result<(), IpcError> {
    let parsed = EntityRef::parse(id).map_err(|error| {
        IpcError::invalid(format!(
            "`{id}` is not an id an estate file can give: {error}"
        ))
    })?;
    if parsed.namespace != namespace {
        return Err(IpcError::invalid(format!(
            "`{id}` is in the `{}` namespace and this entry is a {namespace}; an \
             imported id keeps the file's spelling, so it has to be the right one",
            parsed.namespace
        )));
    }
    Ok(())
}

fn duplicate_file_id(id: &str) -> IpcError {
    IpcError::invalid(format!(
        "`{id}` is the id of two entries in this file, and an id is an address"
    ))
}

/// The file's environment, refused by name rather than by `0017`'s constraint.
fn vet_environment(value: &str, id: &str) -> Result<Environment, IpcError> {
    Environment::ALL
        .into_iter()
        .find(|environment| environment.as_str() == value)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "`{id}` is in the environment {value:?}, which is not one of {:?}",
                Environment::ALL.map(Environment::as_str)
            ))
        })
}

/// The monitor names one asset lists, trimmed, deduplicated and refused when
/// blank.
///
/// `estate_file.rs` states the rule this enforces at the other end: an empty
/// entry is refused rather than dropped, *"so that nothing watches this and
/// somebody meant to fill this in do not read alike"*. `0020`'s
/// `asset_monitors_chk` is the floor under it.
fn vet_monitors(monitors: &[String], id: &str) -> Result<Vec<String>, IpcError> {
    let mut out: Vec<String> = Vec::new();
    for name in monitors {
        let name = name.trim();
        if name.is_empty() {
            return Err(IpcError::invalid(format!(
                "`{id}` names a monitor with a blank name; leave the entry out \
                 instead"
            )));
        }
        if !out.iter().any(|kept| kept == name) {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The plumbing the four writers share
// ---------------------------------------------------------------------------

/// The entity row every asset and every route is written beside.
///
/// **`on conflict` because a delete tombstones the entity row and removes only
/// the `knobas.asset` one.** [`create`] can never reach that clause -- it mints
/// a fresh UUID -- but the import keeps the file's id, so re-importing a file
/// that names an asset the reader deleted meets its own tombstone. Without the
/// clause that is a duplicate-key error surfaced as `internal`, halfway through
/// a transaction, on a file the preview called importable.
///
/// Reviving it is the right answer rather than a refusal: the id is the estate
/// file's, an id carries its namespace so nothing of another kind can collide
/// with it, and a file that still names the asset is a file saying the thing is
/// there. `deleted_at = null` is what puts it back in the launcher and takes
/// the *withdrawn* marker off the links drawn to it, which are the two things
/// the tombstone was doing.
const REVIVE_ENTITY: &str = "insert into knobas.entity (id, kind, title, updated_at)
     values ($1, $2, $3, now())
     on conflict (id) do update
        set kind = excluded.kind, title = excluded.title,
            deleted_at = null, updated_at = now()";

/// One asset row on its way into the table, as either writer hands it over.
///
/// A struct rather than nine positional arguments, and a **shared** insert
/// rather than one per writer: [`create`] mints an id and states nothing about
/// environment, owner or monitors, and the import (#439) keeps the file's id
/// and states all three. Everything else about the two is identical, and two
/// copies of "insert the entity row, then the asset row" is exactly the pair
/// that drifts the first time a column is added to one of them.
struct AssetRowInsert<'a> {
    /// Minted (`create`) or the estate file's (the import). Either way it is
    /// the id of the `knobas.entity` row written beside it.
    id: &'a str,
    parent_id: Option<&'a str>,
    type_id: &'a str,
    name: &'a str,
    properties: &'a serde_json::Map<String, serde_json::Value>,
    environment: Option<Environment>,
    owner: Option<&'a str>,
    /// The Uptime Kuma names of the monitors watching it (`0020`), which only
    /// a file states.
    monitors: &'a [String],
    /// Already resolved by the caller, because the two callers resolve it
    /// differently: `create` reads the parent once, and the import walks a
    /// whole file parent-first.
    path_text: &'a str,
}

/// The entity row and the asset row, written together.
async fn insert_asset(
    tx: &mut Transaction<'_, Postgres>,
    row: AssetRowInsert<'_>,
) -> Result<(), IpcError> {
    sqlx::query(REVIVE_ENTITY)
        .bind(row.id)
        .bind(NAMESPACE)
        .bind(row.name)
        .execute(&mut **tx)
        .await?;

    sqlx::query(
        "insert into knobas.asset
             (id, parent_id, type_id, name, properties, path_text, environment, owner, monitors)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(row.id)
    .bind(row.parent_id)
    .bind(row.type_id)
    .bind(row.name)
    .bind(serde_json::Value::Object(row.properties.clone()))
    .bind(row.path_text)
    .bind(row.environment.map(Environment::as_str))
    .bind(row.owner)
    .bind(row.monitors)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One route row on its way into the table -- [`AssetRowInsert`]'s twin, for
/// its reason.
struct RouteRowInsert<'a> {
    id: &'a str,
    asset_id: &'a str,
    target_id: Option<&'a str>,
    name: &'a str,
    url: &'a str,
    visibility: Visibility,
    properties: &'a serde_json::Map<String, serde_json::Value>,
}

/// The entity row and the route row, written together.
async fn insert_route(
    tx: &mut Transaction<'_, Postgres>,
    row: RouteRowInsert<'_>,
) -> Result<(), IpcError> {
    sqlx::query(REVIVE_ENTITY)
        .bind(row.id)
        .bind(ROUTE_NAMESPACE)
        .bind(row.name)
        .execute(&mut **tx)
        .await?;

    sqlx::query(
        "insert into knobas.route (id, asset_id, target_id, name, url, visibility, properties)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(row.id)
    .bind(row.asset_id)
    .bind(row.target_id)
    .bind(row.name)
    .bind(row.url)
    .bind(row.visibility.as_str())
    .bind(serde_json::Value::Object(row.properties.clone()))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The stored row, as the writers need it: the wire row plus the two columns
/// no reader draws.
struct Stored {
    parent_id: Option<String>,
    type_id: String,
    name: String,
    status: AssetStatus,
    environment: Option<Environment>,
    owner: Option<String>,
    properties: serde_json::Map<String, serde_json::Value>,
}

/// Read an asset inside a transaction and hold it until the transaction ends.
///
/// `for update` because every writer here reads a value, decides against it and
/// writes: without the lock two edits racing on one asset would each write a
/// history line saying it changed from the value the *other* one replaced.
async fn locked(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<Stored, IpcError> {
    let row = sqlx::query(
        "select parent_id, type_id, name, status, environment, owner, properties
           from knobas.asset where id = $1 for update",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| no_such_asset(id))?;

    let status: String = row.try_get("status")?;
    let environment: Option<String> = row.try_get("environment")?;
    let properties: serde_json::Value = row.try_get("properties")?;
    Ok(Stored {
        parent_id: row.try_get("parent_id")?,
        type_id: row.try_get("type_id")?,
        name: row.try_get("name")?,
        status: AssetStatus::parse(&status)?,
        environment: environment.as_deref().map(Environment::parse).transpose()?,
        owner: row.try_get("owner")?,
        properties: match properties {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        },
    })
}

/// One asset as the wire carries it, after a write.
async fn one(pool: &PgPool, id: &str) -> Result<AssetRow, IpcError> {
    let row = sqlx::query(ONE)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| no_such_asset(id))?;
    rows_of(pool, std::slice::from_ref(&row))
        .await?
        .pop()
        .ok_or_else(|| no_such_asset(id))
}

/// The three statements that set a single column, each named where it is used.
///
/// `&'static str` constants rather than a helper that matches on a column
/// *name*: an earlier draft did the latter and carried an arm no caller could
/// reach, which is a second switch over a choice the call site has already
/// made. Nothing a caller typed is ever any part of these.
const SET_STATUS: &str =
    "update knobas.asset set status = coalesce($2, 'none'), updated_at = now() where id = $1";
const SET_ENVIRONMENT: &str =
    "update knobas.asset set environment = $2, updated_at = now() where id = $1";
const SET_OWNER: &str = "update knobas.asset set owner = $2, updated_at = now() where id = $1";

/// Run one of [`SET_STATUS`], [`SET_ENVIRONMENT`] or [`SET_OWNER`].
async fn set_column(
    tx: &mut Transaction<'_, Postgres>,
    statement: &'static str,
    id: &str,
    value: Option<&str>,
) -> Result<(), IpcError> {
    sqlx::query(statement)
        .bind(id)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The `path_text` a child of `parent` would carry.
///
/// `for update`, for [`locked`]'s reason applied one row over: `create` reads
/// the parent's name here, decides a `path_text` from it and writes that into
/// the child. A rename of the parent committing in between would leave the new
/// child carrying the old name for good -- the rename's own
/// [`recompute_paths`] walks only the descendants its transaction can see, and
/// a child inserted after it is not one. The insert's own `for key share` on
/// the foreign key does not close this: a rename does not touch the key.
async fn path_below(tx: &mut Transaction<'_, Postgres>, parent: &str) -> Result<String, IpcError> {
    let row = sqlx::query("select name, path_text from knobas.asset where id = $1 for update")
        .bind(parent)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no asset {parent} to create under")))?;
    let name: String = row.try_get("name")?;
    let path: String = row.try_get("path_text")?;
    Ok(if path.is_empty() {
        name
    } else {
        format!("{path}{PATH_SEPARATOR}{name}")
    })
}

/// Rewrite `path_text` for an asset and everything under it.
///
/// One statement, because a subtree walked in Rust is a round trip per asset
/// and a window in which half the estate says where it used to be.
async fn recompute_paths(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<(), IpcError> {
    sqlx::query(
        "with recursive sub(id, path_text) as (
             select a.id,
                    coalesce((select case when p.path_text = '' then p.name
                                          else p.path_text || $2::text || p.name end
                                from knobas.asset p where p.id = a.parent_id), '')
               from knobas.asset a where a.id = $1
             union all
             select c.id,
                    case when s.path_text = '' then sp.name
                         else s.path_text || $2::text || sp.name end
               from sub s
               join knobas.asset sp on sp.id = s.id
               join knobas.asset c on c.parent_id = s.id
         )
         update knobas.asset a set path_text = sub.path_text
           from sub where a.id = sub.id and a.path_text is distinct from sub.path_text",
    )
    .bind(id)
    .bind(PATH_SEPARATOR)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The name of the asset that **closes the loop**, or `None` for a legal move.
///
/// Walks up from `parent`: if `id` is anywhere above it, `id` would end up
/// holding itself. The answer is a *name* rather than a bare `true` so the
/// refusal can say where the loop runs, and the name is the one step below
/// `id` on that walk -- the asset `id` already holds on the way down to
/// `parent`. Returning `id`'s own name instead would make the refusal read
/// *"hel1 is already held by hel1"*, which is the "under itself" sentence the
/// whole message exists to avoid; `below` is what makes it a fact a reader can
/// act on, and it is the only interpolation in that message no other argument
/// already supplies.
///
/// `below` is `null` only on the walk's first row, which is `parent` itself --
/// the one-step case `move_to` has already refused above and
/// `asset_no_self_parent_chk` forbids underneath. The `coalesce` is the floor
/// under that, not a route.
async fn cycle_through(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    parent: &str,
) -> Result<Option<String>, IpcError> {
    let row = sqlx::query(
        "with recursive up as (
             select a.id, a.parent_id, a.name, null::text as below
               from knobas.asset a where a.id = $2
             union all
             select p.id, p.parent_id, p.name, up.name
               from knobas.asset p join up on p.id = up.parent_id
         )
         select coalesce(below, name) as name from up where id = $1",
    )
    .bind(id)
    .bind(parent)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| row.try_get::<String, _>("name"))
        .transpose()
        .map_err(Into::into)
}

/// The `not_found` every read and every writer raises for an id no asset
/// carries. One sentence, in one place: three call sites wrote it out and a
/// fourth would have written it slightly differently.
fn no_such_asset(id: &str) -> IpcError {
    IpcError::not_found(format!("no asset {id}"))
}

/// The same, for a route.
fn no_such_route(id: &str) -> IpcError {
    IpcError::not_found(format!("no route {id}"))
}

// ---------------------------------------------------------------------------
// The plumbing the route writers share (#432)
// ---------------------------------------------------------------------------

/// The stored route, as its writers need it.
struct StoredRoute {
    target_id: Option<String>,
    name: String,
    url: String,
    visibility: Visibility,
    properties: serde_json::Map<String, serde_json::Value>,
}

/// Read a route inside a transaction and hold it until the transaction ends.
///
/// `for update`, for [`locked`]'s reason: every writer here reads a value,
/// decides against it and writes, and two edits racing on one route would
/// otherwise each record a change from the value the other one replaced.
async fn locked_route(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
) -> Result<StoredRoute, IpcError> {
    let row = sqlx::query(
        "select target_id, name, url, visibility, properties
           from knobas.route where id = $1 for update",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| no_such_route(id))?;

    let properties: serde_json::Value = row.try_get("properties")?;
    Ok(StoredRoute {
        target_id: row.try_get("target_id")?,
        name: row.try_get("name")?,
        url: row.try_get("url")?,
        visibility: Visibility::parse(&row.try_get::<String, _>("visibility")?)?,
        properties: match properties {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        },
    })
}

/// One route as the wire carries it, after a write.
async fn one_route(pool: &PgPool, id: &str) -> Result<RouteRow, IpcError> {
    let row = sqlx::query(ROUTE_ONE)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| no_such_route(id))?;
    route_row_of(&row)
}

/// Refuse an asset id nothing answers to, saying what it was wanted **for**.
///
/// Named for what it does rather than for what it asks: it *refuses*, and it
/// takes a lock while it is at it, which is not what a reader expects of a
/// function called `exists`. The neighbours it sits among are `vet_name_of`,
/// `vet_url` and `no_such_asset`.
///
/// `for update`, for [`path_below`]'s reason one row over: a route created
/// against an asset deleted in a transaction committing in between would leave
/// the insert failing on the foreign key with Postgres' own sentence instead
/// of this one. The lock is what makes the check and the insert one decision.
async fn must_exist(
    tx: &mut Transaction<'_, Postgres>,
    asset_id: &str,
    wanted_for: &str,
) -> Result<(), IpcError> {
    sqlx::query("select 1 as one from knobas.asset where id = $1 for update")
        .bind(asset_id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|_| ())
        .ok_or_else(|| IpcError::not_found(format!("no asset {asset_id} {wanted_for}")))
}

/// The four statements that set a single route column, each named where it is
/// used -- [`SET_STATUS`]'s arrangement, for its reason.
const SET_ROUTE_NAME: &str = "update knobas.route set name = $2, updated_at = now() where id = $1";
const SET_ROUTE_URL: &str = "update knobas.route set url = $2, updated_at = now() where id = $1";
const SET_ROUTE_TARGET: &str =
    "update knobas.route set target_id = $2, updated_at = now() where id = $1";
// No `coalesce` here, unlike `SET_STATUS`: a status edit carries an `Option`
// and a cleared one means `none`, while a route's visibility is never cleared
// -- `RouteEdit::Visibility` carries the value itself -- so a default in the
// statement would be an arm no caller can reach.
const SET_ROUTE_VISIBILITY: &str =
    "update knobas.route set visibility = $2, updated_at = now() where id = $1";

/// Run one of the four `SET_ROUTE_*` statements.
async fn set_route_column(
    tx: &mut Transaction<'_, Postgres>,
    statement: &'static str,
    id: &str,
    value: Option<&str>,
) -> Result<(), IpcError> {
    sqlx::query(statement)
        .bind(id)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// A URL or endpoint, refused when it is neither.
///
/// **A scheme and no more.** `https://kuma.local`, `postgres://10.0.0.20:5432`
/// and `ssh://hel1` are all routes the estate actually has, so this is not the
/// http(s) rule [`PropertyValue::vet`] applies to a *url property* -- that one
/// guards an *open URL* action, and this one guards a fact about a machine.
/// What it does refuse is a bare host, because "reachable at kuma.local" does
/// not say how, and whitespace, because a URL with a space in it is a line
/// somebody pasted two of.
fn vet_url(url: &str) -> Result<String, IpcError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(IpcError::invalid("a route needs a URL"));
    }
    if url.split_once("://").is_none_or(|(scheme, rest)| {
        scheme.is_empty()
            || !scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            || rest.is_empty()
    }) {
        return Err(IpcError::invalid(format!(
            "{url:?} carries no scheme -- a route is a URL or an endpoint, \
             `https://…` or `postgres://…`, not a bare host"
        )));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(IpcError::invalid(format!(
            "{url:?} has whitespace in it, so it is not one address"
        )));
    }
    Ok(url.to_owned())
}

/// The properties of a thing with no schema to check them against.
///
/// [`vet_properties`] without the type: a route declares nothing, so every key
/// is the reader's own and any of the four kinds fits it. The *values* are
/// vetted exactly as an asset's are -- a blank text, a non-finite number, a
/// date that is not a date and a URL with no scheme are refused here too,
/// because those rules are about what can be read back and not about who
/// declared the key.
fn vet_custom_properties(
    properties: &[(String, PropertyValue)],
) -> Result<serde_json::Map<String, serde_json::Value>, IpcError> {
    let mut bag = serde_json::Map::new();
    for (key, value) in properties {
        let key = key.trim();
        if key.is_empty() {
            return Err(IpcError::invalid("a property needs a key"));
        }
        value.vet(key)?;
        bag.insert(key.to_owned(), stored_value(value)?);
    }
    Ok(bag)
}

/// The asset's entity reference, for the activity line every mutation writes.
///
/// The failure is `internal` rather than `invalid` because the id was read
/// back out of `knobas.asset`, where `asset_id_ns_chk` has already made it a
/// well-formed one: a parse that fails here means the row is malformed, which
/// is knobas' fault and not the caller's.
fn entity_of(id: &str) -> Result<EntityRef, IpcError> {
    EntityRef::parse(id).map_err(IpcError::internal)
}

/// One property value as the `properties` bag stores it.
///
/// `to_value` can only fail on a non-finite float, which [`PropertyValue::vet`]
/// has already refused by the time anything here calls this -- so the failure
/// is `internal`, and it is a mapped error rather than an `unwrap` because a
/// panic inside a command takes the window's IPC worker with it.
fn stored_value(value: &PropertyValue) -> Result<serde_json::Value, IpcError> {
    serde_json::to_value(value).map_err(IpcError::internal)
}

fn vet_type(type_id: &str) -> Result<&'static AssetType, IpcError> {
    asset::find(type_id).ok_or_else(|| {
        IpcError::invalid(format!(
            "{type_id:?} is not one of the {} built-in asset types",
            asset::TYPES.len()
        ))
    })
}

fn vet_name(name: &str) -> Result<String, IpcError> {
    vet_name_of("an asset", name)
}

/// The same rule, said about whichever entity is being named.
///
/// `what` is the article and the noun -- *an asset*, *a route* -- because the
/// refusal is read by somebody who has just pressed Save on a dialog, and
/// "an asset needs a name" over a route's name box is a sentence about
/// something they were not doing.
fn vet_name_of(what: &str, name: &str) -> Result<String, IpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid(format!("{what} needs a name")));
    }
    Ok(name.to_owned())
}

/// A typed key must be given the kind its type declares; a custom key may be
/// any of the four.
fn vet_against_schema(
    declared: Option<&'static AssetType>,
    key: &str,
    value: &PropertyValue,
) -> Result<(), IpcError> {
    let Some(property) = declared.and_then(|t| t.properties.iter().find(|p| p.key == key)) else {
        return Ok(());
    };
    if property.kind != value.kind() {
        return Err(IpcError::invalid(format!(
            "{key:?} is a {} property and was given a {}",
            property.kind.as_str(),
            value.kind().as_str()
        )));
    }
    Ok(())
}

fn vet_properties(
    declared: &'static AssetType,
    properties: &[(String, PropertyValue)],
) -> Result<serde_json::Map<String, serde_json::Value>, IpcError> {
    let mut bag = serde_json::Map::new();
    for (key, value) in properties {
        let key = key.trim();
        if key.is_empty() {
            return Err(IpcError::invalid("a property needs a key"));
        }
        value.vet(key)?;
        vet_against_schema(Some(declared), key, value)?;
        bag.insert(key.to_owned(), stored_value(value)?);
    }
    Ok(bag)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const MIGRATION: &str =
        include_str!("../../../knobas-db/migrations/0017_the_estate_and_its_assets.sql");

    const ROUTE_MIGRATION: &str =
        include_str!("../../../knobas-db/migrations/0018_the_route_an_asset_exposes.sql");

    /// The spellings the wire uses and the ones the schema accepts are one
    /// list in two languages, and neither may grow without the other.
    ///
    /// The same shape as `knobas_core::link::Origin`'s pin against `0003` and
    /// `time::BlockKind`'s against `0013`: the constraint is found by the line
    /// that lists it, so the vocabulary has to stay on one line there.
    #[test]
    fn every_status_is_one_the_schema_accepts() {
        let listed = vocabulary(MIGRATION, "asset_status_chk");
        for status in AssetStatus::ALL {
            assert!(
                listed.contains(&status.as_str()),
                "{:?} is on the wire and not in the constraint: {listed:?}",
                status.as_str()
            );
        }
        assert_eq!(listed.len(), AssetStatus::ALL.len(), "{listed:?}");
    }

    #[test]
    fn every_environment_is_one_the_schema_accepts() {
        let listed = vocabulary(MIGRATION, "asset_environment_chk");
        for environment in Environment::ALL {
            assert!(
                listed.contains(&environment.as_str()),
                "{:?} is on the wire and not in the constraint: {listed:?}",
                environment.as_str()
            );
        }
        assert_eq!(listed.len(), Environment::ALL.len(), "{listed:?}");
    }

    /// `0018`'s vocabulary, held to the wire's the way the two above are.
    #[test]
    fn every_visibility_is_one_the_schema_accepts() {
        let listed = vocabulary(ROUTE_MIGRATION, "route_visibility_chk");
        for visibility in Visibility::ALL {
            assert!(
                listed.contains(&visibility.as_str()),
                "{:?} is on the wire and not in the constraint: {listed:?}",
                visibility.as_str()
            );
        }
        assert_eq!(listed.len(), Visibility::ALL.len(), "{listed:?}");
    }

    /// The `'a','b'` list on the line naming `constraint`.
    ///
    /// Two migrations are read through this now (`0017`'s two vocabularies and
    /// `0018`'s), so the panics name the constraint rather than a file: a
    /// message that said `0017` while reading `0018` would send the next
    /// reader to the wrong file on the one day it fires.
    fn vocabulary<'m>(migration: &'m str, constraint: &str) -> Vec<&'m str> {
        let line = migration
            .lines()
            .find(|line| line.contains(constraint))
            .unwrap_or_else(|| panic!("{constraint} is missing from its migration"));
        let (_, rest) = line
            .rsplit_once(" in (")
            .unwrap_or_else(|| panic!("{constraint} does not list its values: {line}"));
        let (list, _) = rest
            .split_once(')')
            .unwrap_or_else(|| panic!("{constraint} does not close its list: {line}"));
        list.split(',')
            .map(|value| value.trim_matches('\''))
            .collect()
    }

    /// The namespace an asset id is written in is the one `knobas-core`
    /// reserves -- so a rename there fails here rather than silently making
    /// every asset id a legal source id.
    #[test]
    fn the_asset_namespace_is_one_knobas_core_reserves() {
        assert!(knobas_core::entity::is_reserved_namespace(NAMESPACE));
        assert!(knobas_core::entity::is_owned_kind(NAMESPACE));
    }

    /// The shell keeps its own copy of this word, and this is what stops the
    /// two drifting (#437).
    ///
    /// `app/src/lib/shell/timer.ts`'s `assetTargetId` decides whether the top
    /// strip looks a timer target up in the estate, and it decides it on the
    /// namespace half of the id. A rename here would leave that matching a
    /// spelling nothing mints any more -- the strip silently back on drawing
    /// uuids, and every frontend test still green, because each of them
    /// supplies its own `asset:` id.
    ///
    /// A source scan from Rust for the same reason
    /// `commands::time::tests::the_shells_context_namespace_is_the_one_the_backend_refuses`
    /// gives for its own: the constant lives in TypeScript, and nothing else
    /// in the tree compares it to the Rust it claims to mirror.
    #[test]
    fn the_shells_asset_namespace_is_the_one_the_estate_mints() {
        const SHELL: &str = include_str!("../../../../app/src/lib/shell/timer.ts");
        let declaration = format!("const ASSET_NAMESPACE = \"{NAMESPACE}\";");
        assert!(
            SHELL.contains(&declaration),
            "app/src/lib/shell/timer.ts does not declare `{declaration}`, so the \
             top strip is looking up a namespace the estate no longer mints"
        );
    }

    /// And the route's, which is the same argument for the other entity this
    /// module writes: a `route:` id no mirror row may name is a route no
    /// sweep can reach.
    #[test]
    fn the_route_namespace_is_one_knobas_core_reserves() {
        assert!(knobas_core::entity::is_reserved_namespace(ROUTE_NAMESPACE));
        assert!(knobas_core::entity::is_owned_kind(ROUTE_NAMESPACE));
    }

    /// [`MONITOR_KIND`]'s doc comment claims the word is one knobas keeps, and
    /// this is the assertion behind the claim (#435).
    ///
    /// It is **not** an owned kind: a monitor is mirrored from Uptime Kuma
    /// (spec #427, *"assets are knobas-owned; monitors are mirrored"*), so the
    /// mirror row it arrives as is `kuma:<id>` and the namespace is the
    /// source's. What the reservation buys is that no *source* may call itself
    /// `monitor` and write its items where monitors live -- which is what
    /// makes [`MONITORED_ASSETS`]' `kind` clause a statement about Kuma's
    /// checks rather than about whatever a source happened to name itself.
    #[test]
    fn the_monitor_kind_is_a_word_knobas_keeps_for_itself() {
        assert!(knobas_core::entity::is_reserved_namespace(MONITOR_KIND));
        assert!(
            !knobas_core::entity::is_owned_kind(MONITOR_KIND),
            "a monitor is mirrored, not owned"
        );
    }

    fn text(value: &str) -> PropertyValue {
        PropertyValue::Text {
            value: value.to_owned(),
        }
    }

    fn stored(pairs: &[(&str, PropertyValue)]) -> serde_json::Value {
        let mut bag = serde_json::Map::new();
        for (key, value) in pairs {
            bag.insert((*key).to_owned(), serde_json::to_value(value).unwrap());
        }
        serde_json::Value::Object(bag)
    }

    /// Story 5 and story 6 in one list: the type's own keys in the type's own
    /// order, then whatever else was typed, alphabetically.
    #[test]
    fn the_pane_lists_declared_keys_in_schema_order_and_custom_keys_after() {
        let listed = properties_of(
            "vm",
            &stored(&[
                ("zzz-note", text("last")),
                ("os", text("Debian 13")),
                ("backup window", text("02:00")),
                ("ip", text("10.0.0.4")),
            ]),
        );
        let keys: Vec<&str> = listed.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                // The VM schema's four, in its order, including the one
                // nothing filled in.
                "hostname",
                "ip",
                "os",
                "size", // then the custom keys, by key
                "backup window",
                "zzz-note",
            ]
        );
        assert_eq!(
            listed.iter().map(|p| p.custom).collect::<Vec<_>>(),
            [false, false, false, false, true, true]
        );
        // The declared-but-unset one is a row with no value, not an absence:
        // that is how a reader learns a VM is meant to have a hostname.
        assert_eq!(listed[0].value, None);
        assert_eq!(listed[1].value, Some(text("10.0.0.4")));
        assert_eq!(listed[1].label, "IP");
        // A custom key labels itself -- there is no schema to ask.
        assert_eq!(listed[4].label, "backup window");
    }

    /// `custom` declares nothing, so every key it holds is a custom one.
    #[test]
    fn a_custom_asset_lists_only_what_was_typed_on_it() {
        let listed = properties_of("custom", &stored(&[("wattage", text("650W"))]));
        assert_eq!(listed.len(), 1);
        assert!(listed[0].custom);
    }

    /// A bag that is not an object, or a value that is not a property, is a
    /// row nobody can draw -- and it must not take the pane down with it.
    #[test]
    fn an_unreadable_bag_yields_the_schema_and_nothing_else() {
        let listed = properties_of("schema", &serde_json::json!("not an object"));
        assert_eq!(
            listed.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
            ["owner"]
        );
        let junk = properties_of("schema", &serde_json::json!({ "owner": 7, "extra": [] }));
        assert_eq!(junk[0].value, None, "an unreadable value reads as unset");
        assert_eq!(junk[1].key, "extra");
        assert_eq!(junk[1].value, None);
    }

    /// Each vetting rule refuses for its own reason, and a good value passes.
    #[test]
    fn a_property_value_is_vetted_for_the_kind_it_claims() {
        assert!(text("10.0.0.4").vet("ip").is_ok());
        assert!(text("   ").vet("ip").is_err());
        assert!(PropertyValue::Number { value: 8080.0 }.vet("port").is_ok());
        assert!(
            PropertyValue::Number {
                value: f64::INFINITY
            }
            .vet("port")
            .is_err()
        );
        assert!(
            PropertyValue::Date {
                value: "2026-09-06".to_owned()
            }
            .vet("last_run")
            .is_ok()
        );
        for bad in ["2026-13-01", "yesterday", ""] {
            assert!(
                PropertyValue::Date {
                    value: bad.to_owned()
                }
                .vet("last_run")
                .is_err(),
                "{bad:?} is not a date"
            );
        }
        assert!(
            PropertyValue::Url {
                value: "https://kuma.local".to_owned()
            }
            .vet("url")
            .is_ok()
        );
        assert!(
            PropertyValue::Url {
                value: "kuma.local".to_owned()
            }
            .vet("url")
            .is_err()
        );
    }

    /// A typed key takes its declared kind and nothing else; a custom key
    /// takes any of the four, which is what makes story 6 an escape hatch
    /// rather than a second schema.
    #[test]
    fn a_typed_key_refuses_a_kind_its_type_did_not_declare() {
        let service = asset::find("service").unwrap();
        assert!(
            vet_against_schema(
                Some(service),
                "port",
                &PropertyValue::Number { value: 443.0 }
            )
            .is_ok()
        );
        let wrong = vet_against_schema(Some(service), "port", &text("443")).unwrap_err();
        assert!(wrong.message.contains("port"), "{}", wrong.message);
        assert!(wrong.message.contains("number"), "{}", wrong.message);
        // Not declared by `service`, so it is the reader's own key.
        assert!(vet_against_schema(Some(service), "wattage", &text("650W")).is_ok());
    }

    #[test]
    fn a_type_no_table_declares_is_refused_by_name() {
        let refused = vet_type("flowrun-scenario").unwrap_err();
        assert_eq!(refused.code, crate::IpcErrorCode::Invalid);
        assert!(
            refused.message.contains("flowrun-scenario"),
            "{}",
            refused.message
        );
        assert!(vet_type("vm").is_ok());
    }

    #[test]
    fn a_blank_name_is_refused_and_a_padded_one_is_trimmed() {
        assert!(vet_name("  \t ").is_err());
        assert_eq!(vet_name("  vm-db-01 ").unwrap(), "vm-db-01");
        // And the refusal names the thing being named, which is the whole
        // point of the second argument.
        assert!(
            vet_name("").unwrap_err().message.contains("an asset"),
            "the asset's refusal says asset"
        );
        assert!(
            vet_name_of("a route", " ")
                .unwrap_err()
                .message
                .contains("a route"),
            "the route's refusal says route"
        );
    }

    // -----------------------------------------------------------------
    // #432: routes
    // -----------------------------------------------------------------

    /// A route's address is a URL or an endpoint, and the rule is *a scheme*.
    ///
    /// The three accepted spellings are ones the real estate holds or plainly
    /// will (`testenv/hetzner/estate.json`'s tunnels are `http://`, its
    /// databases speak `postgres://`, every server is reached over `ssh://`),
    /// and the refusals are the two ways a reader gets it wrong: a bare host,
    /// which does not say *how* the thing is reached, and a pasted line with
    /// whitespace in it, which is not one address.
    ///
    /// The last case is the one worth having: this is deliberately **not**
    /// `PropertyValue::vet`'s http(s) rule. That one guards the *open URL*
    /// action; a route is a fact about a machine, and a database endpoint is
    /// the commonest route there is that no browser can open.
    #[test]
    fn a_route_url_needs_a_scheme_and_nothing_more() {
        for good in [
            "https://kuma.local",
            "postgres://10.20.4.20:5432",
            "ssh://hel1",
            "  http://127.0.0.1:3000/  ",
        ] {
            assert_eq!(vet_url(good).unwrap(), good.trim(), "{good:?} is a route");
        }
        for bad in ["", "   ", "kuma.local", "://nowhere", "https://", "http:/x"] {
            assert!(vet_url(bad).is_err(), "{bad:?} is not an address");
        }
        let spaced = vet_url("https://kuma.local /health").unwrap_err();
        assert!(spaced.message.contains("whitespace"), "{}", spaced.message);
        assert_eq!(spaced.code, crate::IpcErrorCode::Invalid);
    }

    /// A route declares nothing, so every key it carries is the reader's own.
    ///
    /// The same list [`properties_of`] draws for the custom half of an asset,
    /// which is the point of there being one walk: the pane draws both with
    /// one component, and a route's certificate expiry is a property like any
    /// other.
    #[test]
    fn a_routes_properties_are_all_custom_in_key_order() {
        let listed = custom_properties(&stored(&[
            ("opened_by", text("tunnel up")),
            (
                "cert_expires",
                PropertyValue::Date {
                    value: "2026-12-01".to_owned(),
                },
            ),
        ]));
        assert_eq!(
            listed
                .iter()
                .map(|p| (p.key.as_str(), p.custom))
                .collect::<Vec<_>>(),
            [("cert_expires", true), ("opened_by", true)],
            "by key, and every one of them the reader's own"
        );
        assert_eq!(
            listed[0].label, "cert_expires",
            "a custom key labels itself"
        );
        // And the same two rules a bag that is not a bag gets.
        assert!(custom_properties(&serde_json::json!("not an object")).is_empty());
        assert_eq!(
            custom_properties(&serde_json::json!({ "port": [] }))[0].value,
            None,
            "an unreadable value reads as unset"
        );
    }

    /// A route's property values are vetted the way an asset's are, and
    /// against no schema.
    ///
    /// Both halves matter: the *value* rules are about what can be read back
    /// and apply everywhere, and the *schema* rule cannot apply here because
    /// a route has no type -- so a key called `port` may be text on one route
    /// and a number on another, which is what an asset's declared `port`
    /// may not be.
    #[test]
    fn a_routes_property_is_vetted_for_its_value_and_against_no_schema() {
        assert!(
            vet_custom_properties(&[("port".to_owned(), text("8080"))]).is_ok(),
            "no type declares a route's `port`, so text fits"
        );
        assert!(
            vet_custom_properties(&[("port".to_owned(), PropertyValue::Number { value: 8080.0 })])
                .is_ok()
        );
        let blank = vet_custom_properties(&[("note".to_owned(), text("  "))]).unwrap_err();
        assert_eq!(blank.code, crate::IpcErrorCode::Invalid);
        assert!(
            vet_custom_properties(&[(" ".to_owned(), text("x"))]).is_err(),
            "a property needs a key"
        );
    }

    // -----------------------------------------------------------------
    // #431: what is inherited, and what rolls up
    // -----------------------------------------------------------------

    /// A column row with nothing on it but the fields the walk reads.
    fn walked(id: &str, environment: Option<Environment>, owner: Option<&str>) -> AssetRow {
        AssetRow {
            id: format!("asset:{id}"),
            parent_id: None,
            type_id: "vm".to_owned(),
            type_label: "VM".to_owned(),
            monogram: "VM".to_owned(),
            name: id.to_owned(),
            status: AssetStatus::None,
            environment,
            owner: owner.map(str::to_owned),
            has_children: false,
            health: AssetStatus::None,
            inside: AssetStatus::None,
            problems_inside: 0,
            linked_work: 0,
        }
    }

    /// Story 8's three sentences over one path, and the negative under each.
    ///
    /// The path is **site → VM → container**, outermost first, which is the
    /// order `AssetDetail::held_by` arrives in -- and three levels rather than
    /// two because a two-level path cannot tell "nearest ancestor" from "any
    /// ancestor": both answers are the same row.
    #[test]
    fn a_value_comes_from_the_nearest_asset_at_or_above_that_sets_it() {
        let site = walked("hel1", Some(Environment::Prod), Some("Björn"));
        let vm = walked("vm-db-01", None, None);
        let path = [site.clone(), vm.clone()];

        // Nothing set on the container: the *nearest* setter is the site,
        // because the VM sets nothing.
        let container = walked("postgres", None, None);
        let from = inherited(&container, &path, |row| row.environment).expect("inherited");
        assert_eq!(from.value, Environment::Prod);
        assert_eq!(from.source_id, site.id, "the site is where it is set");
        assert_eq!(from.source_name, "hel1");

        // The VM overriding it wins for everything under the VM, and the
        // site's value is never consulted -- which is what makes this
        // "nearest", not "outermost".
        let overridden = [
            site.clone(),
            walked("vm-db-01", Some(Environment::Dev), None),
        ];
        let from = inherited(&container, &overridden, |row| row.environment).expect("inherited");
        assert_eq!(from.value, Environment::Dev);
        assert_eq!(from.source_id, vm.id);

        // Set on the asset itself: the source is the asset, which is how the
        // pane tells "set here" from "inherited from".
        let own = walked("postgres", Some(Environment::Stage), None);
        let from = inherited(&own, &overridden, |row| row.environment).expect("inherited");
        assert_eq!(from.value, Environment::Stage);
        assert_eq!(from.source_id, own.id);

        // Owner walks the same field and answers with a name, not an enum.
        let from = inherited(&container, &path, |row| row.owner.clone()).expect("inherited");
        assert_eq!(from.value, "Björn");
        assert_eq!(from.source_id, site.id);

        // Nobody up the path sets one, which is a `null` on the wire rather
        // than a default: "not set anywhere" is a fact the pane draws.
        let nowhere = [walked("hel1", None, None)];
        assert!(inherited(&container, &nowhere, |row| row.environment).is_none());
        assert!(inherited(&container, &[], |row| row.owner.clone()).is_none());
    }

    /// [`ROLLUP`]'s `case` arms and [`AssetStatus::severity`] are one ordering
    /// in two languages.
    ///
    /// The statement is read out of this file rather than the numbers being
    /// listed here: a hand-written copy is the remembered-list trap, and the
    /// thing that can silently go wrong is exactly that somebody edits the SQL
    /// and not the enum. Both `case` expressions are checked, because the
    /// second one -- the `inside` half -- carries the same four arms and could
    /// be edited on its own.
    #[test]
    fn the_rollup_ranks_the_statuses_the_way_rust_does() {
        let mut seen = 0;
        let mut rest = ROLLUP;
        while let Some(at) = rest.find("case d.status") {
            rest = &rest[at + "case d.status".len()..];
            let arms = rest
                .split_once(" end")
                .unwrap_or_else(|| panic!("a `case d.status` in ROLLUP never ends"))
                .0;
            for status in AssetStatus::ALL {
                if status == AssetStatus::None {
                    // `none` is the `else` arm; there is no `when` for it.
                    continue;
                }
                assert!(
                    arms.contains(&format!(
                        "when '{}' then {}",
                        status.as_str(),
                        status.severity()
                    )),
                    "ROLLUP ranks {:?} differently from AssetStatus::severity: {arms}",
                    status.as_str()
                );
            }
            assert!(
                arms.contains(&format!("else {}", AssetStatus::None.severity())),
                "ROLLUP's else arm is not `none`'s severity: {arms}"
            );
            seen += 1;
        }
        assert_eq!(seen, 2, "ROLLUP has a `health` case and an `inside` case");
        // And the trip back, which is what turns the statement's answer into a
        // status again.
        for status in AssetStatus::ALL {
            assert_eq!(AssetStatus::of_severity(status.severity()), status);
        }
        // Worse is smaller, which is what makes `min` the worst-of.
        assert!(AssetStatus::Down.severity() < AssetStatus::Warn.severity());
        assert!(AssetStatus::Warn.severity() < AssetStatus::Up.severity());
        assert!(AssetStatus::Up.severity() < AssetStatus::None.severity());
    }

    /// The badge counts only what the statement counts.
    ///
    /// Pinned as prose because the count lives in SQL: `problems_inside` is
    /// `warn` and `down` and neither of the other two, and it is over `depth >
    /// 0` so an asset is never a problem inside itself.
    #[test]
    fn the_rollup_counts_only_what_is_underneath_and_only_the_two_bad_statuses() {
        assert!(
            ROLLUP.contains("count(*) filter (where u.depth > 0 and d.status in ('warn','down'))"),
            "ROLLUP no longer counts warn and down strictly underneath: {ROLLUP}"
        );
        assert!(
            ROLLUP.contains("min(case when u.depth = 0 then 3"),
            "ROLLUP's `inside` no longer excludes the asset itself: {ROLLUP}"
        );
    }

    /// The estate file's own checker (`knobas-core`'s `tests/estate_file.rs`)
    /// and the shapes this module deserialises read **the same keys**.
    ///
    /// The two are the format, written twice in two crates that cannot see
    /// each other: `knobas-core` cannot depend on `knobas-app`, so the closed
    /// key vocabulary over there and `deny_unknown_fields` over here are the
    /// only two statements of what an entry may say. A key added to one alone
    /// is either a fact the import drops on the floor (added there) or a file
    /// the checker calls illegal and the import happily reads (added here) --
    /// and the checker says as much in its own words: *"if it is new, add it
    /// here and teach the import about it"*.
    ///
    /// The field names are read **out of serde's own refusal**, which lists
    /// them: `deny_unknown_fields` renders "unknown field `zzz`, expected one
    /// of `id`, `type`, …". That is the deserialiser's own answer rather than
    /// a third list here, which is the whole point -- a `#[serde(rename)]`
    /// moves the name in the error too.
    #[test]
    fn the_file_shapes_read_the_keys_the_estate_files_own_check_allows() {
        const CHECK: &str = include_str!("../../../knobas-core/tests/estate_file.rs");

        /// The quoted strings of a `const NAME: &[&str] = &[ … ];` list.
        fn listed(source: &str, name: &str) -> BTreeSet<String> {
            let (_, rest) = source
                .split_once(&format!("const {name}: &[&str] = &["))
                .unwrap_or_else(|| panic!("estate_file.rs no longer declares {name}"));
            let (body, _) = rest
                .split_once("];")
                .unwrap_or_else(|| panic!("{name} does not close its list"));
            let keys: BTreeSet<String> = body
                .split('"')
                .skip(1)
                .step_by(2)
                .map(str::to_owned)
                .collect();
            assert!(!keys.is_empty(), "this parse read nothing out of {name}");
            keys
        }

        /// The fields serde says a shape accepts, read out of its own refusal.
        fn accepted<'de, T: serde::Deserialize<'de>>(what: &str) -> BTreeSet<String> {
            let refusal = serde_json::from_str::<T>(r#"{"zzz":1}"#)
                .err()
                .unwrap_or_else(|| panic!("{what} accepted a key it does not declare"))
                .to_string();
            let (_, rest) = refusal
                .split_once("expected one of ")
                .unwrap_or_else(|| panic!("serde no longer lists the fields: {refusal}"));
            let fields: BTreeSet<String> = rest
                .split('`')
                .skip(1)
                .step_by(2)
                .map(str::to_owned)
                .collect();
            assert!(
                !fields.is_empty(),
                "this parse read nothing out of {refusal}"
            );
            fields
        }

        assert_eq!(
            accepted::<FileAsset>("FileAsset"),
            listed(CHECK, "ASSET_KEYS"),
            "the estate file's asset keys and the import's asset shape disagree"
        );
        assert_eq!(
            accepted::<FileRoute>("FileRoute"),
            listed(CHECK, "ROUTE_KEYS"),
            "the estate file's route keys and the import's route shape disagree"
        );
    }

    /// The checked-in estate parses, and its plain scalars become the tagged
    /// values everything downstream carries.
    ///
    /// The file is the only asset fixture there is (ADR-0013), and this is the
    /// half of reading it that needs no database: a `2` becomes a number and a
    /// `"cx23"` becomes text, which is the translation #428 left to this
    /// ticket. The `description` landing in the bag is here too, because it is
    /// the one field of the format with nowhere of its own to go.
    #[test]
    fn the_checked_in_estate_parses_and_its_scalars_become_tagged_values() {
        const ESTATE: &str = include_str!("../../../../testenv/hetzner/estate.json");
        let parsed: EstateFile = serde_json::from_str(ESTATE).expect("the real estate file");
        assert_eq!(parsed.assets.len(), 23, "the estate file's assets");
        assert_eq!(parsed.routes.len(), 9, "the estate file's routes");

        let server = parsed
            .assets
            .iter()
            .find(|asset| asset.id == "asset:hetzner-teamcity")
            .expect("the TeamCity host is in the estate file");
        let bag = bag_of(
            Some(vet_type(&server.type_id).expect("`vm` is a type")),
            &server.properties,
            server.description.as_deref(),
        )
        .expect("the server's properties translate");
        assert_eq!(
            bag["vcpu"],
            serde_json::json!({ "kind": "number", "value": 2.0 }),
            "a JSON number is a number property"
        );
        assert_eq!(
            bag["server_type"],
            serde_json::json!({ "kind": "text", "value": "cx23" }),
            "a JSON string on an undeclared key is a text property"
        );
        assert!(
            bag[DESCRIPTION_KEY]["value"]
                .as_str()
                .is_some_and(|text| text.contains("SSH")),
            "the entry's description is a text property of its own: {:?}",
            bag[DESCRIPTION_KEY]
        );
        // The negative control for the two above: a declared key takes the
        // kind its *type* declares, and `os` is one -- so a table that
        // answered `text` to everything would pass the pair and fail nothing.
        assert_eq!(
            bag["os"],
            serde_json::json!({ "kind": "text", "value": "ubuntu-24.04" })
        );
    }

    /// A value whose JSON shape is not the kind the type declares is refused,
    /// and so is one JSON has no property spelling for.
    #[test]
    fn a_file_value_that_is_not_the_kind_its_type_declares_is_refused() {
        let scenario = vet_type("scenario").expect("`scenario` is a type");
        // `last_run` is the table's one date, and `position` (on `step`) its
        // one number -- so these are the two arms that only a real declared
        // kind can reach.
        assert!(
            bag_of(
                Some(scenario),
                &serde_json::from_str(r#"{"last_run":"yesterday"}"#).unwrap(),
                None,
            )
            .is_err(),
            "a declared date takes a date"
        );
        assert_eq!(
            bag_of(
                Some(scenario),
                &serde_json::from_str(r#"{"last_run":"2026-09-06"}"#).unwrap(),
                None,
            )
            .expect("a real date")["last_run"],
            serde_json::json!({ "kind": "date", "value": "2026-09-06" })
        );
        let step = vet_type("step").expect("`step` is a type");
        assert!(
            bag_of(
                Some(step),
                &serde_json::from_str(r#"{"position":"third"}"#).unwrap(),
                None,
            )
            .is_err(),
            "a declared number takes a number"
        );
        for shape in [
            r#"{"k":true}"#,
            r#"{"k":["a"]}"#,
            r#"{"k":null}"#,
            r#"{"k":{}}"#,
        ] {
            assert!(
                bag_of(None, &serde_json::from_str(shape).unwrap(), None).is_err(),
                "{shape} is not a plain scalar"
            );
        }
    }

    /// The file's assets come back parent-first, and a file that holds itself
    /// is refused rather than ordered.
    ///
    /// The ordering is what lets the apply write `parent_id` straight into the
    /// insert, and the refusal is what keeps a cycle away from
    /// `recompute_paths`' uncapped recursive CTE -- the two are one pass, so
    /// they are asserted together.
    #[test]
    fn the_files_assets_are_ordered_parent_first_and_a_cycle_is_refused() {
        let file: EstateFile = serde_json::from_str(
            r#"{"assets":[
                 {"id":"asset:leaf","type":"container","name":"leaf","parent":"asset:mid"},
                 {"id":"asset:mid","type":"vm","name":"mid","parent":"asset:root"},
                 {"id":"asset:root","type":"site","name":"root"}]}"#,
        )
        .expect("a small file");
        let nothing_yet = HashSet::new();
        assert_eq!(
            ordered(&file.assets, &nothing_yet)
                .expect("a tree orders")
                .iter()
                .map(|asset| asset.id.as_str())
                .collect::<Vec<_>>(),
            ["asset:root", "asset:mid", "asset:leaf"],
            "the file lists a leaf first and the order is the tree's"
        );

        let loop_file: EstateFile = serde_json::from_str(
            r#"{"assets":[
                 {"id":"asset:a","type":"site","name":"a","parent":"asset:b"},
                 {"id":"asset:b","type":"site","name":"b","parent":"asset:a"}]}"#,
        )
        .expect("a small file");
        let refusal = ordered(&loop_file.assets, &nothing_yet).expect_err("a cycle has no order");
        assert!(
            refusal.message.contains("asset:a") && refusal.message.contains("asset:b"),
            "the refusal names both ends: {}",
            refusal.message
        );

        // A subtree hung under an asset the estate already holds and the file
        // does not describe. `REFERENCED_ASSETS` is the read that finds such a
        // parent, and this is the half of it that decides whether the file can
        // be written at all: without the estate's own ids here, a legal file
        // waits for a parent that is never placed and is refused as a cycle it
        // does not have.
        let under_the_estate: EstateFile = serde_json::from_str(
            r#"{"assets":[
                 {"id":"asset:new","type":"vm","name":"new","parent":"asset:by-hand"}]}"#,
        )
        .expect("a small file");
        assert!(
            ordered(&under_the_estate.assets, &nothing_yet).is_err(),
            "a parent that is nowhere is not orderable"
        );
        assert_eq!(
            ordered(&under_the_estate.assets, &HashSet::from(["asset:by-hand"]))
                .expect("a parent already in the estate is a parent nothing waits for")
                .len(),
            1
        );
    }
}
