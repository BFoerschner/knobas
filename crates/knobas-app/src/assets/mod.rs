//! The estate: assets, the tree they sit in, and the eight commands that read
//! and edit it (spec #427 "M4.0 Estate", issues #428, #429, #431 and #434).
//!
//! Six of the eight landed with #428; `asset_types` joined them with #429,
//! when the create dialog gave the built-in table a reader, and
//! `context_assets` with #434, when the room's Assets tile did.
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
//! # What this module deliberately does not do yet
//!
//! The create/edit surface (#429) and the keyboard walk and spines (#430)
//! have landed, and so have the routes above. [`in_context`] serves a *stored*
//! room; the Assets tile's rule for the derived rooms -- *All work*'s top
//! level, a source room's monitored assets, a project room's nothing -- is
//! **#435**. A route is **not moved between exposing assets** -- there is no
//! `move_route` -- because a route is the address *of* the thing that answers
//! it: re-exposing one somewhere else is a different route with a different
//! history, which is a delete and a create. The wires story 31 draws between a
//! route row and its target are #433's.

use std::collections::HashMap;

use knobas_core::activity::ActivityRow;
use knobas_core::asset::{self, AssetType, PropertyKind};
use knobas_core::entity::EntityRef;
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
/// be bytes nobody asked for. [`MEMBER_ASSETS`] (#434) does select it, and
/// that is the whole reason it is a fourth statement rather than one of these
/// three: a room's Assets tile is a flat list of assets from anywhere in the
/// estate, so where each one sits is the column that makes it readable.
///
/// Four statements rather than one spliced constant: the SQL audit this repo
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

/// The properties bag, read on its own because it is the one column a column
/// row has no use for.
const PROPERTIES: &str = "select properties from knobas.asset where id = $1";

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
) -> Result<AssetRow, IpcError> {
    let id: String = row.try_get("id")?;
    let type_id: String = row.try_get("type_id")?;
    let declared = asset::find(&type_id);
    let status = AssetStatus::parse(&row.try_get::<String, _>("status")?)?;
    let environment: Option<String> = row.try_get("environment")?;
    let rolled = rollups.get(&id).copied();
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
    rows.iter().map(|row| row_of(row, &rollups)).collect()
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

    let stored: serde_json::Value = sqlx::query(PROPERTIES)
        .bind(id)
        .fetch_one(pool)
        .await?
        .try_get("properties")?;

    let exposes = routes(pool, ROUTES_EXPOSED, id).await?;
    let reachable_via = routes(pool, ROUTES_REACHABLE, id).await?;

    let entity = entity_of(id)?;
    let history = knobas_core::activity::recent(pool, HISTORY_LIMIT, Some(&entity)).await?;

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
    })
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
    let assets = rows_of(pool, &rows).await?;
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

    // `severity` is smaller-is-worse, so this is an ascending sort and the
    // worst row is first. The statement cannot do it: `health` is the rollup's
    // answer and the rollup is a read of its own.
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

    sqlx::query(
        "insert into knobas.entity (id, kind, title, updated_at) values ($1, $2, $3, now())",
    )
    .bind(&id)
    .bind(NAMESPACE)
    .bind(&name)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "insert into knobas.asset (id, parent_id, type_id, name, properties, path_text)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(parent_id)
    .bind(type_id)
    .bind(&name)
    .bind(serde_json::Value::Object(bag))
    .bind(&path_text)
    .execute(&mut *tx)
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

    exists(&mut tx, asset_id, "to expose a route on").await?;
    if let Some(target) = target_id {
        exists(&mut tx, target, "for a route to land on").await?;
    }

    sqlx::query(
        "insert into knobas.entity (id, kind, title, updated_at) values ($1, $2, $3, now())",
    )
    .bind(&id)
    .bind(ROUTE_NAMESPACE)
    .bind(&name)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "insert into knobas.route (id, asset_id, target_id, name, url, visibility, properties)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(asset_id)
    .bind(target_id)
    .bind(&name)
    .bind(&url)
    .bind(visibility.as_str())
    .bind(serde_json::Value::Object(bag))
    .execute(&mut *tx)
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
                    exists(&mut tx, target, "for a route to land on").await?;
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
// The plumbing the four writers share
// ---------------------------------------------------------------------------

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
/// `for update`, for [`path_below`]'s reason one row over: a route created
/// against an asset deleted in a transaction committing in between would leave
/// the insert failing on the foreign key with Postgres' own sentence instead
/// of this one. The lock is what makes the check and the insert one decision.
async fn exists(
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
const SET_ROUTE_VISIBILITY: &str = "update knobas.route set visibility = coalesce($2, 'internal'), updated_at = now() where id = $1";

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
    fn vocabulary<'m>(migration: &'m str, constraint: &str) -> Vec<&'m str> {
        let line = migration
            .lines()
            .find(|line| line.contains(constraint))
            .unwrap_or_else(|| panic!("{constraint} is missing from 0017"));
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
}
