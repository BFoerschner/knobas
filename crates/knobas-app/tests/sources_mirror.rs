//! Every DTO stream F puts on the bridge, against `app/src/lib/ipc/sources.ts`.
//!
//! **The exact key set, not a `contains` walk over a remembered list.** A
//! list-walking test sees only the fields somebody thought to list, so a Rust
//! field added with no TypeScript counterpart is invisible to it -- the trap the
//! M0 carry-over records, and the direction this surface breaks first. Each
//! test serializes (or deserializes) a real instance, asserts the key set, and
//! only then looks for the keys in the mirror.
//!
//! The *input* DTOs are checked from the other side: a JSON object shaped the
//! way the mirror declares it must deserialize into the Rust type. A renamed
//! field there is a form whose submissions Tauri rejects at run time.

const MIRROR: &str = include_str!("../../../app/src/lib/ipc/sources.ts");
const ENTITY_MIRROR: &str = include_str!("../../../app/src/lib/ipc/entity.ts");

/// The keys `value` serializes to must be exactly `expected`, and exactly what
/// the mirror's `interface <name>` declares -- both directions.
///
/// `knobas_sync::mirror` and not a copy: the same check is needed by three
/// types in that crate, and the version this file used to carry was the fourth
/// hand-written one. Three of the four were wrong in the same way.
fn assert_shape(name: &str, value: &serde_json::Value, expected: &[&str]) {
    knobas_sync::mirror::assert_shape(MIRROR, name, value, expected);
}

fn health() -> knobas_sync::config::CredentialHealth {
    knobas_sync::config::CredentialHealth {
        source_id: "mock".to_owned(),
        state: knobas_sync::config::AuthState::Ok,
        checked_at: Some(chrono::Utc::now()),
        detail: Some("Jira 9.4".to_owned()),
        secret_expires_at: Some(chrono::Utc::now()),
    }
}

#[test]
fn the_source_summary_shape_matches_its_typescript_mirror() {
    let summary = knobas_app::sources::SourceSummary {
        id: "jira".to_owned(),
        adapter_kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: "https://jira.example".to_owned(),
        enabled: true,
        sync_interval_secs: 300,
        config: serde_json::json!({ "flavor": "datacenter" }),
        health: health(),
        last_run: None,
        next_run_at: Some(chrono::Utc::now()),
        item_count: 12,
        kinds: knobas_source::SourceDescriptor {
            ..knobas_source_mock::descriptor_template()
        }
        .entity_kinds,
    };
    assert_shape(
        "SourceSummary",
        &serde_json::to_value(&summary).unwrap(),
        &[
            "adapter_kind",
            "base_url",
            "config",
            "display_name",
            "enabled",
            "health",
            "id",
            "item_count",
            "kinds",
            "last_run",
            "next_run_at",
            "sync_interval_secs",
        ],
    );
}

#[test]
fn the_connection_report_shape_matches_its_typescript_mirror() {
    let report = knobas_app::sources::ConnectionReport {
        ok: false,
        account: Some("mara.lindqvist".to_owned()),
        server_version: Some("9.4.0".to_owned()),
        secret_expires_at: Some(chrono::Utc::now()),
        error: Some("the credential was refused".to_owned()),
        code: Some(knobas_app::IpcErrorCode::Unauthorized),
        elapsed_ms: 42,
    };
    let wire = serde_json::to_value(&report).unwrap();
    assert_shape(
        "ConnectionReport",
        &wire,
        &[
            "account",
            "code",
            "elapsed_ms",
            "error",
            "ok",
            "secret_expires_at",
            "server_version",
        ],
    );
    // The one field the UI branches on, in the spelling the mirror's union
    // declares -- `unauthorized` is what turns *Test* into *Re-enter*.
    assert_eq!(wire["code"], serde_json::json!("unauthorized"));
}

#[test]
fn the_db_stats_shape_matches_its_typescript_mirror() {
    let stats = knobas_sync::stats::DbStats {
        db_bytes: 1_024,
        entity_count: 3,
        item_count: 4,
        per_source: vec![knobas_sync::stats::SourceCount {
            source_id: "mock".to_owned(),
            items: 4,
            synced_at: Some(chrono::Utc::now()),
        }],
        oldest_synced_at: Some(chrono::Utc::now()),
        newest_synced_at: Some(chrono::Utc::now()),
    };
    let wire = serde_json::to_value(&stats).unwrap();
    assert_shape(
        "DbStats",
        &wire,
        &[
            "db_bytes",
            "entity_count",
            "item_count",
            "newest_synced_at",
            "oldest_synced_at",
            "per_source",
        ],
    );
    assert_shape(
        "SourceCount",
        &wire["per_source"][0],
        &["items", "source_id", "synced_at"],
    );
}

#[test]
fn the_sync_run_row_shape_matches_its_typescript_mirror() {
    let row = serde_json::to_value(knobas_sync::run_log::SyncRunRow {
        id: 1,
        source_id: "mock".to_owned(),
        trigger: knobas_sync::SyncTrigger::Manual,
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
        outcome: Some(knobas_sync::SyncOutcome::Ok),
        upserted: 21,
        deleted: 0,
        swept: 0,
        error: None,
        cursor_after: Some("c".to_owned()),
    })
    .unwrap();
    assert_shape(
        "SyncRunRow",
        &row,
        &[
            "cursor_after",
            "deleted",
            "error",
            "finished_at",
            "id",
            "outcome",
            "source_id",
            "started_at",
            "swept",
            "trigger",
            "upserted",
        ],
    );
}

#[test]
fn the_descriptor_shape_matches_its_typescript_mirror() {
    let wire = serde_json::to_value(knobas_source_mock::descriptor_template()).unwrap();
    assert_shape(
        "SourceDescriptor",
        &wire,
        &[
            "adapter_kind",
            "adapter_version",
            "auth_methods",
            "capabilities",
            "config_schema",
            "entity_kinds",
            "full_sync_exhaustive",
            "id",
            "name",
            "write_ops",
        ],
    );
    // `KindInfo` rides inside it, and `entity.ts` is where the mirror declares
    // it -- so the keys are looked for there rather than in `sources.ts`.
    knobas_sync::mirror::assert_shape(
        ENTITY_MIRROR,
        "KindInfo",
        &wire["entity_kinds"][0],
        &["id", "label", "monogram", "plural"],
    );
}

/// Every `AuthMethod` the mirror's union has to name.
///
/// The *serialized* spelling, not the Rust identifier: `AuthMethod` is
/// PascalCase on the wire while every other enum here is snake_case, and a
/// mirror that guessed wrong would reject a saved source's auth method.
#[test]
fn the_auth_methods_match_their_typescript_mirror() {
    for method in [
        knobas_source::AuthMethod::UserPassword,
        knobas_source::AuthMethod::Pat,
        knobas_source::AuthMethod::ApiToken,
        knobas_source::AuthMethod::OAuth,
    ] {
        let wire = serde_json::to_string(&method).unwrap();
        assert!(
            MIRROR.contains(&wire),
            "AuthMethod {wire} is missing from app/src/lib/ipc/sources.ts"
        );
    }
}

// -- the input side: what the form sends must decode ---------------------------

/// The mirror's `NewSource` shape decodes into the Rust one.
///
/// From the other direction on purpose: a serialize-and-compare test cannot see
/// an input DTO at all, and a renamed field there is a form whose submissions
/// Tauri rejects at run time with an argument error nobody wrote a branch for.
#[test]
fn the_add_source_payload_the_mirror_describes_decodes() {
    let payload = serde_json::json!({
        "id": "jira-eu",
        "adapter_kind": "jira",
        "display_name": "Jira EU",
        "base_url": "https://jira.eu.example",
        "auth_kind": "Pat",
        "config": { "flavor": "datacenter" },
        "secret": { "value": "pat" },
        "sync_interval_secs": 300,
        "enabled": true
    });
    let decoded: knobas_app::sources::NewSource =
        serde_json::from_value(payload).expect("the mirror's NewSource decodes");
    assert_eq!(decoded.id, "jira-eu");
    assert_eq!(decoded.adapter_kind, "jira");
    assert_eq!(decoded.auth_kind, knobas_source::AuthMethod::Pat);
    assert_eq!(decoded.sync_interval_secs, 300);
    assert!(decoded.enabled);
    // ...and it still redacts on the way through a log line.
    assert!(!format!("{decoded:?}").contains("pat"), "{decoded:?}");
}

#[test]
fn the_patch_and_draft_payloads_the_mirror_describes_decode() {
    // Every field optional, and an empty object is a no-op patch -- which is
    // what `SourcePatch`'s `?:` fields promise.
    let empty: knobas_app::sources::SourcePatch =
        serde_json::from_value(serde_json::json!({})).expect("an empty patch decodes");
    assert!(empty.display_name.is_none() && empty.enabled.is_none());

    let full: knobas_app::sources::SourcePatch = serde_json::from_value(serde_json::json!({
        "display_name": "Renamed",
        "base_url": "https://x.example",
        "config": {},
        "sync_interval_secs": 600,
        "enabled": false
    }))
    .expect("a full patch decodes");
    assert_eq!(full.display_name.as_deref(), Some("Renamed"));
    assert_eq!(full.sync_interval_secs, Some(600));

    let draft: knobas_app::sources::SourceDraft = serde_json::from_value(serde_json::json!({
        "source_id": null,
        "adapter_kind": "mock",
        "base_url": "",
        "auth_kind": "Pat",
        "config": {},
        "secret": null
    }))
    .expect("the mirror's SourceDraft decodes");
    assert!(draft.source_id.is_none());
    assert_eq!(
        draft.auth_kind,
        knobas_source::AuthMethod::Pat,
        "the frozen shape's `AuthMethod`, not an Option -- a saved source is \
         tested against its stored auth, not the draft's"
    );
    assert!(
        serde_json::from_value::<knobas_app::sources::SourceDraft>(serde_json::json!({
            "source_id": null, "adapter_kind": "mock", "base_url": "",
            "auth_kind": null, "config": {}, "secret": null
        }))
        .is_err(),
        "a null auth_kind is not the frozen shape and must not decode"
    );
}
