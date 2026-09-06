//! The estate: assets, the tree they sit in, and the six commands that read
//! and edit it (spec #427 "M4.0 Estate", issue #428).
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
//! # What this module deliberately does not do yet
//!
//! *Effective* environment and owner -- the walk up the parent field to the
//! nearest ancestor that sets them -- and the "N problems inside" rollup are
//! **#431**. What is here is the stored value on the asset itself, which is
//! what that walk will read. Routes are **#432**, the create/edit surface is
//! **#429**, and the keyboard walk and spines are **#430**.

pub mod types;

use knobas_core::activity::ActivityRow;
use knobas_core::entity::EntityRef;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::IpcError;
use types::{AssetType, PropertyKind};

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
/// are computed at read time and both are #431's and M4.1's; this is the
/// stored fact they read.
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
/// `has_children` is what the column draws its chevron from and what tells the
/// Tree that there is a next column to open. It is computed by the read rather
/// than stored: a stored count is a second copy of the parent field, and the
/// two would disagree the first time a move failed halfway.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AssetNode {
    /// `asset:<uuid>`, and also the id of this asset's `knobas.entity` row.
    pub id: String,
    /// The asset that holds this one; `null` at the top of the estate.
    pub parent_id: Option<String>,
    /// One of [`types::TYPES`].
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

/// Everything the fixed right pane draws for one asset.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AssetDetail {
    pub asset: AssetNode,
    /// Known keys first, custom after. See [`AssetProperty`].
    pub properties: Vec<AssetProperty>,
    /// The path down to this asset, **outermost first and excluding itself**.
    /// Empty for an asset at the top of the estate.
    pub held_by: Vec<AssetNode>,
    /// What sits directly under it, in the column's own order.
    pub holds: Vec<AssetNode>,
    /// This asset's own lines from the activity stream, newest first.
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

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// The column list every read below selects, written out in each statement.
///
/// **Never `fts`**: reading a `tsvector` into a row panics at run time
/// (interfaces §1), which is the one rule `knobas_search::corpus` makes
/// structural and the reason it is repeated here in prose. `path_text` is left
/// out too -- the launcher reads it through the corpus and no surface in this
/// module draws it, so selecting it on every column of a Miller walk would be
/// bytes nobody asked for.
///
/// Three statements rather than one spliced constant: the SQL audit this repo
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

fn node_of(row: &sqlx::postgres::PgRow) -> Result<AssetNode, IpcError> {
    let type_id: String = row.try_get("type_id")?;
    let declared = types::find(&type_id);
    let status: String = row.try_get("status")?;
    let environment: Option<String> = row.try_get("environment")?;
    Ok(AssetNode {
        id: row.try_get("id")?,
        parent_id: row.try_get("parent_id")?,
        type_label: declared.map_or_else(|| type_id.clone(), |t| t.label.to_owned()),
        // A type the table has lost is drawn as `??` rather than refused: the
        // row exists, a reader has to be able to see it and move it, and the
        // one way that can happen is an estate file (#439) naming a type this
        // build does not carry.
        monogram: declared.map_or("??", |t| t.monogram).to_owned(),
        type_id,
        name: row.try_get("name")?,
        status: AssetStatus::parse(&status)?,
        environment: environment.as_deref().map(Environment::parse).transpose()?,
        owner: row.try_get("owner")?,
        has_children: row.try_get("has_children")?,
    })
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
pub async fn tree(pool: &PgPool, parent_id: Option<&str>) -> Result<Vec<AssetNode>, IpcError> {
    let rows = sqlx::query(CHILDREN)
        .bind(parent_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(node_of).collect()
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
        .ok_or_else(|| IpcError::not_found(format!("no asset {id}")))?;
    let asset = node_of(&row)?;

    let ancestors = sqlx::query(ANCESTORS).bind(id).fetch_all(pool).await?;
    let held_by = ancestors
        .iter()
        .map(node_of)
        .collect::<Result<Vec<_>, _>>()?;
    let holds = tree(pool, Some(id)).await?;

    let stored: serde_json::Value = sqlx::query(PROPERTIES)
        .bind(id)
        .fetch_one(pool)
        .await?
        .try_get("properties")?;

    let entity = EntityRef::parse(id).map_err(IpcError::internal)?;
    let history = knobas_core::activity::recent(pool, HISTORY_LIMIT, Some(&entity)).await?;

    Ok(AssetDetail {
        properties: properties_of(&asset.type_id, &stored),
        asset,
        held_by,
        holds,
        history,
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
    let declared = types::find(type_id);
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

    if let Some(bag) = bag {
        // `serde_json::Map` iterates in key order under the default feature
        // set, and the sort below does not depend on that: a custom list whose
        // order moved between reads would make the pane flicker.
        let mut custom: Vec<(&String, &serde_json::Value)> = bag
            .iter()
            .filter(|(key, _)| !claimed.contains(&key.as_str()))
            .collect();
        custom.sort_by(|left, right| left.0.cmp(right.0));
        for (key, raw) in custom {
            out.push(AssetProperty {
                key: key.clone(),
                label: key.clone(),
                value: serde_json::from_value(raw.clone()).ok(),
                custom: true,
            });
        }
    }

    out
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
) -> Result<Written<AssetNode>, IpcError> {
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

    let entity = EntityRef::parse(&id).map_err(IpcError::internal)?;
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
) -> Result<Written<AssetNode>, IpcError> {
    let mut tx = pool.begin().await?;
    let current = locked(&mut tx, id).await?;
    let declared = types::find(&current.type_id);
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
                set_column(&mut tx, id, "status", Some(value.as_str())).await?;
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
                set_column(&mut tx, id, "environment", value.map(Environment::as_str)).await?;
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
                set_column(&mut tx, id, "owner", value).await?;
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
                let after = value.as_ref().map(|v| serde_json::to_value(v).unwrap());
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
            let entity = EntityRef::parse(id).map_err(IpcError::internal)?;
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
) -> Result<Written<AssetNode>, IpcError> {
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

    let entity = EntityRef::parse(id).map_err(IpcError::internal)?;
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

/// Delete a **leaf**.
///
/// An asset that holds anything is refused with a `conflict` naming what it
/// holds: deleting a subtree by deleting its root is the one destructive
/// action nobody asks for twice, and `0017` leaves the foreign key with no
/// cascade as the floor under this refusal.
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

    sqlx::query("delete from knobas.asset where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update knobas.entity set deleted_at = now(), updated_at = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let entity = EntityRef::parse(id).map_err(IpcError::internal)?;
    let line = knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        "deleted",
        Some(&entity),
        serde_json::json!({ "asset": { "name": current.name, "type": current.type_id } }),
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

/// The stored row, as the writers need it: the wire node plus the two columns
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
    .ok_or_else(|| IpcError::not_found(format!("no asset {id}")))?;

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
async fn one(pool: &PgPool, id: &str) -> Result<AssetNode, IpcError> {
    let row = sqlx::query(ONE)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no asset {id}")))?;
    node_of(&row)
}

/// The three nullable text columns are set through one statement each, and the
/// column name is a `&'static str` chosen by the match above it -- never
/// anything a caller typed.
async fn set_column(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    column: &'static str,
    value: Option<&str>,
) -> Result<(), IpcError> {
    let statement = match column {
        "status" => {
            "update knobas.asset set status = coalesce($2, 'none'), updated_at = now() where id = $1"
        }
        "environment" => {
            "update knobas.asset set environment = $2, updated_at = now() where id = $1"
        }
        "owner" => "update knobas.asset set owner = $2, updated_at = now() where id = $1",
        other => return Err(IpcError::internal(format!("no such asset column {other}"))),
    };
    sqlx::query(statement)
        .bind(id)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The `path_text` a child of `parent` would carry.
async fn path_below(tx: &mut Transaction<'_, Postgres>, parent: &str) -> Result<String, IpcError> {
    let row = sqlx::query("select name, path_text from knobas.asset where id = $1")
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
/// One statement, because a subtree walked in Rust is a round trip per node
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

/// The name of the asset `moved` would have run into, or `None` for a legal
/// move.
///
/// Walks up from `parent`: if `id` is anywhere above it, `id` would end up
/// holding itself. The answer is the *name* rather than a bare `true`, so the
/// refusal can say which asset closed the loop.
async fn cycle_through(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    parent: &str,
) -> Result<Option<String>, IpcError> {
    let row = sqlx::query(
        "with recursive up as (
             select a.id, a.parent_id, a.name from knobas.asset a where a.id = $2
             union all
             select p.id, p.parent_id, p.name
               from knobas.asset p join up on p.id = up.parent_id
         )
         select name from up where id = $1",
    )
    .bind(id)
    .bind(parent)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| row.try_get::<String, _>("name"))
        .transpose()
        .map_err(Into::into)
}

fn vet_type(type_id: &str) -> Result<&'static AssetType, IpcError> {
    types::find(type_id).ok_or_else(|| {
        IpcError::invalid(format!(
            "{type_id:?} is not one of the {} built-in asset types",
            types::TYPES.len()
        ))
    })
}

fn vet_name(name: &str) -> Result<String, IpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid("an asset needs a name"));
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
        bag.insert(key.to_owned(), serde_json::to_value(value).unwrap());
    }
    Ok(bag)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIGRATION: &str =
        include_str!("../../../knobas-db/migrations/0017_the_estate_and_its_assets.sql");

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
        let service = types::find("service").unwrap();
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
    }
}
