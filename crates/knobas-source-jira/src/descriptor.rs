//! What knobas knows about this adapter before an instance of it exists.
//!
//! `list_adapters()` (interfaces §2.2) serves this template to the Add-source
//! form, which is *generated* from `config_schema` + `auth_methods` (spec §3a).
//! Nothing downstream carries a hardcoded Jira table, which is why the kind
//! metadata (label, plural, monogram) lives here and not in the launcher.

use knobas_source::{AuthMethod, Capability, KindInfo, SourceDescriptor};

use crate::{WRITE_OP_COMMENT, WRITE_OP_CREATE_TICKET, WRITE_OP_LOG_WORK, WRITE_OP_TRANSITION};

/// The static, instance-free descriptor: `id == adapter_kind` (interfaces §4.2).
/// A configured instance's descriptor is this with `id` and `name` replaced by
/// the instance's own.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: crate::ADAPTER_KIND.to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        name: "Jira".to_owned(),
        // M2 (issue #43): this adapter writes. `Capability::Search` stays
        // reserved for a future server-side `Source::search` -- knobas'
        // launcher answers from the mirror either way. A ticket is read by
        // its key (#2097, `crate::refresh`); a view is not.
        capabilities: vec![Capability::Write, Capability::Refresh],
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        // Bearer PAT (DC >= 8.14) or Basic user+password (spec §3).
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // One credential. The second one of envelope version 2 is Uptime
        // Kuma's alone (issue #452), and a form that offered account
        // fields here would be asking for something nothing reads.
        accepts_account: false,
        min_sync_interval_secs: knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS,
        // The ticket system (#1193, ADR-0029): its tickets declare every
        // path the role needs, and it answers the workflow read.
        roles: vec![knobas_source::role::RoleClaim::new(
            knobas_source::role::TICKET_SYSTEM,
            [
                "reachable_transitions",
                knobas_source::role::TRANSLATE_QUERY,
            ],
        )],
        // M2's ratified Jira set (issue #43, ADR-0006), plus M3.1's `log_work`
        // (issue #280). The UI renders its action bar from exactly this, so an
        // op listed here that `write` refuses is an action that 404s, and one
        // `write` accepts but this omits is an action nothing ever offers. The
        // contract battery holds both directions.
        //
        // `log_work` is declared **here and nowhere else**: Gitea and TeamCity
        // hold no worklogs, so the battery's clause 5 is what proves they
        // refuse it rather than a comment saying they would.
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        code_pages: None,
        write_ops: vec![
            WRITE_OP_COMMENT.to_owned(),
            WRITE_OP_TRANSITION.to_owned(),
            WRITE_OP_CREATE_TICKET.to_owned(),
            WRITE_OP_LOG_WORK.to_owned(),
        ],
        entity_kinds: vec![
            KindInfo {
                id: crate::KIND_TICKET.to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "JI".to_owned(),
                // A `cursor: None` run walks the whole JQL result set -- `project
                // in (…)` or the configured filter, unbounded in time -- so every
                // issue the credential can see is emitted, and the engine's sweep
                // may tombstone whatever did not come back. Contrast TeamCity,
                // which emits the newest N builds per configuration and is
                // therefore `false`. Jira emits one kind, so this adapter is
                // wholly exhaustive (ADR-0003 changed where the claim is written,
                // not what Jira claims).
                full_sync_exhaustive: true,
                refreshes: true,
            },
            KindInfo {
                id: crate::KIND_JIRA_VIEW.to_owned(),
                label: "Jira view".to_owned(),
                plural: "Jira views".to_owned(),
                monogram: "JV".to_owned(),
                // Every run reads every board on the synced projects (#877), and a
                // board that stops coming back is gone in Jira: the run itself
                // says so (`views::gone`), and a full sync's sweep agrees.
                full_sync_exhaustive: true,
                // A view's id is not an issue key: it is read with the
                // boards, and `refresh` answers it not offered (#2146).
                refreshes: false,
            },
        ],
        config_schema: config_schema(),
        payload_paths: payload_paths(),
    }
}

/// Where a Jira Data Center issue keeps what knobas reads (#277, ADR-0007).
///
/// Every path here is one this adapter's own reads already relied on -- the
/// sync has requested `status`, `priority`, `assignee` and `project` in its
/// base field list since M1, and the record reaches the mirror verbatim -- so
/// this writes down what was already true, in the one place a reader now asks.
///
/// **`assignee` declares two paths, in this order.** `fields.assignee.name` is
/// the username on a Data Center instance; `fields.assignee.key` is the same
/// identity on instances that still key on it. Two spellings of *one adapter's
/// own field*, which is what a candidate list is for -- not knobas guessing at
/// a source it has not met. Cloud's `accountId` is deliberately absent: this
/// adapter is Data Center only (`config_schema`'s `flavor`), and an
/// `accountId` is not a username, so a reader matching it against the
/// configured identity would never fire.
///
/// **The issue type, the labels and the sprints** (#811) are what
/// `issuetype:`, `label:` and `sprint:current` read. The first two are in the
/// record as Jira sends it (`BASE_FIELDS` has asked for both since M1). The
/// sprints are a custom field whose id is the instance's own, so `map`
/// stores them beside the record as `sprints: [{id, name, state}]`, on every
/// ticket, empty when the field is not configured or the ticket is in none.
///
/// **The estimate** (#816) is `timeoriginalestimate`, which Jira sends in
/// seconds and `BASE_FIELDS` has asked for since M1: the job card's *est.*.
///
/// **The reporter, the created date, the time spent and the worklog**
/// (#2403) are the ticket page card's *Reporter* and *Logged*, all in
/// `BASE_FIELDS` since M1. The reporter declares two paths for one identity,
/// as the assignee does; `timespent` is seconds, as the estimate is. The
/// worklog is `fields.worklog.worklogs`, which the sync's completion fills
/// with every entry beyond the 20 `/search` inlines, so the card's share of
/// the time is the reader's entries across the whole worklog.
///
/// **The comment count** (#894) is `fields.comment.total`: `BASE_FIELDS` asks
/// for `comment` too, and the sync's completion rewrites `total` with the
/// comments it fetched beyond what `/search` returned, so it is the count the
/// mirror holds. The ticket card's *N comments*.
///
/// **No reviewers and no merged flag**: a Jira issue has neither. A kind that
/// declares no path for a field is a miss, which is what those readers did
/// against Jira before this existed.
///
/// **The blocked-like set is this adapter's claim about Jira workflows**, not
/// about one instance: the three names below are Jira's own conventional
/// spellings of "stuck", matched case-insensitively by their reader. An
/// instance whose workflow spells it otherwise contributes no blocker by
/// status -- absence, never a wrong one -- and a per-instance list is a
/// configuration key the day one is asked for, not a guess made here.
///
/// **The done-like set is the same kind of claim** (#1501): Done, Closed and
/// Resolved are the three names Jira's shipped workflows end in, matched the
/// same way. While a ticket's status is one of them, the job context anchored
/// on it is archived. A workflow that ends in another word never archives,
/// which is how knobas behaved before the set existed. Not the status
/// category: a second signal for later, ruled in the grilling of 2026-10-21.
///
/// **A ticket's parent is its epic or parent issue** (ADR-0018, #598), read
/// off the `fields.parent` object Jira nests in an issue record: a sub-task's
/// parent issue, and the epic on a next-gen or recent company-managed project.
/// It answers to its `key` -- `PAY-200` -- which is also on every issue
/// record's root, so the names declared for it resolve on a ticket's own
/// record too, and `under:PAY-200` returns the epic itself when it is
/// mirrored. An epic outside the synced JQL is still the parent its children
/// name. Not the summary: two epics may share one, and nobody types it.
///
/// **Two candidates, first that resolves wins.** A classic Data Center project
/// serves no `fields.parent` for epic children: it keeps the epic in the Epic
/// Link custom field, whose id is the instance's own, so `map` stores it as
/// `epic: {"key": …}` when `epic_link_field` is configured, and that is the
/// second candidate (the deputy's ruling of 2026-10-12 on #598). A sub-task's
/// direct parent comes first. On any one instance one of the two is unused,
/// which is what clause 6's shared evidence slot tolerates (#301).
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_source::{KindPaths, ListPath, ParentPath, PayloadPath};
    vec![
        KindPaths {
            kind: crate::KIND_TICKET.to_owned(),
            status_name: vec![PayloadPath::of(["fields", "status", "name"])],
            priority: vec![PayloadPath::of(["fields", "priority", "name"])],
            assignee: vec![
                PayloadPath::of(["fields", "assignee", "name"]),
                PayloadPath::of(["fields", "assignee", "key"]),
            ],
            project_key: vec![PayloadPath::of(["fields", "project", "key"])],
            project_name: vec![PayloadPath::of(["fields", "project", "name"])],
            issue_type: vec![PayloadPath::of(["fields", "issuetype", "name"])],
            estimate: vec![PayloadPath::of(["fields", "timeoriginalestimate"])],
            comment_count: vec![PayloadPath::of(["fields", "comment", "total"])],
            reporter: vec![
                PayloadPath::of(["fields", "reporter", "name"]),
                PayloadPath::of(["fields", "reporter", "key"]),
            ],
            created: vec![PayloadPath::of(["fields", "created"])],
            time_spent: vec![PayloadPath::of(["fields", "timespent"])],
            worklog: vec![knobas_source::WorklogPath {
                at: PayloadPath::of(["fields", "worklog", "worklogs"]),
                author: vec![
                    PayloadPath::of(["author", "name"]),
                    PayloadPath::of(["author", "key"]),
                ],
                seconds: vec![PayloadPath::of(["timeSpentSeconds"])],
            }],
            // The description and the comments as Jira Data Center stores
            // them (#2308): wiki markup, which the shell converts to draw.
            description: vec![PayloadPath::of(["fields", "description"])],
            markup: knobas_source::Markup::JiraWiki,
            labels: vec![ListPath {
                at: PayloadPath::of(["fields", "labels"]),
                entry: PayloadPath::default(),
            }],
            sprint_state: vec![ListPath {
                at: PayloadPath::of(["sprints"]),
                entry: PayloadPath::of(["state"]),
            }],
            blocked_statuses: vec![
                "Blocked".to_owned(),
                "On Hold".to_owned(),
                "Impediment".to_owned(),
            ],
            done_statuses: vec![
                "Done".to_owned(),
                "Closed".to_owned(),
                "Resolved".to_owned(),
            ],
            parent: vec![
                ParentPath {
                    kind: crate::KIND_TICKET.to_owned(),
                    at: PayloadPath::of(["fields", "parent"]),
                    names: vec![PayloadPath::of(["key"])],
                },
                ParentPath {
                    kind: crate::KIND_TICKET.to_owned(),
                    at: PayloadPath::of(["epic"]),
                    names: vec![PayloadPath::of(["key"])],
                },
            ],
            ..KindPaths::default()
        },
        // The ticket system's saved views (#1207, ADR-0029): each Jira board and
        // filter is one, and the engine makes it a ticket view by this frame,
        // not by the kind's id.
        KindPaths {
            kind: crate::KIND_JIRA_VIEW.to_owned(),
            frame: Some(knobas_core::payload::Frame::SavedView),
            // Jira's, not the source's name: a source named *Jira EU* still
            // lists its views under *Jira boards* and *Jira filters*.
            system_name: Some("Jira".to_owned()),
            // Where `views` writes them (#2050): the saved filter's JQL (a
            // filter's own), the board's type, a Kanban board's sub-query and
            // a Scrum board's active sprint. The projection reads these and
            // no key of its own.
            source_query: vec![PayloadPath::of(["jql"])],
            board_type: vec![PayloadPath::of(["type"])],
            sub_query: vec![PayloadPath::of(["sub_query"])],
            sprint: vec![PayloadPath::of(["sprint"])],
            ..KindPaths::default()
        },
    ]
}

/// The JSON Schema the Add-source form is generated from.
///
/// Every property carries a `default` the form can submit unchanged, and the
/// key set is pinned against [`crate::JiraConfig`]'s own serialization by this
/// module's tests -- a schema key with no struct field is a form input that is
/// silently discarded.
fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "flavor": {
                "type": "string", "enum": ["datacenter", "cloud"], "default": "datacenter",
                "title": "Deployment",
                "description": "Data Center / Server speaks REST v2. Cloud is not supported yet."
            },
            "projects": {
                "type": "array", "default": [], "title": "Projects",
                "description": "Project keys to sync, e.g. PAY. Leave empty to sync everything this account can see.",
                "items": { "type": "string", "pattern": "^[A-Z][A-Z0-9_]{0,31}$" }
            },
            "jql_filter": {
                "type": ["string", "null"], "default": null, "title": "JQL filter",
                "description": "Alternative to Projects: any JQL, without an ORDER BY."
            },
            "username": {
                "type": ["string", "null"], "default": null, "title": "Username",
                "description": "Your Jira account. Filled in by Test connection; used for @me and My items. Also the login name for user + password authentication."
            },
            "epic_link_field": {
                "type": ["string", "null"], "default": null, "title": "Epic Link field id",
                "description": "Where a classic project keeps epic membership. Filled in by Test connection, and by the next sync when left empty: the id is different on every Jira, so one copied from another instance reads the wrong field rather than failing. Leave it empty for a next-gen project, whose epic membership is synced already via fields.parent."
            },
            "sprint_field": {
                "type": ["string", "null"], "default": null, "title": "Sprint field id",
                "description": "Where Jira Software keeps the sprints a ticket is in. Filled in by Test connection, and by the next sync when left empty: the id is different on every Jira. An id you type is kept. Leave it empty and no ticket is in a sprint as far as knobas knows."
            },
            "boards": {
                "type": "string", "enum": ["synced", "favourites", "all", "none"],
                "default": "synced", "title": "Boards",
                "description": "Which Jira boards come in as views in Tickets: the boards on the projects this source syncs, only the boards built on your favourite filters, every board you can see, or none. Read at every sync.",
                "x-knobas-section": "boards-and-filters"
            },
            "favourite_filters": {
                "type": "boolean", "default": true, "title": "My favourite filters",
                "description": "Whether your favourite Jira filters come in as views in Tickets. Read at every sync.",
                "x-knobas-section": "boards-and-filters"
            },
            "page_size": {
                "type": "integer", "minimum": 1, "maximum": 1000, "default": 100,
                "title": "Issues per request"
            },
            "rate_per_sec": {
                "type": "integer", "minimum": 1, "default": 20, "title": "Requests per second"
            },
            "rate_burst": {
                "type": "integer", "minimum": 1, "default": 40, "title": "Request burst"
            },
            "connect_timeout_secs": {
                "type": "integer", "minimum": 1, "default": 10, "title": "Connect timeout (s)"
            },
            "request_timeout_secs": {
                "type": "integer", "minimum": 1, "default": 30, "title": "Request timeout (s)"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sources view renders its action bar from `write_ops`, so what is
    /// here is exactly what the user is offered: M2's ratified Jira set plus
    /// M3.1's `log_work`, and nothing else (issues #43 and #280, ADR-0006).
    /// `Capability::Search` stays absent -- it is reserved for a server-side
    /// `Source::search` the SPI does not have.
    #[test]
    fn the_template_declares_the_ratified_write_set_and_nothing_else() {
        let d = descriptor_template();
        assert_eq!(d.id, crate::ADAPTER_KIND);
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(
            d.capabilities,
            vec![Capability::Write, Capability::Refresh],
            "{:?}",
            d.capabilities
        );
        assert_eq!(
            d.write_ops,
            vec!["comment", "transition", "create_ticket", "log_work"],
            "the action bar is rendered from this list alone"
        );
        assert_eq!(d.adapter_version, crate::ADAPTER_VERSION);
        assert_eq!(
            d.auth_methods,
            vec![
                knobas_source::AuthMethod::Pat,
                knobas_source::AuthMethod::UserPassword
            ]
        );
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, vec![crate::KIND_TICKET, crate::KIND_JIRA_VIEW]);
        // #1207: the views are the ticket system's saved views, by frame.
        let saved: Vec<&str> = d
            .payload_paths
            .iter()
            .filter(|p| p.frame == Some(knobas_core::payload::Frame::SavedView))
            .map(|p| p.kind.as_str())
            .collect();
        assert_eq!(saved, vec![crate::KIND_JIRA_VIEW]);
        assert_eq!(
            d.payload_paths[1].system_name.as_deref(),
            Some("Jira"),
            "the views' groups say Jira, not the source's own name"
        );
        // #2050: the saved-view frame reads a view's query, board type,
        // sub-query and sprint where `views` writes them, and nowhere else.
        let at = |key: &str| vec![knobas_source::PayloadPath::of([key])];
        let view = &d.payload_paths[1];
        assert_eq!(
            (
                &view.source_query,
                &view.board_type,
                &view.sub_query,
                &view.sprint
            ),
            (&at("jql"), &at("type"), &at("sub_query"), &at("sprint"))
        );
        assert_eq!(d.entity_kinds[0].monogram, "JI");
        assert_eq!(d.entity_kinds[0].plural, "Tickets");
    }

    /// A `cursor: None` Jira sync walks the whole JQL result set, so the
    /// engine's post-full-sync sweep is safe: anything whose `synced_at`
    /// predates the run really did stop coming back. `false` here would leave
    /// hard-deleted issues in the mirror forever; `true` on a windowed adapter
    /// (TeamCity) would tombstone its history every run, which is why this is
    /// a per-adapter claim and not a default.
    #[test]
    fn a_full_sync_is_the_whole_corpus() {
        let d = descriptor_template();
        assert!(
            d.entity_kinds.iter().all(|k| k.full_sync_exhaustive),
            "every kind Jira emits is walked in full"
        );
    }

    /// The claim above is one the contract battery *cannot* check -- it would
    /// have to know the remote corpus. What makes it true is a single property
    /// of the query renderer: a `cursor: None` run puts **no time bound** in
    /// its JQL, so the result set is every issue the credential can see.
    ///
    /// Pinned here, beside the claim, rather than in `jql`'s own tests: it is
    /// the descriptor that becomes a lie if the renderer ever gains a bound,
    /// and the engine acts on the descriptor -- it sweeps every row whose
    /// `synced_at` predates a full run, so a windowed full sync would tombstone
    /// live issues. The paging half (that the run walks `total`, not just the
    /// first page) is task 5's and is checked there.
    #[test]
    fn the_full_sync_claim_rests_on_a_query_with_no_time_bound() {
        assert!(
            descriptor_template()
                .entity_kinds
                .iter()
                .all(|k| k.full_sync_exhaustive)
        );
        for config in [
            serde_json::json!({}),
            serde_json::json!({ "projects": ["PAY", "OPS"] }),
            serde_json::json!({ "jql_filter": "labels = sepa" }),
        ] {
            let cfg = crate::JiraConfig::from_json(&config).unwrap();
            let jql = crate::jql::build_jql(&cfg, None, 7_200).to_lowercase();
            for bound in [
                "updated >=",
                "updated>=",
                "created >=",
                "updated <",
                "created <",
            ] {
                assert!(
                    !jql.contains(bound),
                    "a full sync claiming to be exhaustive must not bound time, \
                     but {config} renders {jql:?}"
                );
            }
        }
    }

    /// The Add-source form is generated from `config_schema` (spec §3a), so the
    /// schema and the struct the adapter parses must describe the same keys.
    /// Drift here is a form field that is silently discarded, or a config key
    /// with no way to set it.
    #[test]
    fn the_schema_and_the_config_struct_agree() {
        let d = descriptor_template();
        let schema_keys: std::collections::BTreeSet<String> = d.config_schema["properties"]
            .as_object()
            .expect("config_schema.properties is an object")
            .keys()
            .cloned()
            .collect();
        let struct_keys: std::collections::BTreeSet<String> =
            serde_json::to_value(crate::JiraConfig::default())
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
        assert_eq!(schema_keys, struct_keys);
    }

    /// Every default the form would submit has to parse back.
    #[test]
    fn the_schema_defaults_are_a_valid_configuration() {
        let d = descriptor_template();
        let mut object = serde_json::Map::new();
        for (key, prop) in d.config_schema["properties"].as_object().unwrap() {
            object.insert(
                key.clone(),
                prop.get("default")
                    .unwrap_or_else(|| panic!("{key} has no default for the form to submit"))
                    .clone(),
            );
        }
        crate::JiraConfig::from_json(&serde_json::Value::Object(object)).unwrap();
    }

    /// The form offers interfaces §4.1's Jira rate (#2167): 20 requests a
    /// second with a burst of 40, so a page's worklog completions are not
    /// spaced 200 ms apart. Both stay settings a source can lower.
    #[test]
    fn the_form_offers_twenty_a_second_with_a_burst_of_forty() {
        let props = &descriptor_template().config_schema["properties"];
        assert_eq!(props["rate_per_sec"]["default"], 20);
        assert_eq!(props["rate_burst"]["default"], 40);
    }

    /// A ticket sits in its epic or parent issue (ADR-0018, #598), read off
    /// the `fields.parent` object Jira nests in an issue record, or else off the
    /// `epic` object `map` stores from a classic project's Epic Link field (the
    /// deputy's ruling of 2026-10-12 on #598), and that parent
    /// answers to its **key** -- the name `under:PAY-100` types and a path line
    /// shows. The golden epic is the parent's own record shape, so its root
    /// carries the same key: that is how `under:` returns the epic itself.
    #[test]
    fn a_ticket_declares_fields_parent_as_its_parent_answered_to_by_key() {
        use knobas_core::payload::{resolve_names_as_parent, resolve_parents};
        use knobas_source::{ParentPath, PayloadPath};
        let declarations = descriptor_template().payload_paths;
        let declared = &declarations
            .iter()
            .find(|paths| paths.kind == crate::KIND_TICKET)
            .expect("a ticket declares its paths")
            .parent;
        assert_eq!(
            declared,
            &vec![
                ParentPath {
                    kind: "ticket".to_owned(),
                    at: PayloadPath::of(["fields", "parent"]),
                    names: vec![PayloadPath::of(["key"])],
                },
                ParentPath {
                    kind: "ticket".to_owned(),
                    at: PayloadPath::of(["epic"]),
                    names: vec![PayloadPath::of(["key"])],
                },
            ]
        );

        // Against the golden records: a child naming PAY-200 resolves to one
        // ticket parent called PAY-200, and the golden epic (no parent of its
        // own) answers to PAY-200 as a parent.
        let child = serde_json::json!({
            "key": "PAY-231",
            "fields": { "parent": { "id": "10200", "key": "PAY-200",
                                    "fields": { "summary": "SEPA payout reliability" } } }
        });
        let parents = resolve_parents(&child, declared);
        assert_eq!(parents.len(), 1, "{parents:?}");
        assert_eq!(parents[0].kind, "ticket");
        assert_eq!(parents[0].names, vec!["PAY-200".to_owned()]);

        // A classic project's epic, stored by `map` as `epic` (#598), is the
        // second candidate; a sub-task's direct parent, the first, wins.
        let classic = serde_json::json!({
            "key": "PAY-219", "epic": { "key": "PAY-200" },
            "fields": { "customfield_10108": "PAY-200" }
        });
        let parents = resolve_parents(&classic, declared);
        assert_eq!(parents.len(), 1, "{parents:?}");
        assert_eq!(parents[0].names, vec!["PAY-200".to_owned()]);
        let sub_task = serde_json::json!({
            "key": "PAY-250", "epic": { "key": "PAY-200" },
            "fields": { "parent": { "key": "PAY-219" } }
        });
        assert_eq!(
            resolve_parents(&sub_task, declared)[0].names,
            vec!["PAY-219".to_owned()]
        );

        let epic: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/issue-epic.json")).unwrap();
        assert!(resolve_parents(&epic, declared).is_empty());
        assert_eq!(
            resolve_names_as_parent(&epic, "ticket", &declarations),
            vec!["PAY-200".to_owned()]
        );
    }

    /// The descriptor crosses the IPC bridge as plain data (spec §3a).
    #[test]
    fn the_template_survives_a_json_round_trip() {
        let json = serde_json::to_string(&descriptor_template()).unwrap();
        let back: knobas_source::SourceDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_kind, crate::ADAPTER_KIND);
    }

    /// A ticket's done-like set is the three names Jira's shipped workflows
    /// end in (#1501), declared beside the blocked-like set and kept apart
    /// from it: a status cannot be both stuck and finished.
    #[test]
    fn a_ticket_declares_done_closed_and_resolved_as_done_like() {
        let template = descriptor_template();
        let ticket = template
            .payload_paths
            .iter()
            .find(|paths| paths.kind == crate::KIND_TICKET)
            .expect("the ticket kind declares its paths");
        assert_eq!(ticket.done_statuses, ["Done", "Closed", "Resolved"]);
        for done in &ticket.done_statuses {
            assert!(
                !ticket
                    .blocked_statuses
                    .iter()
                    .any(|blocked| blocked.eq_ignore_ascii_case(done)),
                "{done} is in both sets"
            );
        }
    }

    /// **No example field id in the help text** (#297).
    ///
    /// The Epic Link id is minted per instance -- three seeds of one script
    /// gave `customfield_10101`, `customfield_10109` and `customfield_10101`,
    /// and on one of them `customfield_10102` was *Epic Status*. So an example
    /// is not a harmless illustration: a reader who types it gets a source
    /// that syncs a field which is not the epic relation, silently, with no
    /// error anywhere. The help text says where the value comes from instead,
    /// and *Test connection* puts it there.
    ///
    /// Pinned as "no `customfield_` anywhere in this property", not as the
    /// exact sentence: what must not come back is the *shape* of the mistake.
    #[test]
    fn the_epic_link_help_text_offers_no_example_id() {
        let prop = &descriptor_template().config_schema["properties"]["epic_link_field"];
        let text = serde_json::to_string(prop).unwrap();
        assert!(
            !text.contains("customfield"),
            "an example id is a value somebody will type: {text}"
        );
        assert!(
            text.contains("Test connection"),
            "the help text has to say where the value does come from: {text}"
        );
    }

    /// A schema property that named a password would put a secret in the DB.
    ///
    /// Property *names* and *titles*, not the whole serialized schema: the
    /// `username` field's help text legitimately says "user + password", and a
    /// substring scan over descriptions would forbid explaining the auth
    /// method the schema exists to support.
    #[test]
    fn the_schema_asks_for_no_secret() {
        let d = descriptor_template();
        for (key, prop) in d.config_schema["properties"].as_object().unwrap() {
            let title = prop["title"].as_str().unwrap_or_default().to_lowercase();
            for forbidden in ["password", "token", "secret", "pat"] {
                assert!(!key.to_lowercase().contains(forbidden), "property {key}");
                assert!(
                    !title.contains(forbidden),
                    "property {key} is titled {title:?}"
                );
            }
        }
    }

    /// The role this adapter fills (#1193, ADR-0029), held to its minimum
    /// on the descriptor alone: the install-time check, which the battery
    /// repeats as its clause 10.
    #[test]
    fn the_template_claims_the_ticket_system_role_and_fills_it() {
        let d = descriptor_template();
        let roles: Vec<&str> = d.roles.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(roles, [knobas_source::role::TICKET_SYSTEM]);
        assert_eq!(knobas_source::role::check(&d), Ok(()));
    }
}
