//! Every interface `app/src/lib/ipc/entity.ts` declares, against the Rust it
//! claims to mirror.
//!
//! This file existed as a hole. `sources.ts` has had a shape test per DTO since
//! stream F landed, and `entity.ts` -- the larger mirror, and the one the room,
//! the detail slide-over and the status bar all read through -- had exactly one
//! pinned interface, `KindInfo`, and that only incidentally, because it rides
//! inside a `SourceDescriptor` in `sources_mirror.rs`. Seven interfaces were
//! hand-written prose that nothing compared to anything.
//!
//! # The rules the checks here follow
//!
//! **The exact key set, not a `contains` walk.** A test that looks for the
//! fields somebody thought to list cannot see a Rust field that has no
//! TypeScript counterpart, which is the direction this bridge breaks first.
//! [`knobas_sync::mirror::assert_shape`] compares three witnesses -- the
//! serialized key set, the literal list spelled out here, and the interface
//! body -- and fails on any disagreement in either direction.
//!
//! **Nullable fields are exercised as `None`.** `Option<T>` serializes to a
//! `null` *key*, and the mirror declares `T | null` on that promise. A
//! `skip_serializing_if` added to one of them would drop the key entirely and
//! hand the frontend `undefined` where it declared `null`; the fixtures below
//! leave every nullable field empty somewhere so that change cannot pass.
//!
//! **Unions are read out of the mirror, never listed here.** A hand-copied list
//! of members is the remembered-list trap one level down: it would pass while
//! both the union and the copy of it drifted from the Rust enum.

use chrono::{DateTime, TimeZone, Utc};
use knobas_app::commands::entity::{
    EntityDetail, EntityFilter, EntityOrder, EntityPage, EntityRow, SourceRef,
};
use knobas_core::activity::ActivityRow;
use knobas_core::link::{LinkEnd, LinkEntry, LinkRow, Origin};
use knobas_sync::mirror::{declared_inline_union, declared_union, interface_body};

const MIRROR: &str = include_str!("../../../app/src/lib/ipc/entity.ts");

/// The keys `value` serializes to must be exactly `expected`, and exactly what
/// `interface <name>` in `entity.ts` declares -- both directions.
fn assert_shape(name: &str, value: &serde_json::Value, expected: &[&str]) {
    knobas_sync::mirror::assert_shape(MIRROR, name, value, expected);
}

/// The spellings a Rust enum serializes to must be exactly the members the
/// mirror's union declares -- order-insensitively, since neither side's
/// ordering means anything.
///
/// Both unions on this mirror need this, and the second copy of it was the
/// beginning of the drift `knobas_sync::mirror` exists to stop.
fn assert_same_members(rust: &[&str], declared: Vec<String>, whats_at_stake: &str) {
    let mut rust: Vec<&str> = rust.to_vec();
    rust.sort_unstable();
    let mut declared = declared;
    declared.sort();
    assert_eq!(rust, declared, "{whats_at_stake}");
}

/// A fixed instant, so a fixture reads the same on every run.
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 28, 9, 30, 0).unwrap()
}

fn row(updated_at: Option<DateTime<Utc>>) -> EntityRow {
    EntityRow {
        entity_id: "mock:PAY-231".to_owned(),
        kind: "ticket".to_owned(),
        source_id: "mock".to_owned(),
        title: "Payments retry storm".to_owned(),
        updated_at,
        synced_at: at(),
    }
}

fn source_ref() -> SourceRef {
    SourceRef {
        id: "mock".to_owned(),
        display_name: "Tidewater Mock".to_owned(),
        adapter_kind: "mock".to_owned(),
    }
}

fn kind_info() -> knobas_source::KindInfo {
    knobas_source::KindInfo {
        id: "ticket".to_owned(),
        label: "Ticket".to_owned(),
        plural: "Tickets".to_owned(),
        monogram: "TI".to_owned(),
        full_sync_exhaustive: true,
    }
}

fn activity_row() -> ActivityRow {
    ActivityRow {
        id: 7,
        at: at(),
        actor: "user".to_owned(),
        verb: "linked".to_owned(),
        entity_id: None,
        detail: serde_json::json!({ "n": 1 }),
    }
}

/// A link with **no** note, because `note` is the nullable field on this row
/// and the file's second rule is that nullable fields are exercised as `None`:
/// a `skip_serializing_if` added to it would drop the key and hand the panel
/// `undefined` where the mirror promised `string | null`.
fn link_row() -> LinkRow {
    LinkRow {
        id: uuid::Uuid::nil(),
        from_id: "mock:PAY-231".to_owned(),
        to_id: "note:retry-storm".to_owned(),
        relation: "documents".to_owned(),
        origin: Origin::Manual,
        note: None,
        created_by: "mara".to_owned(),
        created_at: at(),
    }
}

const ACTIVITY_ROW_FIELDS: &[&str] = &["actor", "at", "detail", "entity_id", "id", "verb"];
const SOURCE_REF_FIELDS: &[&str] = &["adapter_kind", "display_name", "id"];
const KIND_INFO_FIELDS: &[&str] = &["full_sync_exhaustive", "id", "label", "monogram", "plural"];

const ENTITY_ROW_FIELDS: &[&str] = &[
    "entity_id",
    "kind",
    "source_id",
    "synced_at",
    "title",
    "updated_at",
];

/// A row that the source never dated, so `updated_at` is checked as the `null`
/// the mirror declares -- the room sorts on that hole (interfaces §4.1).
#[test]
fn the_entity_row_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(row(None)).unwrap();
    assert_shape("EntityRow", &wire, ENTITY_ROW_FIELDS);
    assert_eq!(
        wire["updated_at"],
        serde_json::Value::Null,
        "an undated row keeps the key and nulls it; dropping the key hands the \
         frontend `undefined` where the mirror promised `T | null`"
    );
}

#[test]
fn the_entity_page_shape_matches_its_typescript_mirror() {
    let page = EntityPage {
        rows: vec![row(Some(at()))],
        total: 42,
    };
    let wire = serde_json::to_value(page).unwrap();
    assert_shape("EntityPage", &wire, &["rows", "total"]);
    assert_shape("EntityRow", &wire["rows"][0], ENTITY_ROW_FIELDS);
}

/// The filter is an *input* DTO, so it is pinned through a round trip: the
/// object the mirror describes is decoded and then re-encoded, and the
/// re-encoding is what the shape assertion sees.
///
/// A serialize-only check cannot see an input DTO at all, and a decode-only
/// check cannot see a Rust field the mirror never declares -- serde would just
/// report a missing field for a payload nobody in production writes by hand.
/// The round trip is both halves at once: what comes back out is what the
/// backend actually understood, and a renamed field on either side stops it
/// matching.
#[test]
fn the_entity_filter_shape_matches_its_typescript_mirror() {
    let payload = serde_json::json!({
        "sources": ["mock"],
        "kinds": ["ticket", "pr"],
        "updated_within_days": null,
        "order": "title_asc",
        "include_deleted": true,
    });
    let decoded: EntityFilter =
        serde_json::from_value(payload.clone()).expect("the mirror's EntityFilter decodes");
    let wire = serde_json::to_value(&decoded).unwrap();

    assert_shape(
        "EntityFilter",
        &wire,
        &[
            "include_deleted",
            "kinds",
            "order",
            "sources",
            "updated_within_days",
        ],
    );
    assert_eq!(
        wire, payload,
        "every value the frontend sent survived the decode unchanged"
    );

    // The window is the one nullable field, and `null` there means *no window*
    // rather than zero days -- a decode that defaulted it to `Some(0)` would
    // silently empty every room.
    assert_eq!(decoded.updated_within_days, None);
    assert_eq!(decoded.order, EntityOrder::TitleAsc);
}

/// Both orderings, in the spelling the mirror's union declares.
///
/// Read out of `entity.ts` rather than listed here: an ordering added on one
/// side only is a room the other side can never draw.
#[test]
fn the_entity_orders_match_their_typescript_mirror() {
    let spellings: Vec<serde_json::Value> = [EntityOrder::UpdatedDesc, EntityOrder::TitleAsc]
        .into_iter()
        .map(|order| serde_json::to_value(order).unwrap())
        .collect();
    let spellings: Vec<&str> = spellings
        .iter()
        .map(|order| order.as_str().expect("an order serializes as a string"))
        .collect();
    assert_same_members(
        &spellings,
        declared_union(MIRROR, "EntityOrder"),
        "an ordering declared on one side only is a room the other side can \
         never draw",
    );
}

/// The status bar's stream and the detail view's history panel read this.
///
/// `entity_id` is checked as `None`, which is what a `sync:` line carries: a
/// run is about a source, not about any one entity it touched.
#[test]
fn the_activity_row_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(activity_row()).unwrap();
    assert_shape("ActivityRow", &wire, ACTIVITY_ROW_FIELDS);
    assert_eq!(wire["entity_id"], serde_json::Value::Null);
    // `detail` is `unknown` in the mirror because the Rust type is any JSON
    // value. It is not a string-keyed map, and nothing coerces it on the way
    // out -- only `activity::record` turns a JSON null into `{}` on the way in.
    assert!(wire["detail"].is_object());
    assert!(
        serde_json::to_value(ActivityRow {
            detail: serde_json::json!("a bare string"),
            ..activity_row()
        })
        .unwrap()["detail"]
            .is_string(),
        "a row whose detail is not an object is still well-typed"
    );
}

#[test]
fn the_source_ref_shape_matches_its_typescript_mirror() {
    assert_shape(
        "SourceRef",
        &serde_json::to_value(source_ref()).unwrap(),
        SOURCE_REF_FIELDS,
    );
}

/// `KindInfo` is declared in `entity.ts` even though its Rust home is
/// `knobas-source`, so this is where it is pinned. `sources_mirror.rs` checks
/// the copy that rides inside a `SourceDescriptor`; this checks the one the
/// detail view reads.
#[test]
fn the_kind_info_shape_matches_its_typescript_mirror() {
    assert_shape(
        "KindInfo",
        &serde_json::to_value(kind_info()).unwrap(),
        KIND_INFO_FIELDS,
    );
}

/// The other end, hydrated. `deleted_at` is `None` here and `Some` in the
/// entity-detail fixture below, because it is the nullable field on this shape
/// and both states have to keep the key: the panel's withdrawn marker reads it.
fn link_end() -> LinkEnd {
    LinkEnd {
        entity_id: "note:retry-storm".to_owned(),
        kind: "note".to_owned(),
        title: "Retry storm postmortem".to_owned(),
        deleted_at: None,
    }
}

fn link_entry() -> LinkEntry {
    LinkEntry {
        link: link_row(),
        other: link_end(),
    }
}

const LINK_END_FIELDS: &[&str] = &["deleted_at", "entity_id", "kind", "title"];

const LINK_ROW_FIELDS: &[&str] = &[
    "created_at",
    "created_by",
    "from_id",
    "id",
    "note",
    "origin",
    "relation",
    "to_id",
];

/// The link row, and every origin its `origin` field can hold.
///
/// Empty in M1 and about to stop being empty (#40), which is exactly when an
/// unpinned shape costs something: the panel that draws these is being written
/// against this declaration right now.
#[test]
fn the_link_row_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(link_row()).unwrap();
    assert_shape("LinkRow", &wire, LINK_ROW_FIELDS);
    // A uuid crosses as a string, not as an object or an array of bytes.
    assert!(wire["id"].is_string(), "{}", wire["id"]);

    // The origin union is declared inline on the field rather than as its own
    // exported type, so it is read off the field's line.
    let origins: Vec<&str> = Origin::ALL.iter().map(|origin| origin.as_str()).collect();
    assert_same_members(
        &origins,
        declared_inline_union(interface_body(MIRROR, "LinkRow"), "origin"),
        "an origin declared on one side only is a link the other side cannot \
         classify",
    );
    // ... and the enum serializes as the column value the union names.
    assert_eq!(wire["origin"], serde_json::json!(Origin::Manual.as_str()));

    // `note` is the field Links v1 wires through (#40): a column since `0001`
    // that reached nothing. Both of its states are checked, because they are
    // different failures -- a dropped key is `undefined` where the mirror
    // promised `string | null`, and a note that does not survive serialization
    // is a reason the user typed and the panel never shows.
    assert_eq!(
        wire["note"],
        serde_json::Value::Null,
        "a link with no note keeps the key and nulls it"
    );
    let annotated = serde_json::to_value(LinkRow {
        note: Some("why this link exists".to_owned()),
        ..link_row()
    })
    .unwrap();
    assert_shape("LinkRow", &annotated, LINK_ROW_FIELDS);
    assert_eq!(annotated["note"], serde_json::json!("why this link exists"));
}

/// The hydrated entry the links panel draws: the record, and the end the
/// reader is not on.
///
/// Nested rather than flattened, and the shape assertions below say so from
/// both sides: `link` and `other` are the only two keys, and each of them is
/// its own pinned shape. A `#[serde(flatten)]` here would collide `id` with
/// `entity_id`'s neighbours and hand the panel one bag of fields where it
/// declared two objects.
#[test]
fn the_link_entry_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(link_entry()).unwrap();
    assert_shape("LinkEntry", &wire, &["link", "other"]);
    assert_shape("LinkRow", &wire["link"], LINK_ROW_FIELDS);
    assert_shape("LinkEnd", &wire["other"], LINK_END_FIELDS);
    assert_eq!(
        wire["other"]["deleted_at"],
        serde_json::Value::Null,
        "a live end keeps the key and nulls it -- the panel's withdrawn marker \
         branches on it"
    );

    let withdrawn = serde_json::to_value(LinkEntry {
        other: LinkEnd {
            deleted_at: Some(at()),
            ..link_end()
        },
        ..link_entry()
    })
    .unwrap();
    assert_shape("LinkEnd", &withdrawn["other"], LINK_END_FIELDS);
    assert!(
        withdrawn["other"]["deleted_at"].is_string(),
        "a withdrawn end crosses as an RFC 3339 string: {}",
        withdrawn["other"]["deleted_at"]
    );
}

/// Everything the slide-over draws, and every nested shape inside it.
///
/// The nested assertions are the point: `EntityDetail` is the one DTO on this
/// bridge that carries four other declared shapes, and a top-level key-set
/// check would pass with every one of them wrong.
#[test]
fn the_entity_detail_shape_matches_its_typescript_mirror() {
    let detail = EntityDetail {
        row: row(Some(at())),
        source: source_ref(),
        kind_info: Some(kind_info()),
        body_text: "Retries pile up behind the gateway.".to_owned(),
        author: None,
        payload: serde_json::json!({ "key": "PAY-231" }),
        web_url: None,
        deleted_at: None,
        links: vec![link_entry()],
        activity: vec![activity_row()],
    };

    let wire = serde_json::to_value(detail).unwrap();
    assert_shape(
        "EntityDetail",
        &wire,
        &[
            "activity",
            "author",
            "body_text",
            "deleted_at",
            "kind_info",
            "links",
            "payload",
            "row",
            "source",
            "web_url",
        ],
    );

    assert_shape("EntityRow", &wire["row"], ENTITY_ROW_FIELDS);
    assert_shape("SourceRef", &wire["source"], SOURCE_REF_FIELDS);
    assert_shape("KindInfo", &wire["kind_info"], KIND_INFO_FIELDS);
    assert_shape("LinkEntry", &wire["links"][0], &["link", "other"]);
    assert_shape("LinkRow", &wire["links"][0]["link"], LINK_ROW_FIELDS);
    assert_shape("LinkEnd", &wire["links"][0]["other"], LINK_END_FIELDS);
    assert_shape("ActivityRow", &wire["activity"][0], ACTIVITY_ROW_FIELDS);

    // The three fields the header and the footer branch on are `null`, not
    // absent: *Open in browser* is drawn from `web_url`, the withdrawn banner
    // from `deleted_at`, and both read the key.
    for field in ["author", "web_url", "deleted_at"] {
        assert_eq!(wire[field], serde_json::Value::Null, "{field} lost its key");
    }
}
