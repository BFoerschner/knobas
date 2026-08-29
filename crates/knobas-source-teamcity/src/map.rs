//! TeamCity records to [`SyncItem`]s.
//!
//! Two rules from interfaces §4.1 shape everything here: `payload` is the raw
//! record **verbatim** (spec §3a keeps re-mapping possible without a re-sync),
//! and `updated_at` is the source's own timestamp, never `now()`.

use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;

use crate::rest::{Build, BuildType, parse_ts};
use crate::{KIND_BUILD, KIND_BUILD_CONFIG};

/// `build:<buildId>` -- the numeric id, because build *numbers* are
/// per-configuration and a configuration's counter can be reset.
pub(crate) fn build_key(id: i64) -> String {
    format!("build:{id}")
}

/// `buildType:<buildTypeId>`.
pub(crate) fn build_config_key(id: &str) -> String {
    format!("buildType:{id}")
}

/// Join the non-empty parts into the blob FTS indexes (same shape as
/// `knobas-source-mock`, interfaces §4.1).
fn body_text(parts: impl IntoIterator<Item = Option<String>>) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `<project> / <name>`, falling back to whatever the record actually carries.
fn config_title(id: &str, name: Option<&str>, project: Option<&str>) -> String {
    let name = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(id);
    match project.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => format!("{p} / {name}"),
        None => name.to_owned(),
    }
}

/// The one line that says what happened to a build: `finished SUCCESS`,
/// `running`, `finished canceled`.
///
/// **`finished UNKNOWN` is rewritten, and nothing else is** (issue #105).
/// TeamCity has no `canceled` state; a canceled build is a *finished* build
/// with `status: "UNKNOWN"`, and `statusText` is where the server says which
/// -- 20 of 20 sampled on JetBrains' public instance, which is also the
/// predicate `live_teamcity.rs` certifies against the real server. Composed
/// verbatim, every canceled build in the mirror would carry the literal string
/// `finished UNKNOWN` in the element the launcher shows and FTS indexes:
/// nobody searches "UNKNOWN", and in a snippet it reads as a fault in knobas
/// rather than as a fact about the build.
///
/// The rewrite is a *rendering*, not a re-reading. `payload` keeps the record
/// with its `UNKNOWN` verbatim (spec §3a), `statusText` is indexed
/// independently, and no watermark, filter or key depends on this string.
///
/// This path was unreachable until issue #105: the two queries that produce
/// items sent no dimension that could return a canceled build, so nothing
/// could be mapped from one. Deciding the word became unavoidable the moment
/// it could be read.
fn status_element(state: Option<&str>, status: Option<&str>) -> Option<String> {
    match (state, status) {
        (Some("finished"), Some("UNKNOWN")) => Some("finished canceled".to_owned()),
        (Some(state), Some(status)) => Some(format!("{state} {status}")),
        (Some(state), None) => Some(state.to_owned()),
        (None, status) => status.map(str::to_owned),
    }
}

pub(crate) fn build_item(source_id: &str, raw: &serde_json::Value, b: &Build) -> SyncItem {
    let nested = b.build_type.as_ref();
    let type_id = b
        .build_type_id
        .as_deref()
        .or_else(|| nested.map(|t| t.id.as_str()))
        .unwrap_or("build");
    let config = config_title(
        type_id,
        nested.and_then(|t| t.name.as_deref()),
        nested.and_then(|t| t.project_name.as_deref()),
    );
    // The build *number* names it to a human; the id keeps the title unique
    // when a queued build has none yet.
    let number = b.number.clone().unwrap_or_else(|| b.id.to_string());
    let title = format!("{config} #{number}");
    let status = status_element(b.state.as_deref(), b.status.as_deref());
    let author = b
        .triggered
        .as_ref()
        .and_then(|t| t.user.as_ref())
        .and_then(|u| u.username.clone());
    SyncItem {
        entity: EntityRef::new(source_id, &build_key(b.id)),
        kind: KIND_BUILD.to_owned(),
        title: title.clone(),
        body_text: body_text([
            Some(title),
            status,
            b.status_text.clone(),
            b.branch_name.clone(),
            // What a running build is doing right now: the one line that says
            // *why* it is still running, and the only searchable thing a
            // freshly started build has beyond its number.
            b.running_info
                .as_ref()
                .and_then(|r| r.current_stage_text.clone()),
            author.as_ref().map(|u| format!("triggered by {u}")),
        ]),
        // `triggered.user.username` is the only place TeamCity names the
        // person who started a build; `None` is a build no person started --
        // a VCS, schedule or dependency trigger -- which is **effectively all
        // of them** on a real server.
        //
        // Measured, not estimated (issue #106): 100 of 100 of the newest
        // finished builds on JetBrains' public instance name no user, over an
        // earlier sample of 300 with `triggered.user.username` null
        // throughout. So `author` is empty for effectively every TeamCity
        // build, and #39's `author:` and `@me` tokens match almost nothing for
        // this source. **That is the field being correct and sparse, not
        // broken**, and it was ruled to stay that way.
        //
        // The committer of the change a build ran on is not a fallback:
        // measured the same day, 92 of 100 builds carried zero changes at all
        // (87 of 100 were `snapshotDependency`-triggered), and the 8 that did
        // named their committers as VCS display strings -- "artem tikhomirov"
        // -- while `change.user.username`, the TeamCity account, was null
        // throughout. Interfaces §4.1 pins `author` to "the source's username
        // string", and `vocab.rs` resolves `@me` against the source config's
        // TeamCity login, so a display name could never match it. Filling
        // ~8% of builds with a value from a different name-space would make
        // "author" mean two things depending on the build. The legitimate
        // moment to revisit it is M2's username-mapping work, which §4.1's own
        // parenthesis already schedules.
        author,
        // Finished, else started, else queued: the newest thing that happened
        // to this build.
        updated_at: [&b.finish_date, &b.start_date, &b.queued_date]
            .into_iter()
            .flatten()
            .find_map(|raw| parse_ts(raw)),
        payload: raw.clone(),
        web_url: b.web_url.clone(),
        // TeamCity's cleanup rules remove old builds, but M1 has no deletion
        // channel: the adapter never sees the removal, so it never claims one.
        deleted: false,
    }
}

pub(crate) fn build_config_item(
    source_id: &str,
    raw: &serde_json::Value,
    bt: &BuildType,
) -> SyncItem {
    let title = config_title(&bt.id, bt.name.as_deref(), bt.project_name.as_deref());
    SyncItem {
        entity: EntityRef::new(source_id, &build_config_key(&bt.id)),
        kind: KIND_BUILD_CONFIG.to_owned(),
        title: title.clone(),
        // The id is in the blob on purpose: it is what a build parameter, a
        // commit message or a runbook names, so it has to be searchable.
        body_text: body_text([Some(title), Some(bt.id.clone()), bt.description.clone()]),
        author: None,
        // TeamCity dates no configuration, and inventing `now()` (interfaces
        // §4.1) would make every configuration the newest thing in the
        // launcher on every sync.
        updated_at: None,
        payload: raw.clone(),
        web_url: bt.web_url.clone(),
        deleted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished() -> (serde_json::Value, Build) {
        let raw = serde_json::json!({
            "id": 1187, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
            "state": "finished", "status": "FAILURE",
            "statusText": "Tests failed: 1 (1 new), passed: 41",
            "branchName": "feature/PAY-231-sepa-retry",
            "webUrl": "https://ci.example.com/build/1187",
            "startDate": "20260822T100600+0000", "finishDate": "20260822T101018+0000",
            "buildType": { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                           "projectId": "Payout", "projectName": "Payout" },
            "triggered": { "user": { "username": "mara.lindqvist" } }
        });
        let rec = serde_json::from_value(raw.clone()).expect("build");
        (raw, rec)
    }

    #[test]
    fn a_finished_build_maps_to_a_build_item() {
        let (raw, b) = finished();
        let it = build_item("teamcity", &raw, &b);
        // The key is the numeric build id, not the build *number*: numbers are
        // per-configuration and get reset, ids are server-wide and monotonic.
        assert_eq!(it.entity.to_string(), "teamcity:build:1187");
        assert_eq!(it.entity.namespace, "teamcity");
        assert_eq!(it.entity.key, "build:1187");
        assert_eq!(
            knobas_core::entity::EntityRef::parse("teamcity:build:1187").expect("parses"),
            it.entity,
            "EntityRef splits on the first ':' only, so a prefixed key round trips"
        );
        assert_eq!(it.kind, "build");
        assert_eq!(it.title, "Payout / Integration Tests #1187");
        assert_eq!(it.author.as_deref(), Some("mara.lindqvist"));
        assert_eq!(
            it.updated_at.expect("finished date").to_rfc3339(),
            "2026-08-22T10:10:18+00:00"
        );
        assert_eq!(
            it.web_url.as_deref(),
            Some("https://ci.example.com/build/1187")
        );
        assert!(!it.deleted);
        // What FTS indexes: the status text is the reason a build is searched
        // for at all.
        assert!(it.body_text.contains("Payout / Integration Tests #1187"));
        assert!(it.body_text.contains("Tests failed: 1 (1 new), passed: 41"));
        assert!(it.body_text.contains("feature/PAY-231-sepa-retry"));
        assert!(it.body_text.contains("FAILURE"));
        assert!(it.body_text.contains("triggered by mara.lindqvist"));
        // The raw record, verbatim (spec §3a: a later mapping re-projects it).
        assert_eq!(it.payload, raw);
    }

    /// `finishDate` beats `startDate` beats `queuedDate`, and the test can
    /// witness it: all three are present and all three are different, so a
    /// mapping that picked the wrong one reads a different timestamp rather
    /// than the same one.
    #[test]
    fn the_newest_of_the_three_dates_wins() {
        let raw = serde_json::json!({
            "id": 1, "state": "finished",
            "queuedDate": "20260822T100000+0000",
            "startDate": "20260822T100600+0000",
            "finishDate": "20260822T101018+0000"
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        assert_eq!(
            build_item("teamcity", &raw, &b)
                .updated_at
                .expect("finished")
                .to_rfc3339(),
            "2026-08-22T10:10:18+00:00"
        );

        let raw = serde_json::json!({
            "id": 1, "state": "running",
            "queuedDate": "20260822T100000+0000",
            "startDate": "20260822T100600+0000"
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        assert_eq!(
            build_item("teamcity", &raw, &b)
                .updated_at
                .expect("started")
                .to_rfc3339(),
            "2026-08-22T10:06:00+00:00"
        );
    }

    #[test]
    fn a_running_build_is_dated_by_its_start() {
        let raw = serde_json::json!({
            "id": 1188, "number": "1188", "buildTypeId": "Payout_Build",
            "state": "running", "status": "SUCCESS", "percentageComplete": 60,
            "queuedDate": "20260822T114000+0000", "startDate": "20260822T114500+0000",
            "running-info": { "currentStageText": "step 3/5 `cargo test`" }
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert_eq!(
            it.updated_at.expect("start date").to_rfc3339(),
            "2026-08-22T11:45:00+00:00"
        );
        // No nested buildType: the title falls back to the configuration id,
        // which is the only name the record carries.
        assert_eq!(it.title, "Payout_Build #1188");
        assert_eq!(it.web_url, None);
        assert_eq!(it.author, None, "nothing triggered it in this record");
        assert!(it.body_text.contains("running"));
        assert!(
            it.body_text.contains("step 3/5 `cargo test`"),
            "the stage is what a running build is searchable by: {:?}",
            it.body_text
        );
    }

    #[test]
    fn a_queued_build_is_dated_by_its_queue_time() {
        let raw = serde_json::json!({ "id": 1190, "buildTypeId": "Payout_Build",
                                      "state": "queued", "queuedDate": "20260822T120000+0000" });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert_eq!(
            it.updated_at.expect("queued date").to_rfc3339(),
            "2026-08-22T12:00:00+00:00"
        );
        // No number yet -- a queued build has none. The id keeps the title
        // unambiguous.
        assert_eq!(it.title, "Payout_Build #1190");
    }

    /// A canceled build reads `finished canceled`, and the word `UNKNOWN`
    /// never reaches a user (issue #105).
    ///
    /// TeamCity has no `canceled` state: a canceled build is `state:
    /// "finished"` with `status: "UNKNOWN"`, and `statusText` is where it says
    /// what happened -- 20 of 20 sampled on JetBrains' public instance, and
    /// the predicate `live_teamcity.rs` certifies. Composed verbatim, that is
    /// the string `finished UNKNOWN` in the title-adjacent status element of
    /// every canceled build in the mirror. Nobody searches "UNKNOWN", and read
    /// in a launcher snippet it looks like a fault in knobas rather than a
    /// fact about the build.
    ///
    /// Nothing is lost by rewording it: the raw record keeps `UNKNOWN`
    /// verbatim in `payload` (spec §3a), and `statusText` is indexed
    /// separately and independently.
    #[test]
    fn a_canceled_build_reads_finished_canceled_and_never_unknown() {
        let raw = serde_json::json!({
            "id": 1191, "number": "1191", "buildTypeId": "Payout_Build",
            "state": "finished", "status": "UNKNOWN", "statusText": "Canceled",
            "branchName": "feature/PAY-231-sepa-retry",
            "startDate": "20260822T100600+0000", "finishDate": "20260822T101018+0000"
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert!(
            it.body_text.contains("finished canceled"),
            "a canceled build is searchable by the word a user would type: {:?}",
            it.body_text
        );
        assert!(
            !it.body_text.contains("UNKNOWN"),
            "`UNKNOWN` must not reach a user through the status element: {:?}",
            it.body_text
        );
        assert!(
            it.body_text.contains("Canceled"),
            "TeamCity's own statusText is still indexed: {:?}",
            it.body_text
        );
        assert_eq!(
            it.payload, raw,
            "the record keeps its UNKNOWN verbatim -- only the rendering changed"
        );
    }

    /// The rewording is narrow on purpose: it fires on a **finished** build
    /// with an `UNKNOWN` status, which is the one combination TeamCity uses
    /// for a cancellation, and leaves every other pairing composed verbatim.
    ///
    /// Without the state half, an `UNKNOWN` on any other state would be
    /// relabelled a cancellation, which is a claim the server never made.
    #[test]
    fn nothing_but_a_finished_unknown_is_relabelled() {
        for (state, status, expected) in [
            ("finished", "SUCCESS", "finished SUCCESS"),
            ("finished", "FAILURE", "finished FAILURE"),
            ("finished", "ERROR", "finished ERROR"),
            ("running", "UNKNOWN", "running UNKNOWN"),
            ("queued", "UNKNOWN", "queued UNKNOWN"),
        ] {
            let raw = serde_json::json!({ "id": 1, "state": state, "status": status });
            let b: Build = serde_json::from_value(raw.clone()).expect("build");
            let it = build_item("teamcity", &raw, &b);
            assert!(
                it.body_text.contains(expected),
                "{state}/{status} must compose verbatim as {expected:?}: {:?}",
                it.body_text
            );
            assert!(
                !it.body_text.contains("canceled"),
                "{state}/{status} is not a cancellation: {:?}",
                it.body_text
            );
        }
    }

    /// A build the server dated in a way this adapter cannot read gets no
    /// timestamp rather than `now()`: a fabricated one would put the build at
    /// the top of "recent items" on every sync.
    #[test]
    fn an_undated_build_has_no_updated_at() {
        let raw = serde_json::json!({ "id": 1, "state": "queued", "queuedDate": "soon" });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        assert_eq!(build_item("teamcity", &raw, &b).updated_at, None);
    }

    #[test]
    fn a_build_configuration_maps_to_a_build_config_item() {
        let raw = serde_json::json!({
            "id": "Payout_IntegrationTests", "name": "Integration Tests",
            "projectId": "Payout", "projectName": "Payout",
            "description": "Runs the SEPA suite",
            "webUrl": "https://ci.example.com/buildConfiguration/Payout_IntegrationTests"
        });
        let bt: BuildType = serde_json::from_value(raw.clone()).expect("buildType");
        let it = build_config_item("teamcity", &raw, &bt);
        assert_eq!(
            it.entity.to_string(),
            "teamcity:buildType:Payout_IntegrationTests"
        );
        assert_eq!(it.kind, "build_config");
        assert_eq!(it.title, "Payout / Integration Tests");
        assert!(it.body_text.contains("Runs the SEPA suite"));
        assert!(
            it.body_text.contains("Payout_IntegrationTests"),
            "the id is searchable"
        );
        assert_eq!(
            it.web_url.as_deref(),
            Some("https://ci.example.com/buildConfiguration/Payout_IntegrationTests")
        );
        assert_eq!(it.author, None);
        // TeamCity gives a configuration no timestamp, and `now()` is
        // forbidden (interfaces §4.1) -- so it has none.
        assert_eq!(it.updated_at, None);
        assert_eq!(it.payload, raw);
        assert!(!it.deleted);
    }

    /// A configuration with nothing but an id still maps: no fixture
    /// configuration has a `description`, and a real server may send no
    /// `projectName` either.
    #[test]
    fn a_bare_build_configuration_still_maps() {
        let raw = serde_json::json!({ "id": "Ledger_Deploy_Staging" });
        let bt: BuildType = serde_json::from_value(raw.clone()).expect("buildType");
        let it = build_config_item("teamcity", &raw, &bt);
        assert_eq!(it.title, "Ledger_Deploy_Staging");
        assert_eq!(it.entity.key, "buildType:Ledger_Deploy_Staging");
        assert!(!it.body_text.is_empty());
    }

    /// An instance named `teamcity-eu` namespaces its items to itself, or two
    /// TeamCitys would overwrite each other's rows (interfaces §4.1, P10).
    #[test]
    fn items_are_namespaced_to_the_instance_not_the_adapter_kind() {
        let (raw, b) = finished();
        assert_eq!(
            build_item("teamcity-eu", &raw, &b).entity.to_string(),
            "teamcity-eu:build:1187"
        );
        let bt: BuildType =
            serde_json::from_value(serde_json::json!({ "id": "Payout_Build" })).expect("bt");
        assert_eq!(
            build_config_item("teamcity-eu", &serde_json::json!({}), &bt)
                .entity
                .to_string(),
            "teamcity-eu:buildType:Payout_Build"
        );
    }

    /// The key is the build **id**, and the title carries the build
    /// **number**: they are different identifiers, and this record makes them
    /// different values so a mapping that confused them cannot pass.
    ///
    /// Numbers are per-configuration and a configuration's counter can be
    /// reset, so two builds on one server can share a number. Keying on it
    /// would make two builds one row.
    #[test]
    fn the_key_is_the_id_and_the_title_is_the_number() {
        let raw = serde_json::json!({
            "id": 5001, "number": "42", "buildTypeId": "Payout_Build",
            "state": "finished", "status": "SUCCESS",
            "finishDate": "20260822T101018+0000"
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert_eq!(it.entity.key, "build:5001", "the server-wide id");
        assert_eq!(it.title, "Payout_Build #42", "the human-facing number");
    }

    /// The two key spaces must not collide: a build id and a configuration id
    /// both go into one `EntityRef` key space, and only the prefix separates
    /// them.
    #[test]
    fn a_build_and_a_configuration_cannot_share_a_key() {
        assert_ne!(build_key(7), build_config_key("7"));
        assert!(build_key(7).starts_with("build:"));
        assert!(build_config_key("7").starts_with("buildType:"));
    }
}
