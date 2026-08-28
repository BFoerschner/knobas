//! TeamCity's wire shapes, the `fields=` selectors that produce them, and the
//! locator strings that select them. Pure data: nothing here performs I/O.
//!
//! # Why the selectors are narrower than the structs
//!
//! The structs accept everything a real TeamCity can send, because
//! `SyncItem::payload` keeps the record verbatim and a future selector should
//! not need a parse change. The **selectors** are narrower, and the rule is
//! exact in both directions: a selector asks for **every** name the mapping
//! reads and for **no** name it does not.
//!
//! The first half is the one with teeth. A name the mapping reads and the
//! selector omits is a field that is silently always `None` -- not a parse
//! error, not a violation, just a value that never arrives. That is what kept
//! `SyncItem::author` empty for every build until `triggered` was added here,
//! and it is why `the_field_selectors_cover_everything_the_mapping_reads`
//! exists.
//!
//! The second half is budget rather than correctness, with one sharp edge:
//! `knobas-mockd` validates `fields=` against a closed set of names and
//! answers an unknown one with 400 + an `UnknownField` violation (mockd
//! deviation 6), so a *misspelled* name fails loudly in `tests/mockd.rs`. A
//! correctly spelled name for something nothing reads does not fail anywhere
//! -- mockd serves `href` and `paused` quite happily -- it just costs response
//! size and puts a field in `payload` that no reader will ever look at. Hence
//! `triggered(user(username))` rather than
//! `triggered(type,date,user(username,name))`, which mockd would also serve.

use chrono::{DateTime, Utc};

/// What `/app/rest/server` is asked for -- the two fields
/// [`ConnectionInfo`](knobas_source::ConnectionInfo) shows.
pub(crate) const SERVER_FIELDS: &str = "version,buildNumber";

/// What `/app/rest/users/current` is asked for: whom knobas is authenticated
/// as, which is what *Test connection* puts on screen (P4).
pub(crate) const USER_FIELDS: &str = "username,name";

/// What `/app/rest/buildTypes` is asked for. Without an explicit `fields=`,
/// real TeamCity answers a hyperlink stub (`id`, `href`) and mockd answers 400
/// -- the parameter is mandatory on the collections.
pub(crate) const BUILD_TYPE_FIELDS: &str =
    "count,buildType(id,name,projectId,projectName,description,webUrl)";

/// What `/app/rest/builds` is asked for. The nested `buildType(...)` is what
/// makes client-side project scoping possible: the locator grammar has no
/// project dimension.
pub(crate) const BUILD_FIELDS: &str = concat!(
    "count,build(id,number,buildTypeId,state,status,statusText,branchName,webUrl,",
    "queuedDate,startDate,finishDate,",
    "buildType(id,name,projectId,projectName,webUrl),",
    "running-info(percentageComplete,currentStageText),",
    "triggered(user(username)))"
);

/// Every TeamCity list response: `{count, href, nextHref, <element>: [...]}`.
/// One type for both collections -- the element key is the only difference.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ListEnvelope {
    #[serde(default, rename = "build", alias = "buildType")]
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Server {
    pub version: Option<String>,
    pub build_number: Option<String>,
}

/// `/app/rest/users/current`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentUser {
    pub username: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildType {
    pub id: String,
    pub name: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// Prose a human wrote about the configuration, and part of what
    /// `map::build_config_item` puts in the search blob.
    pub description: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Build {
    pub id: i64,
    pub number: Option<String>,
    pub build_type_id: Option<String>,
    /// `queued` | `running` | `finished`.
    pub state: Option<String>,
    /// `SUCCESS` | `FAILURE` | `ERROR` | `UNKNOWN`.
    pub status: Option<String>,
    pub status_text: Option<String>,
    pub branch_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_date: Option<String>,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub build_type: Option<BuildType>,
    /// Present while a build runs; `currentStageText` is the one line that
    /// says what it is doing right now.
    #[serde(rename = "running-info")]
    pub running_info: Option<RunningInfo>,
    /// Who or what started the build. The only place TeamCity names the
    /// person, and so the only source of `SyncItem::author` for a build.
    pub triggered: Option<Triggered>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunningInfo {
    pub current_stage_text: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Triggered {
    pub user: Option<User>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User {
    pub username: Option<String>,
}

/// TeamCity stamps `yyyyMMdd'T'HHmmssZ` (`20260822T101018+0000`), not RFC
/// 3339 -- feeding those to `DateTime::parse_from_rfc3339` yields `None` for
/// every timestamp the server ever sends, and `updated_at` would silently be
/// null on every item.
pub(crate) fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z")
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Which builds a query asks for -- and, via [`StateFilter::matches`], which
/// records a run counts as finished.
///
/// There is no single-`Queued` or single-`Running` variant on purpose: this
/// adapter never wants one without the other, and the pair has to travel as
/// **one** locator. `state` is a single locator dimension that takes a nested
/// boolean form for a combination; repeating the dimension
/// (`state:running,state:queued`) is not a locator TeamCity accepts, and
/// knobas-mockd records it as a violation with a message naming the right
/// form. Making the wrong form unrepresentable is cheaper than catching it in
/// review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateFilter {
    Finished,
    /// Queued **and** running, in one query.
    InFlight,
}

impl StateFilter {
    fn as_str(self) -> &'static str {
        match self {
            StateFilter::Finished => "finished",
            StateFilter::InFlight => "(queued:true,running:true)",
        }
    }

    /// Does a build's `state` field fall inside this filter?
    ///
    /// The same question the server answered when it selected the page, asked
    /// again locally: the in-flight poll is global, so its results are
    /// re-classified here alongside the scope filter.
    pub(crate) fn matches(self, state: Option<&str>) -> bool {
        match self {
            StateFilter::Finished => state == Some("finished"),
            StateFilter::InFlight => matches!(state, Some("queued" | "running")),
        }
    }
}

/// A `/app/rest/builds` locator, restricted to the dimensions the contract
/// defines: `buildType:`, `state:`, `sinceBuild:`, `defaultFilter:`,
/// `count:`. Anything else -- `project:`, `affectedProject:` -- is recorded as
/// a violation by the mock and must not be sent. Each dimension appears **at
/// most once**; see [`StateFilter`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Locator {
    pub build_type_id: Option<String>,
    pub state: Option<StateFilter>,
    pub since_build_id: Option<i64>,
    /// TeamCity's default filter hides everything that is not a finished,
    /// non-personal, non-canceled build. `Some(false)` turns it off, which is
    /// the only way to ask a question about **every** build regardless of
    /// state; `None` sends the dimension not at all and takes the default.
    ///
    /// Distinct from `state`: `state:` names the states wanted and is the
    /// right dimension when the answer is a set of builds to emit.
    /// `defaultFilter:false` widens the population a *stateless* question is
    /// asked over, which is what the run's opening ceiling query needs -- see
    /// [`sync::ceiling`](crate::sync).
    pub default_filter: Option<bool>,
    pub count: u32,
}

impl Locator {
    /// Deliberately not `to_string`: an inherent `to_string` shadows `Display`
    /// and clippy rejects it, and this string is a wire format rather than a
    /// human rendering.
    pub(crate) fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(id) = &self.build_type_id {
            parts.push(format!("buildType:(id:{id})"));
        }
        if let Some(state) = self.state {
            parts.push(format!("state:{}", state.as_str()));
        }
        if let Some(id) = self.since_build_id {
            parts.push(format!("sinceBuild:(id:{id})"));
        }
        if let Some(on) = self.default_filter {
            parts.push(format!("defaultFilter:{on}"));
        }
        parts.push(format!("count:{}", self.count));
        parts.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `/app/rest/buildTypes` answer, shaped the way a real server sends it
    /// -- richer than [`BUILD_TYPE_FIELDS`] asks for, because the parse must
    /// survive a server that answers with more than it was asked.
    fn build_types_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "href": "/app/rest/buildTypes",
            "buildType": [
                { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                  "projectId": "Payout", "projectName": "Payout",
                  "description": "Runs the SEPA suite",
                  "webUrl": "https://ci.example.com/buildConfiguration/Payout_IntegrationTests",
                  "paused": false },
                { "id": "Ledger_Deploy_Staging", "name": "Deploy Staging",
                  "projectId": "Ledger", "projectName": "Ledger" }
            ]
        })
    }

    /// A finished build and a running one.
    fn builds_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "build": [
                { "id": 1188, "number": "1188", "buildTypeId": "Payout_Build",
                  "state": "running", "status": "SUCCESS", "statusText": "Running",
                  "branchName": "feature/PAY-231-sepa-retry", "percentageComplete": 60,
                  "webUrl": "https://ci.example.com/build/1188",
                  "queuedDate": "20260822T114000+0000", "startDate": "20260822T114500+0000",
                  "running-info": { "percentageComplete": 60, "currentStageText": "step 3/5 `cargo test`" },
                  "buildType": { "id": "Payout_Build", "name": "Build",
                                 "projectId": "Payout", "projectName": "Payout" },
                  "triggered": { "type": "user", "date": "20260822T114000+0000",
                                 "user": { "username": "mara.lindqvist", "name": "Mara Lindqvist" } } },
                { "id": 1187, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
                  "state": "finished", "status": "FAILURE",
                  "statusText": "Tests failed: 1 (1 new), passed: 41",
                  "branchName": "feature/PAY-231-sepa-retry",
                  "webUrl": "https://ci.example.com/build/1187",
                  "queuedDate": "20260822T100500+0000", "startDate": "20260822T100600+0000",
                  "finishDate": "20260822T101018+0000",
                  "buildType": { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                                 "projectId": "Payout", "projectName": "Payout" } }
            ]
        })
    }

    #[test]
    fn a_build_types_listing_parses() {
        let env: ListEnvelope = serde_json::from_value(build_types_json()).expect("envelope");
        assert_eq!(env.items.len(), 2);
        let bt: BuildType = serde_json::from_value(env.items[0].clone()).expect("buildType");
        assert_eq!(bt.id, "Payout_IntegrationTests");
        assert_eq!(bt.name.as_deref(), Some("Integration Tests"));
        assert_eq!(bt.project_id.as_deref(), Some("Payout"));
        assert_eq!(bt.description.as_deref(), Some("Runs the SEPA suite"));
        assert_eq!(
            bt.web_url.as_deref(),
            Some("https://ci.example.com/buildConfiguration/Payout_IntegrationTests")
        );
        // Everything but the id is optional: a trimmed or older server omits
        // fields rather than sending nulls, and a missing description must not
        // fail the whole run.
        let bare: BuildType =
            serde_json::from_value(env.items[1].clone()).expect("sparse buildType");
        assert_eq!(bare.description, None);
        assert_eq!(bare.web_url, None);
    }

    #[test]
    fn a_builds_listing_parses_running_and_finished() {
        let env: ListEnvelope = serde_json::from_value(builds_json()).expect("envelope");
        let running: Build = serde_json::from_value(env.items[0].clone()).expect("running");
        assert_eq!(running.id, 1188);
        assert_eq!(running.state.as_deref(), Some("running"));
        assert_eq!(running.finish_date, None);
        // The hyphen makes this the one field name serde cannot derive.
        assert_eq!(
            running
                .running_info
                .as_ref()
                .and_then(|r| r.current_stage_text.as_deref()),
            Some("step 3/5 `cargo test`")
        );
        assert_eq!(
            running
                .triggered
                .and_then(|t| t.user)
                .and_then(|u| u.username)
                .as_deref(),
            Some("mara.lindqvist")
        );
        let finished: Build = serde_json::from_value(env.items[1].clone()).expect("finished");
        assert_eq!(finished.id, 1187);
        assert_eq!(finished.status.as_deref(), Some("FAILURE"));
        assert!(
            finished.running_info.is_none(),
            "a finished build has no running-info"
        );
        assert_eq!(
            finished.build_type.expect("nested").project_name.as_deref(),
            Some("Payout")
        );
    }

    /// A field this adapter does not know about must not fail the parse: the
    /// server is free to send more than `fields=` asked for, and a future
    /// TeamCity will.
    #[test]
    fn unknown_fields_are_ignored() {
        let b: Build = serde_json::from_value(serde_json::json!({
            "id": 7, "state": "finished", "somethingNew": { "x": 1 }
        }))
        .expect("forward compatible");
        assert_eq!(b.id, 7);
    }

    /// TeamCity does not speak RFC 3339. This is the format it does speak.
    #[test]
    fn teamcity_timestamps_parse() {
        let t = parse_ts("20260822T101018+0000").expect("teamcity format");
        assert_eq!(t.to_rfc3339(), "2026-08-22T10:10:18+00:00");
        let t = parse_ts("20260822T121018+0200").expect("non-utc offset");
        assert_eq!(
            t.to_rfc3339(),
            "2026-08-22T10:10:18+00:00",
            "the offset is applied, not ignored"
        );
        // Tolerated, because a proxy or a future version may normalise it.
        assert!(parse_ts("2026-08-22T10:10:18Z").is_some());
        assert!(parse_ts("").is_none());
        assert!(parse_ts("yesterday").is_none());
        // A date with no zone at all: TeamCity always sends one, and guessing
        // UTC for a server on +02:00 would be two hours of silent skew.
        assert!(parse_ts("20260822T101018").is_none());
    }

    /// The locator is a wire string; its shape is the contract with mockd's
    /// locator grammar, so it is asserted literally.
    #[test]
    fn locators_render_in_a_fixed_order() {
        assert_eq!(
            Locator {
                build_type_id: Some("Payout_Build".to_owned()),
                state: Some(StateFilter::Finished),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "buildType:(id:Payout_Build),state:finished,count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                since_build_id: Some(412),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,sinceBuild:(id:412),count:100"
        );
        // Queued and running in ONE query. `state` is a single locator
        // dimension with a nested boolean form for combinations; repeating it
        // (`state:running,state:queued`) is not a locator TeamCity accepts,
        // and knobas-mockd records it as a violation.
        assert_eq!(
            Locator {
                state: Some(StateFilter::InFlight),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:(queued:true,running:true),count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::InFlight),
                count: 50,
                ..Locator::default()
            }
            .render(),
            "state:(queued:true,running:true),count:50"
        );
        // The spelling the interfaces doc printed before stream T corrected it
        // -- pinned so it cannot creep back in.
        for locator in [
            Locator {
                state: Some(StateFilter::InFlight),
                count: 100,
                ..Locator::default()
            },
            Locator {
                state: Some(StateFilter::Finished),
                count: 100,
                ..Locator::default()
            },
        ] {
            let rendered = locator.render();
            assert!(!rendered.contains("state:running"), "{rendered}");
            assert!(!rendered.contains("state:queued"), "{rendered}");
        }
        // The run's opening ceiling query: no `state`, the default filter
        // explicitly off, one build. `state` would answer a different
        // question -- see the field's doc.
        assert_eq!(
            Locator {
                default_filter: Some(false),
                count: 1,
                ..Locator::default()
            }
            .render(),
            "defaultFilter:false,count:1"
        );
        // No dimension may repeat: the mock rejects a locator that names one
        // twice, whichever one it is.
        let rendered = Locator {
            build_type_id: Some("Payout_Build".to_owned()),
            state: Some(StateFilter::InFlight),
            since_build_id: Some(9),
            default_filter: Some(false),
            count: 100,
        }
        .render();
        for dimension in [
            "buildType:",
            "state:",
            "sinceBuild:",
            "defaultFilter:",
            "count:",
        ] {
            assert_eq!(
                rendered.matches(dimension).count(),
                1,
                "{dimension} appears more than once in {rendered}"
            );
        }
    }

    /// The filter has to answer the same question about a record that the
    /// server answered about the query -- `sync` classifies finished from
    /// in-flight builds with it.
    #[test]
    fn a_state_filter_matches_the_records_the_server_would_return() {
        assert!(StateFilter::Finished.matches(Some("finished")));
        assert!(!StateFilter::Finished.matches(Some("running")));
        assert!(!StateFilter::Finished.matches(Some("queued")));
        assert!(StateFilter::InFlight.matches(Some("running")));
        assert!(StateFilter::InFlight.matches(Some("queued")));
        assert!(!StateFilter::InFlight.matches(Some("finished")));
        // A record with no `state` at all belongs to neither: guessing would
        // move a watermark on a build nobody can classify.
        assert!(!StateFilter::Finished.matches(None));
        assert!(!StateFilter::InFlight.matches(None));
        // Nor does an unknown one.
        assert!(!StateFilter::Finished.matches(Some("deleted")));
        assert!(!StateFilter::InFlight.matches(Some("deleted")));
    }

    /// `fields=` is mandatory on every request, and what it asks for is what
    /// the mapping reads.
    ///
    /// This is a *representation* check and it cannot see the thing that
    /// actually matters -- whether the server recognises the names. That is
    /// `tests/mockd.rs`'s job: mockd answers an unknown name with 400 and a
    /// recorded violation, so a selector that drifts from the contract fails
    /// there, loudly, rather than here.
    #[test]
    fn the_field_selectors_cover_everything_the_mapping_reads() {
        for needed in [
            "id",
            "number",
            "state",
            "status",
            "statusText",
            "branchName",
            "webUrl",
            "finishDate",
            "startDate",
            "queuedDate",
            "buildType(",
            "running-info(",
            "triggered(",
        ] {
            assert!(
                BUILD_FIELDS.contains(needed),
                "BUILD_FIELDS misses {needed}"
            );
        }
        for needed in [
            "id",
            "name",
            "projectId",
            "projectName",
            "description",
            "webUrl",
        ] {
            assert!(
                BUILD_TYPE_FIELDS.contains(needed),
                "BUILD_TYPE_FIELDS misses {needed}, so `map::build_config_item` reads it as None \
                 on every configuration"
            );
        }
        assert!(SERVER_FIELDS.contains("version"));
        assert!(USER_FIELDS.contains("username"));
        // Every collection selector must ask for `count` too, or the envelope
        // the parse reads back is not the one the server sent.
        for f in [BUILD_FIELDS, BUILD_TYPE_FIELDS] {
            assert!(f.starts_with("count,"), "{f:?} must select the envelope");
        }
        // No whitespace: these go into a query string verbatim.
        for f in [BUILD_FIELDS, BUILD_TYPE_FIELDS, SERVER_FIELDS, USER_FIELDS] {
            assert!(!f.contains(' '), "{f:?} must not contain spaces");
            assert_eq!(
                f.matches('(').count(),
                f.matches(')').count(),
                "{f:?} is unbalanced"
            );
        }
    }

    /// The other direction: a selector must not ask for a name **no** reader
    /// in `map` looks at.
    ///
    /// Every name below is one `knobas-mockd` serves -- asking for `href` or
    /// `paused` is a 200, not the 400 an unknown name gets (deviation 6) --
    /// so nothing here is about the mock contract, and a failure here is never
    /// a violation. It is response size and a `payload` key with no reader.
    ///
    /// **Which is why the fix is never "delete the name from the selector"
    /// when a reader is what is missing.** `description` used to fail this
    /// test, and it was the *test* that was wrong: `map::build_config_item`
    /// has always put `bt.description` in the search blob, so the selector
    /// omitting it meant a configuration's prose silently never reached the
    /// index. It is asked for now. If one of these names acquires a reader,
    /// add it here in the same change.
    #[test]
    fn the_selectors_ask_for_nothing_no_reader_looks_at() {
        for unread in ["paused", "href"] {
            for (name, selector) in [
                ("BUILD_FIELDS", BUILD_FIELDS),
                ("BUILD_TYPE_FIELDS", BUILD_TYPE_FIELDS),
            ] {
                assert!(
                    !selector.contains(unread),
                    "{name} asks for {unread}. mockd serves it, so this is not a violation -- it \
                     is response size and a payload key nothing in `map` reads. If you added a \
                     reader for it, add the name here too; if you did not, drop it."
                );
            }
        }
        // `triggered` is asked for at exactly the depth `map::build_item`
        // reads it, and no deeper: mockd would serve
        // `triggered(type,date,user(username,name))` without complaint.
        assert!(
            BUILD_FIELDS.contains("triggered(user(username))"),
            "BUILD_FIELDS must ask for the triggerer, or every build's author is None"
        );
        for deeper in ["triggered(type", "user(username,"] {
            assert!(
                !BUILD_FIELDS.contains(deeper),
                "BUILD_FIELDS asks for {deeper}; nothing in `map` reads below \
                 triggered.user.username"
            );
        }
        assert!(
            !BUILD_TYPE_FIELDS.contains("triggered"),
            "a build configuration has no triggerer"
        );
        // A name the selector asks for and no struct field parses is the same
        // waste one step earlier: it cannot reach a reader at all. Top-level
        // `percentageComplete` was exactly that -- asked for, never parsed
        // (`running-info(percentageComplete)` is the one the mapping reads).
        assert!(
            !BUILD_FIELDS.contains(",percentageComplete"),
            "BUILD_FIELDS asks for a top-level percentageComplete, which `struct Build` does \
             not parse; the mapping reads running-info(percentageComplete)"
        );
        // ...and no preset: mockd honours only `$long`, and asking for the
        // whole subtree defeats the point of a selector.
        for f in [BUILD_FIELDS, BUILD_TYPE_FIELDS, SERVER_FIELDS, USER_FIELDS] {
            assert!(!f.contains('$'), "{f:?} uses a fields= preset");
        }
    }
}
