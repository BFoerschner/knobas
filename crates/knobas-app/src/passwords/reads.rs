//! The Passwords page's two reads (ADR-0056).
//!
//! **One read holds the page.** [`held`] answers every password source, every
//! folder and one compact row per entry: 299 KB at 2,000 entries. The window
//! keeps it, and a folder press or a filter keystroke asks nothing. The rest
//! of one entry is [`entry`].
//!
//! **The single shared root.** Per source, where exactly one folder has no
//! parent, that folder is skipped and its children are the source's top, so
//! Pleasant's `Tidewater/…` reads as `Payments / …`.

use std::collections::{HashMap, HashSet};

use knobas_source::passwords::{
    self, Approval, Attachments, CommentRules, EntryRecord, FieldRecord, Offers, OtpInfo,
};
use serde::Serialize;
use sqlx::PgPool;

/// One password source, as the page names it.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct PwSource {
    pub id: String,
    pub name: String,
    /// The account it signs in as: the name the source's log keeps reads under.
    pub account: Option<String>,
    pub by_hand: bool,
    /// When its last run that succeeded ended, RFC 3339.
    pub synced_at: Option<String>,
    /// When it last failed to answer: the last run ended unreachable, or a
    /// live read just did. `None` once a call to it succeeds.
    pub down_at: Option<String>,
    /// It has never finished a run.
    pub never_synced: bool,
    pub offers: Offers,
    /// The folder every other one of its sat under, left out.
    pub skipped_root: Option<String>,
    /// Its kind's display name, for Settings and nowhere on the page.
    pub adapter_name: String,
}

/// One folder.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct PwFolder {
    pub id: String,
    /// The index of its source in [`Held::sources`].
    pub source: u16,
    /// Its parent folder, `None` at its source's top.
    pub parent: Option<String>,
    pub name: String,
    pub comment_read: bool,
    pub can_add: bool,
}

/// An entry's row bit: it has a one-time code.
pub const HAS_CODE: u8 = 1;
/// It waits for an approval.
pub const NEEDS_APPROVAL: u8 = 2;
/// A read asks for a usage comment.
pub const COMMENT_REQUIRED: u8 = 4;
/// The account may not read its password.
pub const CANNOT_READ: u8 = 8;

/// One entry's row: `[id, source index, folder id, title, username, address,
/// changed (RFC 3339), flags, expires (a date), also in]`. An array, so 2,000
/// rows stay small on the bridge.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct PwRow(
    pub String,
    pub u16,
    pub Option<String>,
    pub String,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub u8,
    pub Option<String>,
    pub Vec<String>,
);

/// Everything the page draws from.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct Held {
    pub sources: Vec<PwSource>,
    pub folders: Vec<PwFolder>,
    pub entries: Vec<PwRow>,
    /// The entries used last, newest first, at most four.
    pub recent: Vec<String>,
}

/// The rest of one entry.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct PwEntry {
    pub id: String,
    pub source: String,
    pub title: String,
    pub username: Option<String>,
    pub address: Option<String>,
    pub folder: Option<String>,
    pub notes: Option<String>,
    pub fields: Vec<FieldRecord>,
    pub otp: Option<OtpInfo>,
    #[ts(type = "string | null")]
    pub expires: Option<chrono::NaiveDate>,
    pub changed: Option<String>,
    pub changed_by: Option<String>,
    pub comment: CommentRules,
    pub approval: Approval,
    pub attachments: Attachments,
    pub can_read: bool,
    pub can_modify: bool,
}

/// The `knobas.setting` key the recent entries are kept under: ids only.
pub const RECENT_KEY: &str = "passwords.recent";
/// How many recent entries the page shows.
pub const RECENT: usize = 4;

fn rfc3339(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(sqlx::FromRow)]
struct SourceRow {
    id: String,
    kind: String,
    display_name: String,
    config: serde_json::Value,
    sync_by_hand: bool,
    synced_at: Option<chrono::DateTime<chrono::Utc>>,
    last_outcome: Option<String>,
    last_finished: Option<chrono::DateTime<chrono::Utc>>,
    auth_state: String,
    auth_checked_at: Option<chrono::DateTime<chrono::Utc>>,
}

const SOURCES: &str = "
select c.id, c.kind, c.display_name, c.config, c.sync_by_hand,
       ok.finished_at as synced_at, f.outcome as last_outcome, f.finished_at as last_finished,
       c.auth_state, c.auth_checked_at
  from knobas.source_config c
  left join lateral (
      select finished_at from knobas.sync_run
       where source_id = c.id and outcome = 'ok' and finished_at is not null
       order by finished_at desc limit 1) ok on true
  left join lateral (
      select outcome, finished_at from knobas.sync_run
       where source_id = c.id and finished_at is not null
       order by finished_at desc limit 1) f on true
 where c.id = any($1)
 order by c.id";

#[derive(sqlx::FromRow)]
struct ItemRow {
    entity_id: String,
    source_id: String,
    kind: String,
    title: String,
    item_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    payload: serde_json::Value,
}

/// Fixed keys of a small payload; `sync.item` narrowed by its source index.
const ITEMS: &str = "
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.payload
  from sync.item i
  join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
 where i.source_id = any($1) and i.kind = any($2)";

/// What [`held`] needs besides the database: the fillers, their templates,
/// and the sources a live read just found down.
pub struct Ask<'a> {
    pub fillers: &'a [String],
    pub templates: &'a [knobas_source::SourceDescriptor],
    pub down: &'a HashMap<String, chrono::DateTime<chrono::Utc>>,
}

/// A row of the payload as the page reads it: leniently, so a record that
/// grew a field later still reads (risk 8 of the design).
fn lenient<T: serde::de::DeserializeOwned + Default>(payload: &serde_json::Value) -> T {
    serde_json::from_value(payload.clone()).unwrap_or_default()
}

/// Every password source, folder and entry.
///
/// # Errors
/// The database's.
pub async fn held(pool: &PgPool, ask: Ask<'_>) -> Result<Held, sqlx::Error> {
    let sources: Vec<SourceRow> = sqlx::query_as(SOURCES)
        .bind(ask.fillers)
        .fetch_all(pool)
        .await?;
    let kinds = [passwords::ENTRY_KIND, passwords::FOLDER_KIND];
    let items: Vec<ItemRow> = sqlx::query_as(ITEMS)
        .bind(ask.fillers)
        .bind(&kinds[..])
        .fetch_all(pool)
        .await?;
    let recent = recent(pool).await?;
    Ok(assemble(&sources, &items, &ask, recent))
}

fn assemble(sources: &[SourceRow], items: &[ItemRow], ask: &Ask<'_>, recent: Vec<String>) -> Held {
    let index: HashMap<&str, u16> = sources
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), u16::try_from(i).unwrap_or(u16::MAX)))
        .collect();
    // Folders first: the shared root of each source is found among them.
    let mut folders: Vec<PwFolder> = Vec::new();
    for item in items.iter().filter(|i| i.kind == passwords::FOLDER_KIND) {
        let Some(&source) = index.get(item.source_id.as_str()) else {
            continue;
        };
        let record: passwords::FolderRecord = lenient(&item.payload);
        folders.push(PwFolder {
            id: item.entity_id.clone(),
            source,
            parent: record.parent,
            name: item.title.clone(),
            comment_read: record.comment.read,
            can_add: record.can_add,
        });
    }
    let mut skipped: HashMap<u16, String> = HashMap::new();
    for source in 0..u16::try_from(sources.len()).unwrap_or(u16::MAX) {
        let tops: Vec<&PwFolder> = folders
            .iter()
            .filter(|f| f.source == source && f.parent.is_none())
            .collect();
        if let [only] = tops[..] {
            skipped.insert(source, only.id.clone());
        }
    }
    let gone: HashSet<&String> = skipped.values().collect();
    let lift = |parent: Option<String>| parent.filter(|p| !gone.contains(p));
    let folders: Vec<PwFolder> = folders
        .iter()
        .filter(|f| !gone.contains(&f.id))
        .map(|f| PwFolder {
            parent: lift(f.parent.clone()),
            ..f.clone()
        })
        .collect();

    let mut entries = Vec::new();
    for item in items.iter().filter(|i| i.kind == passwords::ENTRY_KIND) {
        let Some(&source) = index.get(item.source_id.as_str()) else {
            continue;
        };
        let record: EntryRecord = lenient(&item.payload);
        let mut flags = 0;
        if record.otp.is_some() {
            flags |= HAS_CODE;
        }
        if matches!(record.approval, Approval::Required { .. }) {
            flags |= NEEDS_APPROVAL;
        }
        if record.comment.read {
            flags |= COMMENT_REQUIRED;
        }
        if !record.can_read {
            flags |= CANNOT_READ;
        }
        entries.push(PwRow(
            item.entity_id.clone(),
            source,
            lift(record.folder),
            item.title.clone(),
            record.username,
            record.address,
            item.item_updated_at.map(rfc3339),
            flags,
            record.expires.map(|d| d.to_string()),
            record
                .also_in
                .into_iter()
                .filter_map(|f| lift(Some(f)))
                .collect(),
        ));
    }

    let sources = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let template = ask.templates.iter().find(|t| t.adapter_kind == s.kind);
            let ran_down = s.last_outcome.as_deref() == Some("unreachable");
            let health_down = s.auth_state == "unreachable";
            let down_at = ask
                .down
                .get(&s.id)
                .copied()
                .or(if ran_down { s.last_finished } else { None })
                .or(if health_down { s.auth_checked_at } else { None });
            PwSource {
                id: s.id.clone(),
                name: s.display_name.clone(),
                account: s
                    .config
                    .get("username")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                by_hand: s.sync_by_hand,
                synced_at: s.synced_at.map(rfc3339),
                down_at: down_at.map(rfc3339),
                never_synced: s.last_finished.is_none(),
                offers: template
                    .and_then(|t| t.passwords.clone())
                    .unwrap_or_default(),
                skipped_root: u16::try_from(i).ok().and_then(|i| skipped.get(&i).cloned()),
                adapter_name: template.map_or_else(|| s.kind.clone(), |t| t.name.clone()),
            }
        })
        .collect();
    Held {
        sources,
        folders,
        entries,
        recent,
    }
}

/// The rest of one entry, or `None` for an entity that is no live entry.
///
/// # Errors
/// The database's.
pub async fn entry(pool: &PgPool, entity_id: &str) -> Result<Option<PwEntry>, sqlx::Error> {
    let row: Option<ItemRow> = sqlx::query_as(
        "select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.payload
           from sync.item i
           join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
          where i.entity_id = $1 and i.kind = $2",
    )
    .bind(entity_id)
    .bind(passwords::ENTRY_KIND)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|item| {
        let r: EntryRecord = lenient(&item.payload);
        PwEntry {
            id: item.entity_id,
            source: item.source_id,
            title: item.title,
            username: r.username,
            address: r.address,
            folder: r.folder,
            notes: r.notes,
            fields: r.fields,
            otp: r.otp,
            expires: r.expires,
            changed: item.item_updated_at.map(rfc3339),
            changed_by: r.changed_by,
            comment: r.comment,
            approval: r.approval,
            attachments: r.attachments,
            can_read: r.can_read,
            can_modify: r.can_modify,
        }
    }))
}

/// What a password source's Settings page counts (ADR-0056): its entries
/// and folders, and how many entries keep a code key in each format the
/// sync found (`payload->'otp'->>'format'`), `unreadable` included.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct SourceFacts {
    #[ts(type = "number")]
    pub entries: i64,
    #[ts(type = "number")]
    pub folders: i64,
    /// Format id and how many entries have it, most first.
    #[ts(type = "Array<[string, number]>")]
    pub formats: Vec<(String, i64)>,
}

/// [`SourceFacts`] for `source_id`.
///
/// # Errors
/// The database's.
pub async fn source_facts(pool: &PgPool, source_id: &str) -> Result<SourceFacts, sqlx::Error> {
    let (entries, folders): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where i.kind = $2), count(*) filter (where i.kind = $3)
           from sync.item i
           join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
          where i.source_id = $1",
    )
    .bind(source_id)
    .bind(passwords::ENTRY_KIND)
    .bind(passwords::FOLDER_KIND)
    .fetch_one(pool)
    .await?;
    let formats: Vec<(String, i64)> = sqlx::query_as(
        "select i.payload->'otp'->>'format' as format, count(*)
           from sync.item i
           join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
          where i.source_id = $1 and i.kind = $2 and i.payload->'otp' is not null
            and jsonb_typeof(i.payload->'otp') = 'object'
          group by 1 order by 2 desc, 1",
    )
    .bind(source_id)
    .bind(passwords::ENTRY_KIND)
    .fetch_all(pool)
    .await?;
    Ok(SourceFacts {
        entries,
        folders,
        formats,
    })
}

/// The recent entries, newest first.
///
/// # Errors
/// The database's.
pub async fn recent(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let stored = crate::settings::stored_value(pool, RECENT_KEY).await?;
    Ok(stored
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default())
}

/// Put `entity_id` first among the recent entries. Ids only, so no title or
/// value reaches `knobas.setting`.
///
/// # Errors
/// The database's.
pub async fn used(pool: &PgPool, entity_id: &str) -> Result<(), sqlx::Error> {
    let mut list = recent(pool).await?;
    list.retain(|id| id != entity_id);
    list.insert(0, entity_id.to_owned());
    list.truncate(RECENT);
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(RECENT_KEY)
    .bind(serde_json::json!(list))
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> SourceRow {
        SourceRow {
            id: id.into(),
            kind: "pleasant".into(),
            display_name: "Team passwords".into(),
            config: serde_json::json!({"username": "mara"}),
            sync_by_hand: true,
            synced_at: None,
            last_outcome: Some("unreachable".into()),
            last_finished: Some(chrono::Utc::now()),
            auth_state: "ok".into(),
            auth_checked_at: None,
        }
    }

    fn folder(source: &str, key: &str, parent: Option<&str>, name: &str) -> ItemRow {
        ItemRow {
            entity_id: format!("{source}:{key}"),
            source_id: source.into(),
            kind: passwords::FOLDER_KIND.into(),
            title: name.into(),
            item_updated_at: None,
            payload: serde_json::to_value(passwords::FolderRecord {
                name: name.into(),
                parent: parent.map(|p| format!("{source}:{p}")),
                ..passwords::FolderRecord::default()
            })
            .unwrap(),
        }
    }

    fn entry_in(source: &str, key: &str, folder: Option<&str>, record: EntryRecord) -> ItemRow {
        ItemRow {
            entity_id: format!("{source}:{key}"),
            source_id: source.into(),
            kind: passwords::ENTRY_KIND.into(),
            title: key.into(),
            item_updated_at: None,
            payload: serde_json::to_value(EntryRecord {
                folder: folder.map(|f| format!("{source}:{f}")),
                ..record
            })
            .unwrap(),
        }
    }

    #[test]
    fn a_single_shared_root_is_skipped_and_flags_say_what_a_row_needs() {
        let sources = [source("pw")];
        let items = [
            folder("pw", "root", None, "Tidewater"),
            folder("pw", "pay", Some("root"), "Payments"),
            folder("pw", "db", Some("pay"), "Databases"),
            entry_in(
                "pw",
                "e1",
                Some("db"),
                EntryRecord {
                    otp: Some(OtpInfo {
                        format: "keepass".into(),
                        digits: 6,
                        period: 30,
                        algorithm: passwords::OtpAlgorithm::Sha1,
                    }),
                    comment: CommentRules {
                        read: true,
                        modify: false,
                    },
                    can_read: false,
                    approval: Approval::Required { approver: None },
                    ..EntryRecord::default()
                },
            ),
            entry_in(
                "pw",
                "e2",
                Some("root"),
                EntryRecord {
                    can_read: true,
                    ..EntryRecord::default()
                },
            ),
        ];
        let down = HashMap::new();
        let held = assemble(
            &sources,
            &items,
            &Ask {
                fillers: &[],
                templates: &[],
                down: &down,
            },
            vec![],
        );
        assert_eq!(held.sources[0].skipped_root.as_deref(), Some("pw:root"));
        assert_eq!(
            held.folders
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            ["pw:pay", "pw:db"]
        );
        assert_eq!(held.folders[0].parent, None, "Payments is the source's top");
        assert_eq!(held.folders[1].parent.as_deref(), Some("pw:pay"));
        let e1 = held.entries.iter().find(|r| r.0 == "pw:e1").unwrap();
        assert_eq!(
            e1.7,
            HAS_CODE | NEEDS_APPROVAL | COMMENT_REQUIRED | CANNOT_READ
        );
        let e2 = held.entries.iter().find(|r| r.0 == "pw:e2").unwrap();
        assert_eq!(e2.2, None, "an entry in the skipped root sits at the top");
        assert_eq!(e2.7, 0);
        assert!(
            held.sources[0].down_at.is_some(),
            "its last run ended unreachable"
        );
        assert_eq!(held.sources[0].account.as_deref(), Some("mara"));
    }

    #[test]
    fn two_top_folders_keep_both() {
        let sources = [source("pw")];
        let items = [folder("pw", "a", None, "A"), folder("pw", "b", None, "B")];
        let down = HashMap::new();
        let held = assemble(
            &sources,
            &items,
            &Ask {
                fillers: &[],
                templates: &[],
                down: &down,
            },
            vec![],
        );
        assert_eq!(held.sources[0].skipped_root, None);
        assert_eq!(held.folders.len(), 2);
    }
}
