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
    AttentionGroup, EntityDetail, EntityFilter, EntityOrder, EntityPage, EntityRow, GroupTally,
    KindTally, NoteLinkInput, PipelineRow, RunRow, SourceRef, SuggestionPage, UrlMatch,
};
use knobas_core::activity::ActivityRow;
use knobas_core::link::{LinkEnd, LinkEntry, LinkRow, Origin};
use knobas_core::suggest::{RuleClass, SuggestionEntry};
use knobas_sync::mirror::{declared_inline_union, declared_union, interface_body};

const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/entity.ts");

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
        // A ticket sits nowhere: the ADR-0007 miss `path` reports for every
        // record with no readable `ancestors` (#284).
        path: None,
        // A ticket's adapter declares no state (#666): the miss, as `null`.
        state: None,
    }
}

fn source_ref() -> SourceRef {
    SourceRef {
        id: "mock".to_owned(),
        display_name: "Tidewater Mock".to_owned(),
        adapter_kind: "mock".to_owned(),
        enabled: true,
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

/// A link with **no** note, because `note` is a nullable field on this row and
/// the file's second rule is that nullable fields are exercised as `None`: a
/// `skip_serializing_if` added to it would drop the key and hand the panel
/// `undefined` where the mirror promised `string | null`.
///
/// A *confirmed* link, so `rule`, `rule_class` and `reason` are `None` too --
/// the four suggestion fields (#41) split cleanly between the two states, and
/// [`proposal_row`] is the other half.
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
        confirmed_at: Some(at()),
        rule: None,
        rule_class: None,
        reason: None,
    }
}

/// The same row in the other state: a proposal, which is what makes
/// `confirmed_at` null and the other three present.
fn proposal_row() -> LinkRow {
    LinkRow {
        origin: Origin::Suggested,
        created_by: "knobas".to_owned(),
        confirmed_at: None,
        rule: Some("branch_name_key".to_owned()),
        rule_class: Some(RuleClass::ExactKey),
        reason: Some("the branch name contains PAY-231".to_owned()),
        ..link_row()
    }
}

const ACTIVITY_ROW_FIELDS: &[&str] = &["actor", "at", "detail", "entity_id", "id", "verb"];
const SOURCE_REF_FIELDS: &[&str] = &["adapter_kind", "display_name", "enabled", "id"];
const KIND_INFO_FIELDS: &[&str] = &["full_sync_exhaustive", "id", "label", "monogram", "plural"];

const ENTITY_ROW_FIELDS: &[&str] = &[
    "entity_id",
    "kind",
    // Where the row sits inside its source, ratified as a §10.8 exception
    // under #284's criterion 5. On the wire even when it is `null`, which is
    // what lets one TypeScript declaration serve both Rust structs.
    "path",
    "source_id",
    // The row's state chip (#666), on the wire as `null` for a miss.
    "state",
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
        tally: None,
        groups: None,
    };
    let wire = serde_json::to_value(page).unwrap();
    assert_shape("EntityPage", &wire, &["groups", "rows", "tally", "total"]);
    assert_shape("EntityRow", &wire["rows"][0], ENTITY_ROW_FIELDS);
    assert_eq!(
        wire["tally"],
        serde_json::Value::Null,
        "a plain order's page keeps the key and nulls it"
    );

    // A tile order's page (#680), with a tally line of each shape: one row
    // with a tone and one without, which is `null` on the wire.
    let tile = EntityPage {
        rows: Vec::new(),
        total: 3,
        tally: Some(vec![
            KindTally {
                kind: "pr".to_owned(),
                tone: Some(knobas_core::tone::Tone::Info),
                count: 2,
            },
            KindTally {
                kind: "branch".to_owned(),
                tone: None,
                count: 1,
            },
        ]),
        // The Code tile's groups (#712), each on the wire in its own word.
        groups: Some(vec![
            GroupTally {
                group: AttentionGroup::ReviewRequested,
                count: 2,
            },
            GroupTally {
                group: AttentionGroup::MyBranches,
                count: 1,
            },
        ]),
    };
    let wire = serde_json::to_value(tile).unwrap();
    assert_shape("EntityPage", &wire, &["groups", "rows", "tally", "total"]);
    for line in wire["tally"].as_array().unwrap() {
        assert_shape("KindTally", line, &["count", "kind", "tone"]);
    }
    assert_eq!(wire["tally"][1]["tone"], serde_json::Value::Null);
    for group in wire["groups"].as_array().unwrap() {
        assert_shape("GroupTally", group, &["count", "group"]);
    }
    assert_eq!(wire["groups"][0]["group"], "review_requested");
    assert_eq!(wire["groups"][1]["group"], "my_branches");
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
        "context": null,
        "project": null,
        "include_deleted": true,
        "reach": null,
    });
    let decoded: EntityFilter =
        serde_json::from_value(payload.clone()).expect("the mirror's EntityFilter decodes");
    let wire = serde_json::to_value(&decoded).unwrap();

    assert_shape(
        "EntityFilter",
        &wire,
        &[
            "context",
            "include_deleted",
            "kinds",
            "order",
            "project",
            "reach",
            "sources",
            "updated_within_days",
        ],
    );
    assert_eq!(
        wire, payload,
        "every value the frontend sent survived the decode unchanged"
    );

    // The window is one of the nullable fields, and `null` there means *no
    // window* rather than zero days -- a decode that defaulted it to `Some(0)`
    // would silently empty every room.
    assert_eq!(decoded.updated_within_days, None);
    assert_eq!(decoded.order, EntityOrder::TitleAsc);
    // And `null` on the project dimension is *unscoped* rather than "the
    // project spelled nothing", which is the same distinction (#208).
    assert_eq!(decoded.project, None);
}

/// A filter written before #817 carries no `reach`, and reads as unscoped.
#[test]
fn a_filter_without_reach_is_unscoped() {
    let decoded: EntityFilter = serde_json::from_value(serde_json::json!({
        "sources": [], "kinds": [], "updated_within_days": null, "order": "updated_desc",
        "context": null, "project": null, "include_deleted": false,
    }))
    .unwrap();
    assert_eq!(decoded.reach, None);
}

/// The job-reach read's row (#817), both origins, against `entity.ts`.
#[test]
fn the_reached_shape_matches_its_typescript_mirror() {
    use knobas_core::reach::{Origin as ReachOrigin, Reached};
    let own = Reached {
        entity_id: "gitea:svc#1".into(),
        kind: "pr".into(),
        step: "gitea:svc#1".into(),
        origin: ReachOrigin::Own,
    };
    assert_shape(
        "Reached",
        &serde_json::to_value(&own).unwrap(),
        &["entity_id", "kind", "step", "origin"],
    );
    let parent = serde_json::to_value(ReachOrigin::Parent {
        ticket: "jira:PAY-231".into(),
    })
    .unwrap();
    assert!(MIRROR.contains(
        r#"| { kind: "own" }
  | { kind: "parent"; ticket: string }
  | { kind: "link_set"; set: string; name: string };"#
    ));
    assert_eq!(
        serde_json::to_value(ReachOrigin::LinkSet {
            set: "payout-stack".into(),
            name: "Payout stack".into(),
        })
        .unwrap(),
        serde_json::json!({ "kind": "link_set", "set": "payout-stack", "name": "Payout stack" })
    );
    assert_eq!(
        parent,
        serde_json::json!({ "kind": "parent", "ticket": "jira:PAY-231" })
    );
}

/// The projects read's one DTO (#208), against what `entity.ts` declares.
///
/// `name` is exercised as `None` per this file's rule, and it is the field
/// whose absence the read exists to survive: a project with a key and no
/// readable name is still a project, carried by its key, and a
/// `skip_serializing_if` here would hand the switcher `undefined` where the
/// mirror promised `string | null`.
#[test]
fn the_project_shape_matches_its_typescript_mirror() {
    let project = knobas_core::project::Project {
        source_id: "mock".to_owned(),
        key: "PAY".to_owned(),
        name: None,
    };
    let wire = serde_json::to_value(&project).unwrap();

    assert_shape("Project", &wire, &["key", "name", "source_id"]);
    assert_eq!(wire["name"], serde_json::Value::Null);
}

/// A row's state chip (#666) and its five tones, in the spelling the
/// mirror's `Tone` union declares: a tone the frontend does not name is a
/// chip drawn with no colour.
#[test]
fn the_state_chip_and_its_tones_match_their_typescript_mirror() {
    use knobas_core::payload::StateChip;
    use knobas_core::tone::Tone;
    let chip = StateChip {
        text: "merged".to_owned(),
        tone: Tone::Ok,
    };
    assert_shape(
        "StateChip",
        &serde_json::to_value(&chip).unwrap(),
        &["text", "tone"],
    );
    let tones: Vec<serde_json::Value> = [Tone::Ok, Tone::Warn, Tone::Err, Tone::Info, Tone::Muted]
        .into_iter()
        .map(|tone| serde_json::to_value(tone).unwrap())
        .collect();
    let tones: Vec<&str> = tones.iter().filter_map(serde_json::Value::as_str).collect();
    assert_same_members(
        &tones,
        declared_union(MIRROR, "Tone"),
        "a tone declared on one side only is a chip the shell cannot colour",
    );
}

/// Every ordering -- the two plain ones and the two tile orders (#680) -- in
/// the spelling the mirror's union declares.
///
/// Read out of `entity.ts` rather than listed here: an ordering added on one
/// side only is a room the other side can never draw.
#[test]
fn the_entity_orders_match_their_typescript_mirror() {
    let spellings: Vec<serde_json::Value> = [
        EntityOrder::UpdatedDesc,
        EntityOrder::TitleAsc,
        EntityOrder::Attention,
        EntityOrder::LatestPerParent,
    ]
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
    "confirmed_at",
    "created_at",
    "created_by",
    "from_id",
    "id",
    "note",
    "origin",
    "reason",
    "relation",
    "rule",
    "rule_class",
    "to_id",
];

const SUGGESTION_ENTRY_FIELDS: &[&str] = &["from", "link", "to"];

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

/// The four fields a **suggestion** rides on (#41), in both of the states a
/// link row can be in.
///
/// Both, because they are different failures and the shape check alone catches
/// neither. `confirmed_at` is the *state* the two reads are cut on, so a
/// `skip_serializing_if` on it would hand the frontend `undefined` exactly
/// where it branches on `null`; and a proposal that lost `reason` on the wire
/// is a suggestion the tray cannot draw, which #41 says is not shippable.
#[test]
fn a_proposal_and_a_confirmed_link_are_the_same_shape_in_two_states() {
    let confirmed = serde_json::to_value(link_row()).unwrap();
    assert_shape("LinkRow", &confirmed, LINK_ROW_FIELDS);
    assert!(confirmed["confirmed_at"].is_string());
    for absent in ["rule", "rule_class", "reason"] {
        assert_eq!(
            confirmed[absent],
            serde_json::Value::Null,
            "a link a person drew keeps {absent} and nulls it"
        );
    }

    let proposal = serde_json::to_value(proposal_row()).unwrap();
    assert_shape("LinkRow", &proposal, LINK_ROW_FIELDS);
    assert_eq!(
        proposal["confirmed_at"],
        serde_json::Value::Null,
        "the null is the whole difference between a link and a suggestion"
    );
    assert_eq!(
        proposal["reason"],
        serde_json::json!("the branch name contains PAY-231")
    );
    assert_eq!(proposal["rule"], serde_json::json!("branch_name_key"));
    assert_eq!(proposal["rule_class"], serde_json::json!("exact_key"));

    // The class union is declared inline on the field, so it is read off that
    // field's line -- a class the frontend cannot name is a badge it cannot
    // draw, and it is the axis a reader calibrates trust on.
    let classes: Vec<&str> = RuleClass::ALL.iter().map(|class| class.as_str()).collect();
    assert_same_members(
        &classes,
        declared_inline_union(interface_body(MIRROR, "LinkRow"), "rule_class"),
        "a rule class declared on one side only is a suggestion the other side \
         cannot label",
    );
}

/// The tray's own two shapes: a proposal with **both** ends resolved, and the
/// page that carries the room's total beside the rows.
///
/// Nested for the same reason `LinkEntry` is: flattening `link`, `from` and
/// `to` into one bag would collide `id` three ways.
#[test]
fn the_suggestion_shapes_match_their_typescript_mirror() {
    let entry = SuggestionEntry {
        link: proposal_row(),
        from: LinkEnd {
            entity_id: "gitea:tidewater/payout#b1".to_owned(),
            kind: "branch".to_owned(),
            title: "feature/PAY-231-retry".to_owned(),
            deleted_at: None,
        },
        // The withdrawn state, on the end that is likelier to have it: a
        // proposal may point at something the source removed between the pass
        // that found it and the reader looking at it.
        to: LinkEnd {
            entity_id: "mock:PAY-231".to_owned(),
            kind: "ticket".to_owned(),
            title: "Payout retry storm".to_owned(),
            deleted_at: Some(at()),
        },
    };
    let wire = serde_json::to_value(&entry).unwrap();
    assert_shape("SuggestionEntry", &wire, SUGGESTION_ENTRY_FIELDS);
    assert_shape("LinkRow", &wire["link"], LINK_ROW_FIELDS);
    assert_shape("LinkEnd", &wire["from"], LINK_END_FIELDS);
    assert_shape("LinkEnd", &wire["to"], LINK_END_FIELDS);
    assert_eq!(wire["from"]["deleted_at"], serde_json::Value::Null);
    assert!(wire["to"]["deleted_at"].is_string());

    let page = serde_json::to_value(SuggestionPage {
        rows: vec![entry],
        total: 7,
        more: vec![],
    })
    .unwrap();
    assert_shape("SuggestionPage", &page, &["rows", "total", "more"]);
    assert_eq!(
        page["total"], 7,
        "the total is the room's, not the page's -- it is what the heading shows"
    );
    assert_shape("SuggestionEntry", &page["rows"][0], SUGGESTION_ENTRY_FIELDS);
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
        builds: vec![row(Some(at()))],
        status: None,
        issue_type: None,
        priority: None,
        estimate_seconds: None,
    };

    let wire = serde_json::to_value(detail).unwrap();
    assert_shape(
        "EntityDetail",
        &wire,
        &[
            "activity",
            "author",
            "body_text",
            "builds",
            "deleted_at",
            "estimate_seconds",
            "issue_type",
            "kind_info",
            "links",
            "payload",
            "priority",
            "row",
            "source",
            "status",
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
    assert_shape("EntityRow", &wire["builds"][0], ENTITY_ROW_FIELDS);

    // The three fields the header and the footer branch on are `null`, not
    // absent: *Open in browser* is drawn from `web_url`, the withdrawn banner
    // from `deleted_at`, and both read the key.
    for field in ["author", "web_url", "deleted_at", "status"] {
        assert_eq!(wire[field], serde_json::Value::Null, "{field} lost its key");
    }
}

/// What a pasted URL resolved to (#496).
///
/// Two keys and no more: a paste answers *which entity is this link*, and the
/// frontend then opens that entity by the read that already exists. A field
/// added here would be one the launcher fetches on every paste and draws
/// nowhere.
#[test]
fn the_url_match_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(UrlMatch {
        entity_id: "jira:PAY-231".to_owned(),
        kind: "ticket".to_owned(),
    })
    .unwrap();
    assert_shape("UrlMatch", &wire, &["entity_id", "kind"]);
}

/// The Builds frame's two rows (#848): a run with every nullable field
/// empty -- a queued run -- inside a pipeline with no group, so a
/// `skip_serializing_if` on any of them cannot pass.
#[test]
fn the_pipeline_and_run_shapes_match_their_typescript_mirror() {
    let queued = RunRow {
        entity_id: "teamcity:build:7".to_owned(),
        number: None,
        state: None,
        branch: None,
        started: None,
        finished: None,
        duration_seconds: None,
        updated_at: None,
        web_url: None,
    };
    let run_keys = [
        "branch",
        "duration_seconds",
        "entity_id",
        "finished",
        "number",
        "started",
        "state",
        "updated_at",
        "web_url",
    ];
    assert_shape("RunRow", &serde_json::to_value(&queued).unwrap(), &run_keys);
    let pipeline = PipelineRow {
        entity_id: "teamcity:buildType:Payout_Lint".to_owned(),
        source_id: "teamcity".to_owned(),
        source_name: "TeamCity".to_owned(),
        name: "Lint".to_owned(),
        group: None,
        path: vec!["Payments".to_owned()],
        pinned: false,
        web_url: None,
        recent: vec![queued],
    };
    let wire = serde_json::to_value(&pipeline).unwrap();
    assert_shape(
        "PipelineRow",
        &wire,
        &[
            "entity_id",
            "group",
            "name",
            "path",
            "pinned",
            "recent",
            "source_id",
            "source_name",
            "web_url",
        ],
    );
    assert_shape("RunRow", &wire["recent"][0], &run_keys);
}

/// `job_pipelines` (#850): a pipeline id and its reasons, each tagged on
/// `kind` -- `ipc/entity.ts`' `JobPipeline` and `PipelineReason`.
#[test]
fn the_job_pipeline_shape_matches_its_typescript_mirror() {
    use knobas_app::commands::entity::{JobPipeline, PipelineReason};
    let row = JobPipeline {
        pipeline_id: "teamcity:buildType:Payout_Lint".to_owned(),
        reasons: vec![
            PipelineReason::Branch {
                branch: "feature/PAY-231-sepa-retry".to_owned(),
            },
            PipelineReason::PrCheck {
                pr: "gitea:acme/svc#142".to_owned(),
            },
            PipelineReason::Linked {
                step: "jira:PAY-230".to_owned(),
                origin: knobas_core::reach::Origin::Parent {
                    ticket: "jira:PAY-230".to_owned(),
                },
            },
        ],
    };
    let wire = serde_json::to_value(&row).unwrap();
    assert_shape("JobPipeline", &wire, &["pipeline_id", "reasons"]);
    assert_eq!(
        wire["reasons"],
        serde_json::json!([
            { "kind": "branch", "branch": "feature/PAY-231-sepa-retry" },
            { "kind": "pr_check", "pr": "gitea:acme/svc#142" },
            { "kind": "linked", "step": "jira:PAY-230",
              "origin": { "kind": "parent", "ticket": "jira:PAY-230" } }
        ])
    );
    assert!(MIRROR.contains(concat!(
        r#"| { kind: "branch"; branch: string }"#,
        "\n  ",
        r#"| { kind: "pr_check"; pr: string }"#,
        "\n  ",
        r#"| { kind: "linked"; step: string; origin: ReachOrigin };"#
    )));
}

/// #851's pipeline settings: every field on the wire, in both directions,
/// through the repository and the trigger it nests.
#[test]
fn the_pipeline_settings_shape_matches_its_typescript_mirror() {
    use knobas_source::pipeline::{
        ParameterKind, ParameterOption, ParameterSpec, PipelineRepository, PipelineSettings,
        PipelineTrigger,
    };
    let settings = PipelineSettings {
        repositories: vec![PipelineRepository {
            name: "payout-service".to_owned(),
            url: None,
            default_branch: None,
            entity_id: None,
        }],
        triggers: vec![PipelineTrigger {
            name: "VCS".to_owned(),
            facts: vec![],
            disabled: false,
        }],
        parameters: vec![],
        parameter_write_op: None,
        parameter_specs: vec![
            ParameterSpec {
                name: "env".to_owned(),
                label: None,
                hint: None,
                kind: ParameterKind::Select {
                    options: vec![ParameterOption {
                        value: "staging".to_owned(),
                        label: None,
                    }],
                },
            },
            ParameterSpec {
                name: "clean".to_owned(),
                label: None,
                hint: None,
                kind: ParameterKind::Checkbox {
                    checked: "true".to_owned(),
                    unchecked: "false".to_owned(),
                },
            },
            ParameterSpec {
                name: "filter".to_owned(),
                label: None,
                hint: None,
                kind: ParameterKind::Text,
            },
        ],
        chain: vec![knobas_source::pipeline::ChainStep {
            pipeline: "teamcity:buildType:Payout_Build".to_owned(),
            name: "Build".to_owned(),
        }],
    };
    let wire = serde_json::to_value(&settings).unwrap();
    // #857's chain.
    assert_shape("ChainStep", &wire["chain"][0], &["name", "pipeline"]);
    assert_shape(
        "PipelineSettings",
        &wire,
        &[
            "chain",
            "parameter_specs",
            "parameter_write_op",
            "parameters",
            "repositories",
            "triggers",
        ],
    );
    // #852's parameter specs, each kind on its tag.
    assert_shape(
        "ParameterSpec",
        &wire["parameter_specs"][0],
        &["hint", "kind", "label", "name"],
    );
    assert_shape(
        "ParameterOption",
        &wire["parameter_specs"][0]["kind"]["options"][0],
        &["label", "value"],
    );
    assert_eq!(wire["parameter_specs"][0]["kind"]["type"], "select");
    assert_eq!(wire["parameter_specs"][1]["kind"]["type"], "checkbox");
    assert_eq!(
        wire["parameter_specs"][2]["kind"],
        serde_json::json!({ "type": "text" })
    );
    assert!(MIRROR.contains(concat!(
        r#"| { type: "text" }"#,
        "
  ",
        r#"| { type: "select"; options: ParameterOption[] }"#,
        "
  ",
        r#"| { type: "checkbox"; checked: string; unchecked: string };"#
    )));
    assert_shape(
        "PipelineRepository",
        &wire["repositories"][0],
        &["default_branch", "entity_id", "name", "url"],
    );
    assert_shape(
        "PipelineTrigger",
        &wire["triggers"][0],
        &["disabled", "facts", "name"],
    );
}

/// #857's chain step of a run.
#[test]
fn the_chain_step_row_shape_matches_its_typescript_mirror() {
    let row = knobas_app::commands::entity::ChainStepRow {
        pipeline_id: "teamcity:buildType:Payout_Build".to_owned(),
        name: "Build".to_owned(),
        run: None,
    };
    let wire = serde_json::to_value(&row).unwrap();
    assert_shape("ChainStepRow", &wire, &["name", "pipeline_id", "run"]);
    assert!(MIRROR.contains(r#"invoke<ChainStepRow[]>("run_chain", { runId })"#));
}

/// #852's Run… preset, and the two commands by their registered names.
#[test]
fn the_run_preset_shape_matches_its_typescript_mirror() {
    let preset = knobas_app::run_presets::RunPreset {
        pipeline_id: "teamcity:buildType:Payout_IntegrationTests".to_owned(),
        name: "Only the retry tests".to_owned(),
        branch: None,
        parameters: vec![],
        clean_checkout: false,
        first_in_queue: true,
    };
    let wire = serde_json::to_value(&preset).unwrap();
    assert_shape(
        "RunPreset",
        &wire,
        &[
            "branch",
            "clean_checkout",
            "first_in_queue",
            "name",
            "parameters",
            "pipeline_id",
        ],
    );
    for command in ["\"run_presets\"", "\"save_run_preset\""] {
        assert!(MIRROR.contains(command), "{command} is not invoked");
    }
}

fn note_row() -> knobas_core::note::NoteRow {
    knobas_core::note::NoteRow {
        id: "note:7f2cf0d4-1f1e-4b2f-9a4a-0d1c2e3f4a5b".to_owned(),
        title: "SEPA retry investigation".to_owned(),
        body_md: "The counter starts at zero — see [[mock:PAY-231]].".to_owned(),
        created_at: at(),
        updated_at: at(),
    }
}

const NOTE_ROW_FIELDS: &[&str] = &["body_md", "created_at", "id", "title", "updated_at"];

#[test]
fn the_note_row_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(note_row()).unwrap();
    assert_shape("NoteRow", &wire, NOTE_ROW_FIELDS);
}

/// A ref carries what it named and what it found, and the *unresolved* case is
/// exercised as `None` -- which is the case the whole DTO exists for.
///
/// `null` and not a missing key: the editor branches on it to draw a ref as
/// unresolved, and `undefined` where it declared `null` is the failure this
/// file's header describes.
#[test]
fn the_note_ref_shape_matches_its_typescript_mirror() {
    let resolved = knobas_core::note::NoteRef {
        target_id: "mock:PAY-231".to_owned(),
        target: Some(link_end()),
    };
    let wire = serde_json::to_value(&resolved).unwrap();
    assert_shape("NoteRef", &wire, &["target", "target_id"]);
    assert_shape("LinkEnd", &wire["target"], LINK_END_FIELDS);

    let unresolved = knobas_core::note::NoteRef {
        target_id: "mock:NOSUCH-1".to_owned(),
        target: None,
    };
    let wire = serde_json::to_value(&unresolved).unwrap();
    assert_shape("NoteRef", &wire, &["target", "target_id"]);
    assert!(
        wire["target"].is_null(),
        "an unresolved ref is a null target, not an absent key: {wire}"
    );
}

/// What a caller sends to have a note born with a link (#502).
///
/// Round-tripped rather than serialized, for the reason
/// [`the_entity_filter_shape_matches_its_typescript_mirror`] states: this is
/// an **input** DTO, and only decoding the mirror's own object and re-encoding
/// it sees both directions at once.
///
/// The relation is a plain `string` on both sides on purpose. The curated menu
/// is a rendering decision and lives in `app/src/lib/detail/relations.ts`
/// (§5a's vocabulary is open), so an enum here would put a closed list in the
/// one place the design says there is not one.
#[test]
fn the_note_link_input_shape_matches_its_typescript_mirror() {
    let payload = serde_json::json!({
        "target_id": "ctx:2f1a5d6e-0c9b-4a3e-8f7d-1b2c3d4e5f60",
        "relation": "captured-in",
    });
    let decoded: NoteLinkInput =
        serde_json::from_value(payload).expect("the mirror's NoteLinkInput decodes");
    let wire = serde_json::to_value(&decoded).unwrap();

    assert_shape("NoteLinkInput", &wire, &["relation", "target_id"]);
    assert_eq!(wire["relation"], serde_json::json!("captured-in"));
}

/// The whole note view, including both of its lists at once.
///
/// The `refs`/`links` pair is the part a reader is most likely to "simplify"
/// into one field, so the fixture carries a ref that resolves, a ref that does
/// not, and a link -- and every nested shape is checked, because a field added
/// to `LinkEntry` reaches this DTO without touching it.
#[test]
fn the_note_detail_shape_matches_its_typescript_mirror() {
    let detail = knobas_app::commands::entity::NoteDetail {
        note: note_row(),
        refs: vec![
            knobas_core::note::NoteRef {
                target_id: "mock:PAY-231".to_owned(),
                target: Some(link_end()),
            },
            knobas_core::note::NoteRef {
                target_id: "mock:NOSUCH-1".to_owned(),
                target: None,
            },
        ],
        links: vec![link_entry()],
    };
    let wire = serde_json::to_value(&detail).unwrap();
    assert_shape("NoteDetail", &wire, &["links", "note", "refs"]);
    assert_shape("NoteRow", &wire["note"], NOTE_ROW_FIELDS);
    assert_shape("NoteRef", &wire["refs"][0], &["target", "target_id"]);
    assert_shape("LinkEnd", &wire["refs"][0]["target"], LINK_END_FIELDS);
    assert!(wire["refs"][1]["target"].is_null());
    assert_shape("LinkEntry", &wire["links"][0], &["link", "other"]);
    assert_shape("LinkRow", &wire["links"][0]["link"], LINK_ROW_FIELDS);
    assert_shape("LinkEnd", &wire["links"][0]["other"], LINK_END_FIELDS);
}

// -- the start-work flow (#44) ---------------------------------------------

/// One step, with every nullable field empty -- so a `skip_serializing_if`
/// added to either would drop the key and hand the view `undefined` where the
/// mirror declares `null`.
fn flow_step() -> knobas_core::start_work::FlowStep {
    knobas_core::start_work::FlowStep {
        id: 7,
        ticket_id: "jira:PAY-231".to_owned(),
        step: knobas_core::start_work::Step::CreateBranch,
        position: 0,
        outcome: knobas_core::start_work::StepOutcome::Pending,
        payload: serde_json::json!({
            "CreateBranch": {
                "entity": "gitea:tidewater/payout-service",
                "name": "feature/PAY-231-sepa-retry",
                "from_ref": "main"
            }
        }),
        write_id: None,
        detail: None,
        updated_at: at(),
    }
}

#[test]
fn the_start_work_step_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(flow_step()).unwrap();
    assert_shape(
        "StartWorkStep",
        &wire,
        &[
            "id",
            "ticket_id",
            "step",
            "position",
            "outcome",
            "payload",
            "write_id",
            "detail",
            "updated_at",
        ],
    );
}

/// The two vocabularies, read out of the mirror rather than listed here.
///
/// A step kind the view cannot name is a row it draws as nothing; an outcome it
/// cannot name is worse -- `queued` drawn as a failure would have the reader
/// retry a write that is already on its way, and drawn as a success would have
/// them believe work happened that has not.
#[test]
fn the_start_work_vocabularies_match_their_typescript_mirror() {
    assert_same_members(
        &knobas_core::start_work::Step::ALL
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        declared_union(MIRROR, "StartWorkStepKind"),
        "a step kind the stepper cannot name is a row it cannot draw",
    );
    assert_same_members(
        &knobas_core::start_work::StepOutcome::ALL
            .iter()
            .map(|o| o.as_str())
            .collect::<Vec<_>>(),
        declared_union(MIRROR, "StartWorkOutcome"),
        "an outcome the stepper cannot name is a step the reader misjudges",
    );
}

// -- the inbox (issue #45) --------------------------------------------------

const INBOX_ITEM_FIELDS: &[&str] = &[
    "category",
    "entity_id",
    "key",
    "kind",
    "occurred_at",
    "reason",
    "snoozed_until",
    "source_id",
    "title",
    "web_url",
];

/// A credential expiry: the one item with **no entity behind it**, so every
/// nullable field on this shape is exercised as `None` somewhere.
///
/// That matters more here than on most shapes. `entity_id` and `kind` are null
/// for exactly one of the six categories, so a `skip_serializing_if` added to
/// either would pass every test written against a review request and hand the
/// inbox view `undefined` on the one row that needs the *Open* button hidden.
fn inbox_expiry() -> knobas_core::inbox::InboxItem {
    knobas_core::inbox::InboxItem {
        key: "credential_expiry:jira".to_owned(),
        category: knobas_core::inbox::Category::CredentialExpiry,
        source_id: "jira".to_owned(),
        entity_id: None,
        kind: None,
        title: "Tidewater Jira".to_owned(),
        reason: "the Tidewater Jira credential expires on Friday 4 Sep 2026".to_owned(),
        occurred_at: at(),
        web_url: None,
        snoozed_until: None,
    }
}

/// A review request: the shape with everything filled in, and the one the
/// action bar is drawn from.
fn inbox_review() -> knobas_core::inbox::InboxItem {
    knobas_core::inbox::InboxItem {
        key: "review_request:gitea:acme/payouts#144".to_owned(),
        category: knobas_core::inbox::Category::ReviewRequest,
        source_id: "gitea".to_owned(),
        entity_id: Some("gitea:acme/payouts#144".to_owned()),
        kind: Some("pr".to_owned()),
        title: "Add payout CSV export".to_owned(),
        reason: "jonas.becker asked for your review".to_owned(),
        occurred_at: at(),
        web_url: Some("https://gitea.example/acme/payouts/pulls/144".to_owned()),
        snoozed_until: Some(at()),
    }
}

#[test]
fn the_inbox_item_shape_matches_its_typescript_mirror() {
    let filled = serde_json::to_value(inbox_review()).unwrap();
    assert_shape("InboxItem", &filled, INBOX_ITEM_FIELDS);
    assert_eq!(filled["category"], serde_json::json!("review_request"));

    let empty = serde_json::to_value(inbox_expiry()).unwrap();
    assert_shape("InboxItem", &empty, INBOX_ITEM_FIELDS);
    for absent in ["entity_id", "kind", "web_url", "snoozed_until"] {
        assert!(
            empty[absent].is_null(),
            "{absent} must keep its key as null, not vanish"
        );
    }
}

/// Nested, not flattened -- #53's ratified shape. A flattened bag would make a
/// reader guess which half `actions` came from, and would put the derivation's
/// fields and the app's answer in one namespace where a later collision is
/// silent.
#[test]
fn the_inbox_entry_shape_matches_its_typescript_mirror() {
    let entry = knobas_app::inbox::InboxEntry {
        item: inbox_review(),
        actions: vec!["approve".to_owned(), "comment".to_owned()],
    };
    let wire = serde_json::to_value(&entry).unwrap();
    assert_shape("InboxEntry", &wire, &["actions", "item"]);
    assert_shape("InboxItem", &wire["item"], INBOX_ITEM_FIELDS);
    assert_eq!(wire["actions"], serde_json::json!(["approve", "comment"]));

    let none = knobas_app::inbox::InboxEntry {
        item: inbox_expiry(),
        actions: Vec::new(),
    };
    assert_eq!(
        serde_json::to_value(&none).unwrap()["actions"],
        serde_json::json!([]),
        "no actions is an empty list, never a missing key"
    );
}

/// Every category, read off the enum rather than listed here: a hand-copied
/// list is the remembered-list trap one level down, and would pass while both
/// the union and the copy drifted from the Rust.
///
/// A category declared on one side only is a demand the interface cannot
/// label -- and the label is the whole of how a reader tells a failed build
/// from a mention at a glance.
#[test]
fn the_inbox_categories_match_their_typescript_mirror() {
    let spellings: Vec<&str> = knobas_core::inbox::Category::ALL
        .iter()
        .map(|category| category.as_str())
        .collect();
    assert_same_members(
        &spellings,
        declared_union(MIRROR, "InboxCategory"),
        "a category declared on one side only is a demand the interface \
         cannot label",
    );
}

/// The two shelves, which are one predicate asked from either side. A third
/// member on one side would be a shelf the backend never fills.
#[test]
fn the_inbox_shelves_match_their_typescript_mirror() {
    let spellings: Vec<serde_json::Value> = [
        knobas_core::inbox::Shelf::Stream,
        knobas_core::inbox::Shelf::Snoozed,
    ]
    .into_iter()
    .map(|shelf| serde_json::to_value(shelf).unwrap())
    .collect();
    let spellings: Vec<&str> = spellings
        .iter()
        .map(|shelf| shelf.as_str().expect("a shelf serializes as a string"))
        .collect();
    assert_same_members(
        &spellings,
        declared_union(MIRROR, "InboxShelf"),
        "a shelf declared on one side only is a read the backend never answers",
    );
}

const CONTEXT_ROW_FIELDS: &[&str] = &[
    "anchor_id",
    "archived_at",
    "created_at",
    "id",
    "kind",
    "title",
];

/// The switcher's row (#47). `anchor_id` and `archived_at` are the nullable
/// fields and are exercised as `None` -- an ad-hoc, live context -- per this
/// file's rule that every nullable field is empty somewhere.
#[test]
fn the_context_row_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(knobas_core::context::ContextRow {
        id: "ctx:5b1c0f1e".to_owned(),
        kind: knobas_core::context::ContextKind::Adhoc,
        title: "Staging DB configuration".to_owned(),
        anchor_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap(),
        archived_at: None,
    })
    .unwrap();
    assert_shape("ContextRow", &wire, CONTEXT_ROW_FIELDS);
    assert_eq!(wire["anchor_id"], serde_json::Value::Null);
    assert_eq!(wire["archived_at"], serde_json::Value::Null);
}

/// Every context kind, in the spelling the mirror's union declares -- read
/// out of `entity.ts` rather than listed here, the rule `EntityOrder` set.
#[test]
fn the_context_kinds_match_their_typescript_mirror() {
    let spellings: Vec<&str> = knobas_core::context::ContextKind::ALL
        .iter()
        .map(|kind| kind.as_str())
        .collect();
    assert_same_members(
        &spellings,
        declared_union(MIRROR, "ContextKind"),
        "a kind declared on one side only is a chip the other side can never \
         draw",
    );
}

const MINI_BOARD_FIELDS: &[&str] = &["columns", "sources"];
const MINI_BOARD_COLUMN_FIELDS: &[&str] = &["cards", "status", "total"];
const MINI_BOARD_CARD_FIELDS: &[&str] = &[
    "assignee",
    "entity_id",
    "issue_type",
    "key",
    "labels",
    "mine",
    "path",
    "priority",
    "signals",
    "source_id",
    "title",
    "updated_at",
];
const SIGNAL_FIELDS: &[&str] = &["kind", "text", "tone"];
const SOURCE_STATUSES_FIELDS: &[&str] = &["source_id", "statuses"];

/// The mini board's four shapes (#177), each against what `entity.ts`
/// declares.
///
/// Both nullable fields are exercised as `None` in the fixture, per this
/// file's rule: `MiniBoardColumn.status` -- which is the *terminal group*, the
/// one column whose absence of a status is the whole point -- and
/// `MiniBoardCard.priority`, whose miss is the other pinned direction. A
/// `skip_serializing_if` on either would hand the tile `undefined` where the
/// mirror declared `null`, and the tile branches on exactly that.
#[test]
fn the_mini_board_shapes_match_their_typescript_mirror() {
    let board = knobas_core::mini_board::MiniBoard {
        columns: vec![knobas_core::mini_board::MiniBoardColumn {
            status: None,
            cards: vec![knobas_core::mini_board::MiniBoardCard {
                entity_id: "mock:PAY-231".to_owned(),
                source_id: "mock".to_owned(),
                key: "PAY-231".to_owned(),
                title: "Retry failed SEPA payouts".to_owned(),
                priority: None,
                issue_type: None,
                assignee: None,
                updated_at: None,
                path: None,
                mine: false,
                labels: vec!["sepa".to_owned()],
                signals: vec![knobas_core::signals::Signal {
                    kind: knobas_core::signals::SignalKind::Blocker,
                    text: "blocked by OPS-77".to_owned(),
                    tone: knobas_core::tone::Tone::Err,
                }],
            }],
            total: 1,
        }],
        sources: vec![knobas_core::mini_board::SourceStatuses {
            source_id: "mock".to_owned(),
            statuses: vec!["In Progress".to_owned()],
        }],
    };
    let wire = serde_json::to_value(&board).unwrap();

    assert_shape("MiniBoard", &wire, MINI_BOARD_FIELDS);
    assert_shape(
        "MiniBoardColumn",
        &wire["columns"][0],
        MINI_BOARD_COLUMN_FIELDS,
    );
    assert_shape(
        "MiniBoardCard",
        &wire["columns"][0]["cards"][0],
        MINI_BOARD_CARD_FIELDS,
    );
    assert_shape(
        "SourceStatuses",
        &wire["sources"][0],
        SOURCE_STATUSES_FIELDS,
    );
    assert_shape(
        "Signal",
        &wire["columns"][0]["cards"][0]["signals"][0],
        SIGNAL_FIELDS,
    );
    assert_same_members(
        &knobas_core::signals::SignalKind::ALL.map(knobas_core::signals::SignalKind::as_str),
        declared_union(MIRROR, "SignalKind"),
        "a signal kind on one side only is a `has:` value the shell cannot name",
    );

    assert_eq!(wire["columns"][0]["status"], serde_json::Value::Null);
    for nullable in ["priority", "issue_type", "assignee", "updated_at", "path"] {
        assert_eq!(
            wire["columns"][0]["cards"][0][nullable],
            serde_json::Value::Null,
            "{nullable}"
        );
    }
}

// -- the standup digest (#288) ----------------------------------------------

/// Every field of a digest line, in the order `DigestLine` declares them.
const DIGEST_LINE_FIELDS: &[&str] = &[
    "entity_id",
    "kind",
    "title",
    "source",
    "verb",
    "reason",
    "at",
];

/// The line with an item behind it -- the shape every list but one draws.
fn digest_line() -> knobas_app::standup::DigestLine {
    knobas_app::standup::DigestLine {
        entity_id: Some("jira:PAY-231".to_owned()),
        kind: Some("ticket".to_owned()),
        title: "Payments retry storm".to_owned(),
        source: "jira".to_owned(),
        verb: "log_work".to_owned(),
        reason: "you logged work on it in jira".to_owned(),
        at: at(),
    }
}

/// The one line with **no** item: a running timer on an ad-hoc label.
///
/// Exercised as its own fixture because `entity_id` and `kind` are null on
/// exactly this line, and a `skip_serializing_if` added to either would pass
/// every test written against the other and hand the view `undefined` on the
/// one row whose whole point is that it must not be a link.
fn digest_label_line() -> knobas_app::standup::DigestLine {
    knobas_app::standup::DigestLine {
        entity_id: None,
        kind: None,
        title: "DB config for the migration".to_owned(),
        source: "knobas".to_owned(),
        verb: "timer".to_owned(),
        reason: "the timer is running on this label".to_owned(),
        at: at(),
    }
}

#[test]
fn the_digest_line_shape_matches_its_typescript_mirror() {
    let filled = serde_json::to_value(digest_line()).unwrap();
    assert_shape("DigestLine", &filled, DIGEST_LINE_FIELDS);

    let label = serde_json::to_value(digest_label_line()).unwrap();
    assert_shape("DigestLine", &label, DIGEST_LINE_FIELDS);
    for absent in ["entity_id", "kind"] {
        assert!(
            label[absent].is_null(),
            "{absent} must keep its key as null, not vanish -- the view branches on it"
        );
    }
}

/// The digest itself, and the day its first list is about.
///
/// `yesterday_day` is exercised **as `None`** as well as filled: a week of
/// silence is a real answer, and the heading has to be able to say so rather
/// than reading `undefined`.
#[test]
fn the_standup_digest_shape_matches_its_typescript_mirror() {
    let filled = knobas_app::standup::StandupDigest {
        yesterday_day: Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 28).expect("a Friday")),
        yesterday: vec![digest_line()],
        today: vec![digest_label_line()],
        blockers: vec![digest_line()],
    };
    let wire = serde_json::to_value(&filled).unwrap();
    assert_shape(
        "StandupDigest",
        &wire,
        &["yesterday_day", "yesterday", "today", "blockers"],
    );
    assert_eq!(
        wire["yesterday_day"],
        serde_json::json!("2026-08-28"),
        "a `NaiveDate` crosses as the `YYYY-MM-DD` the address and the heading use"
    );
    assert_shape("DigestLine", &wire["yesterday"][0], DIGEST_LINE_FIELDS);

    let silent = knobas_app::standup::StandupDigest {
        yesterday_day: None,
        yesterday: Vec::new(),
        today: Vec::new(),
        blockers: Vec::new(),
    };
    let wire = serde_json::to_value(&silent).unwrap();
    assert!(wire["yesterday_day"].is_null());
    for empty in ["yesterday", "today", "blockers"] {
        assert_eq!(
            wire[empty],
            serde_json::json!([]),
            "an empty list is a list, never a missing key"
        );
    }
}

/// The standup protocol's three shapes (#289).
///
/// `publication` and `page_entity_id` are exercised **as `None`** as well as
/// filled, for this file's standing reason: a protocol nobody has published is
/// the ordinary case, and the panel branches on the key being `null` rather
/// than on it being absent.
#[test]
fn the_standup_protocol_shape_matches_its_typescript_mirror() {
    use knobas_app::protocol::{Protocol, Publication, PublishTarget};

    let published = Protocol {
        day: chrono::NaiveDate::from_ymd_opt(2026, 9, 3).expect("a date"),
        note_id: "note:6f1e".to_owned(),
        page_title: "2026-09-03".to_owned(),
        publication: Some(Publication {
            write_id: 12,
            state: knobas_core::write_queue::WriteState::Sent,
            detail: None,
            page_entity_id: Some("confluence:98411".to_owned()),
            linked: true,
        }),
    };
    let wire = serde_json::to_value(&published).unwrap();
    assert_shape(
        "Protocol",
        &wire,
        &["day", "note_id", "page_title", "publication"],
    );
    assert_eq!(
        wire["day"],
        serde_json::json!("2026-09-03"),
        "a `NaiveDate` crosses as the `YYYY-MM-DD` the address and the title use"
    );
    assert_shape(
        "Publication",
        &wire["publication"],
        &["write_id", "state", "detail", "page_entity_id", "linked"],
    );
    assert_eq!(
        wire["publication"]["state"],
        serde_json::json!("sent"),
        "the queue's own vocabulary, which `sources.ts`'s WriteState already declares"
    );

    let unpublished = Protocol {
        publication: None,
        ..published
    };
    let wire = serde_json::to_value(&unpublished).unwrap();
    assert!(
        wire["publication"].is_null(),
        "an unpublished protocol keeps the key as null -- the panel branches on it"
    );

    let waiting = Publication {
        write_id: 12,
        state: knobas_core::write_queue::WriteState::Pending,
        detail: Some("the wiki did not answer".to_owned()),
        page_entity_id: None,
        linked: false,
    };
    let wire = serde_json::to_value(&waiting).unwrap();
    assert!(wire["page_entity_id"].is_null());

    let target = PublishTarget {
        source_id: "confluence".to_owned(),
        parent: "confluence:98400".to_owned(),
    };
    assert_shape(
        "PublishTarget",
        &serde_json::to_value(&target).unwrap(),
        &["source_id", "parent"],
    );
}

/// What filing a ticket from an action item answers with (#289, story 69).
///
/// The `None` case is the one worth pinning: a create still on the queue has
/// no ticket to name, and the panel says so rather than reading `undefined`.
/// Since #779 it also carries the write's state and the queue's sentence.
#[test]
fn the_action_item_ticket_shape_matches_its_typescript_mirror() {
    use knobas_app::commands::entity::ActionItemTicket;

    let filed = ActionItemTicket {
        write_id: 31,
        state: knobas_core::write_queue::WriteState::Sent,
        detail: None,
        ticket_entity_id: Some("jira:PAY-999".to_owned()),
        linked: true,
    };
    assert_shape(
        "ActionItemTicket",
        &serde_json::to_value(&filed).unwrap(),
        &["write_id", "state", "detail", "ticket_entity_id", "linked"],
    );

    // On its way (#779): the queue's word, and no ticket to name yet.
    let queued = ActionItemTicket {
        write_id: 31,
        state: knobas_core::write_queue::WriteState::Pending,
        detail: Some("the tracker did not answer".to_owned()),
        ticket_entity_id: None,
        linked: false,
    };
    let wire = serde_json::to_value(&queued).unwrap();
    assert!(wire["ticket_entity_id"].is_null());
    assert_eq!(wire["state"], "pending");
}

/// The `notify` command's argument (#339). Three strings and no nullable
/// field; the test earns its place by being the one thing that notices a
/// field renamed on one side.
#[test]
fn the_notification_draft_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(knobas_app::notify::NotificationDraft {
        title: "Tidewater (mock)".to_owned(),
        body: "the credential expires on Sunday".to_owned(),
        address: "#/inbox".to_owned(),
    })
    .unwrap();
    assert_shape("NotificationDraft", &wire, &["address", "body", "title"]);
}

/// The `notification:clicked` payload (#339): the address the notification
/// was sent with, and nothing else -- the store reads exactly that key.
#[test]
fn the_notification_clicked_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(knobas_app::notify::NotificationClicked {
        address: "#/entity/gitea:acme%2Fpayouts%23144".to_owned(),
    })
    .unwrap();
    assert_shape("NotificationClicked", &wire, &["address"]);
}

/// The Docs reader's shapes (#859), each against what `entity.ts` declares,
/// with every nullable field `None` somewhere.
#[test]
fn the_docs_shapes_match_their_typescript_mirror() {
    use knobas_app::docs::{
        DocAncestor, DocBacklink, DocExport, DocForYou, DocHit, DocNode, DocPage, DocSnippet,
        DocSpace, DocSpaceRef, DocWork, DocsHits, DocsTree,
    };
    use knobas_source::docs::{
        DocBlock, DocComment, DocSection, DocVersion, Inline, PanelKind, SectionRefusal, TableCell,
    };

    let tree = DocsTree {
        spaces: vec![DocSpace {
            source_id: "confluence".to_owned(),
            key: "OPS".to_owned(),
            name: None,
            pages: vec![DocNode {
                entity_id: "confluence:1".to_owned(),
                title: "On-call handbook".to_owned(),
                parent: None,
            }],
        }],
        last_read: None,
        for_you: vec![DocForYou {
            entity_id: "confluence:2".to_owned(),
            title: "SEPA payout retry design".to_owned(),
            space_key: None,
            by_name: None,
        }],
    };
    let wire = serde_json::to_value(&tree).unwrap();
    assert_shape("DocsTree", &wire, &["for_you", "last_read", "spaces"]);
    assert_shape(
        "DocForYou",
        &wire["for_you"][0],
        &["by_name", "entity_id", "space_key", "title"],
    );
    assert_shape(
        "DocSpace",
        &wire["spaces"][0],
        &["key", "name", "pages", "source_id"],
    );
    assert_shape(
        "DocNode",
        &wire["spaces"][0]["pages"][0],
        &["entity_id", "parent", "title"],
    );

    let page = DocPage {
        entity_id: "confluence:1".to_owned(),
        source_id: "confluence".to_owned(),
        source_name: "Confluence".to_owned(),
        title: "On-call handbook".to_owned(),
        space: Some(DocSpaceRef {
            key: "OPS".to_owned(),
            name: None,
        }),
        ancestors: vec![DocAncestor {
            title: "Ops home".to_owned(),
            entity_id: None,
        }],
        editor: None,
        editor_name: None,
        updated_at: None,
        web_url: None,
        readable: true,
        version: None,
        labels: vec![],
        blocks: vec![DocBlock::Table {
            rows: vec![vec![TableCell {
                header: true,
                text: vec![Inline::Break],
            }]],
        }],
        comments: vec![DocComment {
            author: None,
            author_name: None,
            at: None,
            blocks: vec![],
        }],
        me: vec![],
        sections: vec![DocSection {
            number: 1,
            text: None,
            refusal: Some(SectionRefusal::Table),
        }],
        history: vec![DocVersion {
            number: None,
            by: None,
            by_name: None,
            at: None,
            message: None,
        }],
        backlinks: vec![DocBacklink {
            entity_id: "confluence:3".to_owned(),
            title: "Payout design".to_owned(),
            space_key: None,
        }],
        work: vec![DocWork {
            entity_id: "jira:PAY-231".to_owned(),
            key: "PAY-231".to_owned(),
            title: "Retry".to_owned(),
            status: None,
            linked: true,
        }],
        writes: vec!["comment".to_owned()],
    };
    let wire = serde_json::to_value(&page).unwrap();
    assert_shape(
        "DocPage",
        &wire,
        &[
            "ancestors",
            "backlinks",
            "blocks",
            "comments",
            "editor",
            "editor_name",
            "entity_id",
            "history",
            "labels",
            "me",
            "readable",
            "sections",
            "source_id",
            "source_name",
            "space",
            "title",
            "updated_at",
            "version",
            "web_url",
            "work",
            "writes",
        ],
    );
    assert_shape(
        "DocSection",
        &wire["sections"][0],
        &["number", "refusal", "text"],
    );
    assert_shape(
        "DocVersion",
        &wire["history"][0],
        &["at", "by", "by_name", "message", "number"],
    );
    assert_shape(
        "DocBacklink",
        &wire["backlinks"][0],
        &["entity_id", "space_key", "title"],
    );
    assert_shape(
        "DocWork",
        &wire["work"][0],
        &["entity_id", "key", "linked", "status", "title"],
    );
    let refusals: Vec<String> = [
        SectionRefusal::Table,
        SectionRefusal::Code,
        SectionRefusal::Macro,
        SectionRefusal::Formatting,
    ]
    .iter()
    .map(|r| {
        serde_json::to_value(r)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    })
    .collect();
    assert_same_members(
        &refusals.iter().map(String::as_str).collect::<Vec<_>>(),
        declared_union(MIRROR, "SectionRefusal"),
        "SectionRefusal in entity.ts must be exactly the Rust enum's spellings",
    );

    // The search (#860) and the export (#861).
    let hits = DocsHits {
        total: 1,
        took_ms: 1,
        needles: vec!["retry".to_owned()],
        unknown_tokens: vec![],
        hits: vec![DocHit {
            entity_id: "confluence:2".to_owned(),
            title: "SEPA payout retry design".to_owned(),
            space: Some(DocSpaceRef {
                key: "ENG".to_owned(),
                name: None,
            }),
            path: None,
            snippet: DocSnippet {
                section: None,
                comment_by: None,
                segments: vec![knobas_search::Segment {
                    text: "retry".to_owned(),
                    hit: true,
                }],
            },
            matches: 1,
            editor: None,
            editor_name: None,
            updated_at: None,
        }],
    };
    let found = serde_json::to_value(&hits).unwrap();
    assert_shape(
        "DocsHits",
        &found,
        &["hits", "needles", "took_ms", "total", "unknown_tokens"],
    );
    assert_shape(
        "DocHit",
        &found["hits"][0],
        &[
            "editor",
            "editor_name",
            "entity_id",
            "matches",
            "path",
            "snippet",
            "space",
            "title",
            "updated_at",
        ],
    );
    assert_shape(
        "DocSnippet",
        &found["hits"][0]["snippet"],
        &["comment_by", "section", "segments"],
    );
    let export = DocExport {
        file: "retry-budget.md".to_owned(),
        folder: "/Users/x/Downloads".to_owned(),
    };
    assert_shape(
        "DocExport",
        &serde_json::to_value(&export).unwrap(),
        &["file", "folder"],
    );
    assert_shape("DocSpaceRef", &wire["space"], &["key", "name"]);
    assert_shape(
        "DocAncestor",
        &wire["ancestors"][0],
        &["entity_id", "title"],
    );
    assert_shape(
        "DocComment",
        &wire["comments"][0],
        &["at", "author", "author_name", "blocks"],
    );
    assert_shape(
        "TableCell",
        &wire["blocks"][0]["rows"][0][0],
        &["header", "text"],
    );

    let kinds: Vec<String> = [
        PanelKind::Info,
        PanelKind::Note,
        PanelKind::Warning,
        PanelKind::Tip,
    ]
    .iter()
    .map(|t| {
        serde_json::to_value(t)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    })
    .collect();
    assert_same_members(
        &kinds.iter().map(String::as_str).collect::<Vec<_>>(),
        declared_union(MIRROR, "PanelKind"),
        "PanelKind in entity.ts must be exactly the Rust enum's spellings",
    );

    // The two tagged unions: the `type` tags each declares, against what the
    // Rust enums serialize to.
    let tags = |name: &str| -> Vec<String> {
        let start = MIRROR
            .find(&format!("export type {name} ="))
            .unwrap_or_else(|| panic!("entity.ts declares {name}"));
        let body = &MIRROR[start..];
        let body = &body[..body.find(";\n\n").expect("the union ends")];
        body.match_indices("type: \"")
            .map(|(at, m)| {
                let rest = &body[at + m.len()..];
                rest[..rest.find('"').unwrap()].to_owned()
            })
            .collect()
    };
    let tag = |v: serde_json::Value| v["type"].as_str().unwrap().to_owned();
    let text = || vec![Inline::text("x")];
    let blocks = [
        DocBlock::Heading {
            level: 2,
            text: text(),
        },
        DocBlock::Paragraph { text: text() },
        DocBlock::List {
            ordered: false,
            items: vec![],
        },
        DocBlock::Code {
            code: String::new(),
            language: None,
        },
        DocBlock::Table { rows: vec![] },
        DocBlock::Panel {
            kind: PanelKind::Info,
            title: None,
            blocks: vec![],
        },
        DocBlock::Placeholder {
            name: "roadmap".to_owned(),
        },
    ];
    let rust: Vec<String> = blocks
        .iter()
        .map(|b| tag(serde_json::to_value(b).unwrap()))
        .collect();
    assert_same_members(
        &rust.iter().map(String::as_str).collect::<Vec<_>>(),
        tags("DocBlock"),
        "DocBlock's tags in entity.ts must be exactly the Rust enum's",
    );
    let inlines = [
        Inline::text("x"),
        Inline::Link {
            text: String::new(),
            href: String::new(),
        },
        Inline::PageLink {
            text: String::new(),
            title: String::new(),
            space: None,
            entity_id: None,
        },
        Inline::Issue {
            key: String::new(),
            entity_id: None,
            title: None,
            status: None,
        },
        Inline::Mention {
            user: None,
            name: None,
            me: false,
        },
        Inline::Break,
    ];
    let rust: Vec<String> = inlines
        .iter()
        .map(|i| tag(serde_json::to_value(i).unwrap()))
        .collect();
    assert_same_members(
        &rust.iter().map(String::as_str).collect::<Vec<_>>(),
        tags("DocInline"),
        "DocInline's tags in entity.ts must be exactly the Rust enum's",
    );
}

/// #866's link sets: every DTO the six commands answer or take, and the
/// commands by their registered names.
#[test]
fn the_link_set_shapes_match_their_typescript_mirror() {
    use knobas_app::link_sets::{
        LinkSetDetail, LinkSetDraft, LinkSetMember, LinkSetRow, LinkSetRule, MemberRow, RuleRow,
        RuleTicket,
    };
    let row = LinkSetRow {
        id: "payout-stack".to_owned(),
        name: "Payout stack".to_owned(),
        description: String::new(),
        members: vec![MemberRow {
            kind: "repo".to_owned(),
            entity_id: "gitea:tidewater/payout-service".to_owned(),
            title: None,
        }],
        rules: vec![RuleRow {
            kind: "epic".to_owned(),
            value: "jira:PAY-200".to_owned(),
            title: None,
        }],
        tickets: 3,
        mine: 1,
    };
    let fields = [
        "description",
        "id",
        "members",
        "mine",
        "name",
        "rules",
        "tickets",
    ];
    let wire = serde_json::to_value(&row).unwrap();
    assert_shape("LinkSetRow", &wire, &fields);
    assert_shape(
        "LinkSetMemberRow",
        &wire["members"][0],
        &["entity_id", "kind", "title"],
    );
    assert_shape(
        "LinkSetRuleRow",
        &wire["rules"][0],
        &["kind", "title", "value"],
    );
    let detail = serde_json::to_value(LinkSetDetail {
        set: row,
        shows_on: vec![RuleTicket {
            entity_id: "jira:PAY-231".to_owned(),
            title: "PAY-231".to_owned(),
            mine: true,
        }],
    })
    .unwrap();
    assert_eq!(
        detail["id"], "payout-stack",
        "the row is flattened into the detail"
    );
    assert_shape(
        "RuleTicket",
        &detail["shows_on"][0],
        &["entity_id", "mine", "title"],
    );
    let draft = serde_json::to_value(LinkSetDraft {
        id: None,
        name: "Payout stack".to_owned(),
        description: String::new(),
        members: vec![LinkSetMember {
            kind: "page".to_owned(),
            entity_id: "confluence:1".to_owned(),
        }],
        rules: vec![LinkSetRule {
            kind: "manual".to_owned(),
            value: "jira:PAY-231".to_owned(),
        }],
    })
    .unwrap();
    assert_shape(
        "LinkSetDraft",
        &draft,
        &["description", "id", "members", "name", "rules"],
    );
    assert_shape(
        "LinkSetMember",
        &draft["members"][0],
        &["entity_id", "kind"],
    );
    assert_shape("LinkSetRule", &draft["rules"][0], &["kind", "value"]);
    for command in [
        "\"link_sets\"",
        "\"link_set\"",
        "\"save_link_set\"",
        "\"delete_link_set\"",
        "\"preview_link_set_rule\"",
        "\"hide_link_set_member\"",
    ] {
        assert!(MIRROR.contains(command), "{command} is not invoked");
    }
}
