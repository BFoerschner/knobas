//! One rule, two implementations, against a real PostgreSQL (#277).
//!
//! A declared payload path is resolved twice in this codebase: in Rust, by
//! `knobas_core::payload`'s resolvers, which is what the contract battery
//! certifies an adapter's declarations with; and in SQL, by
//! `declared_string!`, `declared_flag!` and `declared_list!`, which is what
//! every reader actually runs -- the project census is a pass over the whole
//! live corpus and the mini board narrows inside its own statement, so pulling
//! payloads into Rust to resolve them is not on offer.
//!
//! Two implementations of one rule drift. This is the pin: every case below
//! goes through **both**, and the assertion is that they agree *and* that they
//! agree on the stated answer. A case whose expected value were read out of
//! one of the two would be a tautology; each is written out.
//!
//! The cases are the failure directions ADR-0007 requires pinned, at the level
//! the resolution itself decides them: a candidate that lands on an object, a
//! blank string, a key an object does not have, a source that declares
//! nothing, a kind that declares nothing, a field that declares nothing.

use knobas_core::payload::{
    Declarations, KindPaths, ListPath, ParentPath, PayloadPath, StateChip, StatePath, StateValue,
    parent_path, resolve_flag, resolve_list, resolve_names_as_parent, resolve_parents,
    resolve_state, resolve_string,
};
use knobas_core::tone::Tone;
use sqlx::{PgPool, Row};

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("payload_paths")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// One live mirror item, carrying the payload it is given.
async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, payload: &serde_json::Value) {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// The statement a reader takes: one row per mirrored item, its declared
/// status, priority and merged flag resolved the way the mini board and the
/// merge pass resolve them.
const RESOLVED: &str = concat!(
    "select i.entity_id, ",
    knobas_core::declared_string!("$1", "status_name"),
    " as status, ",
    knobas_core::declared_string!("$1", "priority"),
    " as priority, ",
    knobas_core::declared_flag!("$1", "merged"),
    " as merged, (select array_agg(value order by value) from ",
    knobas_core::declared_list!("$1", "reviewers"),
    " lst) as reviewers
       from sync.live_item i
      where i.entity_id = $2"
);

/// What one item's declared fields resolve to, through the database.
struct Resolved {
    status: Option<String>,
    priority: Option<String>,
    merged: Option<bool>,
    reviewers: Vec<String>,
}

async fn in_sql(pool: &PgPool, declarations: &Declarations, entity_id: &str) -> Resolved {
    let row = sqlx::query(RESOLVED)
        .bind(declarations.as_param())
        .bind(entity_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{entity_id} must resolve: {e}"));
    Resolved {
        status: row.get("status"),
        priority: row.get("priority"),
        merged: row.get("merged"),
        reviewers: row
            .get::<Option<Vec<String>>, _>("reviewers")
            .unwrap_or_default(),
    }
}

/// The same four questions asked of the payload in Rust.
fn in_rust(
    declarations: &Declarations,
    source: &str,
    kind: &str,
    payload: &serde_json::Value,
) -> Resolved {
    let empty = KindPaths::default();
    let paths = declarations.get(source, kind).unwrap_or(&empty);
    let mut reviewers = resolve_list(payload, &paths.reviewers);
    reviewers.sort();
    Resolved {
        status: resolve_string(payload, &paths.status_name),
        priority: resolve_string(payload, &paths.priority),
        merged: resolve_flag(payload, &paths.merged),
        reviewers,
    }
}

/// Run one case through both implementations and assert they agree with each
/// other **and** with the stated answer.
async fn both(
    pool: &PgPool,
    declarations: &Declarations,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
    expected: (Option<&str>, Option<&str>, Option<bool>, &[&str]),
) {
    item(pool, source, kind, key, &payload).await;
    let sql = in_sql(pool, declarations, &format!("{source}:{key}")).await;
    let rust = in_rust(declarations, source, kind, &payload);
    let (status, priority, merged, reviewers) = expected;
    let reviewers: Vec<String> = reviewers.iter().map(|r| (*r).to_owned()).collect();

    assert_eq!(sql.status.as_deref(), status, "SQL status for {key}");
    assert_eq!(rust.status.as_deref(), status, "Rust status for {key}");
    assert_eq!(sql.priority.as_deref(), priority, "SQL priority for {key}");
    assert_eq!(
        rust.priority.as_deref(),
        priority,
        "Rust priority for {key}"
    );
    assert_eq!(sql.merged, merged, "SQL merged for {key}");
    assert_eq!(rust.merged, merged, "Rust merged for {key}");
    assert_eq!(sql.reviewers, reviewers, "SQL reviewers for {key}");
    assert_eq!(rust.reviewers, reviewers, "Rust reviewers for {key}");
}

/// Jira Data Center's spellings, and a source that spells a status flat.
fn declarations() -> Declarations {
    Declarations::empty()
        .with(
            "jira",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                status_name: vec![
                    PayloadPath::of(["fields", "status", "name"]),
                    PayloadPath::of(["status"]),
                ],
                priority: vec![PayloadPath::of(["fields", "priority", "name"])],
                ..KindPaths::default()
            }],
        )
        .with(
            "gitea",
            vec![KindPaths {
                kind: "pr".to_owned(),
                merged: vec![PayloadPath::of(["merged"])],
                reviewers: vec![ListPath {
                    at: PayloadPath::of(["requested_reviewers"]),
                    entry: PayloadPath::of(["login"]),
                }],
                ..KindPaths::default()
            }],
        )
}

#[tokio::test]
async fn a_declared_path_resolves_the_same_in_sql_and_in_rust() {
    let pool = scratch().await;
    let declarations = declarations();

    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-1",
        serde_json::json!({ "fields": {
            "status": { "name": "In Progress" },
            "priority": { "name": "High" }
        }}),
        (Some("In Progress"), Some("High"), None, &[]),
    )
    .await;

    // The second candidate, which is what a `coalesce` arm used to be: the
    // first lands on nothing, the flat spelling answers.
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-2",
        serde_json::json!({ "status": "To Do" }),
        (Some("To Do"), None, None, &[]),
    )
    .await;

    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#7",
        serde_json::json!({
            "merged": true,
            "requested_reviewers": [{ "login": "tom.reyes" }, { "login": "mara.lindqvist" }]
        }),
        (None, None, Some(true), &["mara.lindqvist", "tom.reyes"]),
    )
    .await;
}

/// The miss directions, both implementations, one case each. A path that lands
/// on an object, on a blank string or on nothing at all contributes **no**
/// value -- `->>` would have stringified the object into `{"id":3}`, which is
/// a guess dressed as an observation.
#[tokio::test]
async fn a_path_that_lands_on_no_usable_value_misses_in_both() {
    let pool = scratch().await;
    let declarations = declarations();

    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-10",
        serde_json::json!({ "fields": { "status": { "name": { "id": 3 } } } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-11",
        serde_json::json!({ "fields": { "status": { "name": "   " } } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-12",
        serde_json::json!({ "fields": { "status": serde_json::Value::Null } }),
        (None, None, None, &[]),
    )
    .await;
    // A flag is a boolean or it is nothing, and a list that is not an array
    // yields nothing rather than raising -- `jsonb_array_elements` raises on a
    // scalar, which would fail the read for every source at once.
    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#8",
        serde_json::json!({ "merged": "true", "requested_reviewers": { "login": "mara" } }),
        (None, None, None, &[]),
    )
    .await;
    both(
        &pool,
        &declarations,
        "gitea",
        "pr",
        "tidewater/payout#9",
        serde_json::json!({ "requested_reviewers": serde_json::Value::Null }),
        (None, None, None, &[]),
    )
    .await;
}

/// Three ways a declaration can be absent, and all three miss rather than
/// falling back on a shape knobas happens to recognise. This is the clause
/// that makes "a kind that declares no path is a miss, never a guess" a fact
/// about the code rather than a promise on a doc comment: the payload below is
/// Jira-shaped in every case, and no reader may read it.
#[tokio::test]
async fn an_undeclared_source_kind_or_field_misses_in_both() {
    let pool = scratch().await;
    let declarations = declarations();
    let jira_shaped = serde_json::json!({ "fields": {
        "status": { "name": "In Progress" }, "priority": { "name": "High" }
    }});

    // A source nothing declares for.
    both(
        &pool,
        &declarations,
        "jira-eu",
        "ticket",
        "PAY-20",
        jira_shaped.clone(),
        (None, None, None, &[]),
    )
    .await;
    // A kind this source does not declare: Jira declares `ticket` only.
    both(
        &pool,
        &declarations,
        "jira",
        "page",
        "PAY-21",
        jira_shaped.clone(),
        (None, None, None, &[]),
    )
    .await;
    // A field this kind does not declare: Jira's `ticket` declares no merged
    // flag and no reviewers, and this pull-request-shaped payload carries
    // both.
    both(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-22",
        serde_json::json!({
            "merged": true,
            "requested_reviewers": [{ "login": "mara" }],
            "fields": { "status": { "name": "In Progress" } }
        }),
        (Some("In Progress"), None, None, &[]),
    )
    .await;
    // Nobody declares anything at all.
    both(
        &pool,
        &Declarations::empty(),
        "jira",
        "ticket",
        "PAY-23",
        jira_shaped,
        (None, None, None, &[]),
    )
    .await;
}

// -- the parent declaration (ADR-0018, #595) ---------------------------------
//
// The deputy's ruling of 2026-10-11 on #594 freezes one `at` with two shapes
// on one condition: the branch on shape lives in exactly one SQL macro
// (`declared_parents!`, read through `declared_parent_names!`) and one Rust
// resolver (`resolve_parents`), held together here. Every `under:` read goes
// through them. So every case below runs through both, and states its answer.

/// A parent as the comparison sees it: its kind and the names it answers to,
/// in chain order, outermost first.
type Chain = Vec<(String, Vec<String>)>;

/// The chain the SQL macro yields for one item, ordered by its position and,
/// inside a parent, by the order its names were declared in.
async fn chain_in_sql(pool: &PgPool, declarations: &Declarations, entity_id: &str) -> Chain {
    const PARENTS: &str = concat!(
        "select p.pos, p.kind from sync.live_item i cross join lateral ",
        knobas_core::declared_parents!("$1"),
        " p where i.entity_id = $2 order by p.pos"
    );
    const NAMES: &str = concat!(
        "select n.pos, n.name from sync.live_item i cross join lateral ",
        knobas_core::declared_parent_names!("$1"),
        " n where i.entity_id = $2 order by n.pos, n.name_ord"
    );
    let parents: Vec<(i64, String)> = sqlx::query_as(PARENTS)
        .bind(declarations.as_param())
        .bind(entity_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("{entity_id}'s parents must resolve: {e}"));
    let names: Vec<(i64, String)> = sqlx::query_as(NAMES)
        .bind(declarations.as_param())
        .bind(entity_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("{entity_id}'s parent names must resolve: {e}"));
    parents
        .into_iter()
        .map(|(pos, kind)| {
            let of_this: Vec<String> = names
                .iter()
                .filter(|(at, _)| *at == pos)
                .map(|(_, name)| name.clone())
                .collect();
            (kind, of_this)
        })
        .collect()
}

/// The names one item answers to **as a parent**, through the database,
/// sorted: the SQL side is a set.
async fn self_names_in_sql(
    pool: &PgPool,
    declarations: &Declarations,
    entity_id: &str,
) -> Vec<String> {
    const SELF: &str = concat!(
        "select s.name from sync.live_item i cross join lateral ",
        knobas_core::declared_names_as_parent!("$1"),
        " s where i.entity_id = $2 order by s.name"
    );
    sqlx::query_scalar(SELF)
        .bind(declarations.as_param())
        .bind(entity_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("{entity_id}'s own names must resolve: {e}"))
}

/// Run one item through both implementations of the parent read and assert
/// they agree with each other **and** with the stated chain and own names.
#[allow(clippy::too_many_arguments)]
async fn both_parents(
    pool: &PgPool,
    declarations: &Declarations,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
    chain: &[(&str, &[&str])],
    own_names: &[&str],
) {
    item(pool, source, kind, key, &payload).await;
    let id = format!("{source}:{key}");
    let expected: Chain = chain
        .iter()
        .map(|(kind, names)| {
            (
                (*kind).to_owned(),
                names.iter().map(|n| (*n).to_owned()).collect(),
            )
        })
        .collect();

    let empty = KindPaths::default();
    let declared = declarations.get(source, kind).unwrap_or(&empty);
    let rust: Chain = resolve_parents(&payload, &declared.parent)
        .into_iter()
        .map(|parent| (parent.kind, parent.names))
        .collect();
    assert_eq!(rust, expected, "Rust parent chain for {key}");
    assert_eq!(
        chain_in_sql(pool, declarations, &id).await,
        expected,
        "SQL parent chain for {key}"
    );

    let mut own: Vec<String> = own_names.iter().map(|n| (*n).to_owned()).collect();
    own.sort();
    let mut rust_own = resolve_names_as_parent(&payload, kind, declarations.of_source(source));
    rust_own.sort();
    assert_eq!(rust_own, own, "Rust names {key} answers to as a parent");
    assert_eq!(
        self_names_in_sql(pool, declarations, &id).await,
        own,
        "SQL names {key} answers to as a parent"
    );
}

/// Gitea's repo as one object, Confluence's chain as an array, and a Jira
/// ticket with two places its parent may be -- the shapes #595 to #599
/// declare.
fn parent_declarations() -> Declarations {
    let repo = |at: &[&str]| ParentPath {
        kind: "repo".to_owned(),
        at: PayloadPath::of(at.iter().copied()),
        names: vec![PayloadPath::of(["full_name"]), PayloadPath::of(["name"])],
    };
    Declarations::empty()
        .with(
            "gitea",
            vec![
                KindPaths {
                    kind: "commit".to_owned(),
                    parent: vec![repo(&["repository"])],
                    ..KindPaths::default()
                },
                KindPaths {
                    kind: "pr".to_owned(),
                    parent: vec![repo(&["base", "repo"])],
                    ..KindPaths::default()
                },
            ],
        )
        .with(
            "confluence",
            vec![KindPaths {
                kind: "page".to_owned(),
                parent: vec![ParentPath {
                    kind: "page".to_owned(),
                    at: PayloadPath::of(["ancestors"]),
                    names: vec![PayloadPath::of(["title"])],
                }],
                ..KindPaths::default()
            }],
        )
        .with(
            "jira",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                parent: vec![
                    ParentPath {
                        kind: "ticket".to_owned(),
                        at: PayloadPath::of(["fields", "epic"]),
                        names: vec![PayloadPath::of(["key"])],
                    },
                    ParentPath {
                        kind: "ticket".to_owned(),
                        at: PayloadPath::of(["fields", "parent"]),
                        names: vec![PayloadPath::of(["key"])],
                    },
                ],
                ..KindPaths::default()
            }],
        )
}

/// Both shapes `at` may take: one object is the direct parent, an array is the
/// whole chain, outermost first. A blank or non-string name is dropped from
/// its parent and the parent stays; a non-object element is dropped from the
/// chain.
#[tokio::test]
async fn a_parent_resolves_the_same_in_sql_and_in_rust_in_both_shapes() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    both_parents(
        &pool,
        &declarations,
        "gitea",
        "commit",
        "tidewater/payout-service@c90d11",
        serde_json::json!({
            "sha": "c90d11",
            "repository": { "full_name": "tidewater/payout-service", "name": "payout-service" }
        }),
        &[("repo", &["tidewater/payout-service", "payout-service"])],
        &[],
    )
    .await;

    both_parents(
        &pool,
        &declarations,
        "confluence",
        "page",
        "4711",
        serde_json::json!({
            "title": "SEPA retries",
            "ancestors": [
                { "title": "Runbooks" },
                "not an object",
                { "title": "  " },
                { "title": "Payments" }
            ]
        }),
        &[
            ("page", &["Runbooks"]),
            ("page", &[]),
            ("page", &["Payments"]),
        ],
        // A page is itself a parent kind Confluence declares, so it answers
        // to its own title.
        &["SEPA retries"],
    )
    .await;
}

/// The miss directions: an `at` that leads to a string, a number or nothing
/// is no parent at all, and so is the empty path, which would otherwise make
/// every item its own parent. An empty array is an answer -- "no ancestors" --
/// and not a miss.
#[tokio::test]
async fn a_parent_at_that_leads_to_neither_shape_misses_in_both() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    both_parents(
        &pool,
        &declarations,
        "gitea",
        "commit",
        "tidewater/payout-service@string",
        serde_json::json!({ "repository": "tidewater/payout-service" }),
        &[],
        &[],
    )
    .await;
    both_parents(
        &pool,
        &declarations,
        "gitea",
        "commit",
        "tidewater/payout-service@absent",
        serde_json::json!({ "sha": "1" }),
        &[],
        &[],
    )
    .await;
    both_parents(
        &pool,
        &declarations,
        "confluence",
        "page",
        "4712",
        serde_json::json!({ "title": "Home", "ancestors": [] }),
        &[],
        &["Home"],
    )
    .await;

    let empty_at = Declarations::empty().with(
        "gitea",
        vec![KindPaths {
            kind: "commit".to_owned(),
            parent: vec![ParentPath {
                kind: "repo".to_owned(),
                at: PayloadPath::of(Vec::<String>::new()),
                names: vec![PayloadPath::of(["name"])],
            }],
            ..KindPaths::default()
        }],
    );
    both_parents(
        &pool,
        &empty_at,
        "gitea",
        "commit",
        "tidewater/payout-service@empty-at",
        serde_json::json!({ "name": "itself" }),
        &[],
        &[],
    )
    .await;
}

/// Candidates in declaration order, the first whose `at` resolves winning --
/// the rule every declared path here follows. A ticket carrying both an epic
/// and a parent is under its epic; one carrying only a parent is under that.
#[tokio::test]
async fn the_first_parent_candidate_that_resolves_wins_in_both() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    both_parents(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-231",
        serde_json::json!({
            "key": "PAY-231",
            "fields": { "epic": { "key": "PAY-100" }, "parent": { "key": "PAY-7" } }
        }),
        &[("ticket", &["PAY-100"])],
        &["PAY-231"],
    )
    .await;
    both_parents(
        &pool,
        &declarations,
        "jira",
        "ticket",
        "PAY-232",
        serde_json::json!({
            "key": "PAY-232",
            "fields": { "epic": "PAY-100", "parent": { "key": "PAY-7" } }
        }),
        &[("ticket", &["PAY-7"])],
        &["PAY-232"],
    )
    .await;
}

/// A mirrored parent answers to the names its children's declaration names,
/// read off its own record: a Gitea repo's `full_name` and `name` sit at its
/// root exactly as they sit inside a pull request's `base.repo`. A kind no
/// declaration names as a parent answers to nothing, and so does a source that
/// declares nothing.
#[tokio::test]
async fn a_mirrored_parent_answers_to_its_declared_names_in_both() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    both_parents(
        &pool,
        &declarations,
        "gitea",
        "repo",
        "tidewater/payout-service",
        serde_json::json!({ "full_name": "tidewater/payout-service", "name": "payout-service" }),
        &[],
        &["payout-service", "tidewater/payout-service"],
    )
    .await;
    both_parents(
        &pool,
        &declarations,
        "gitea",
        "branch",
        "tidewater/payout-service@refs/heads/main",
        serde_json::json!({ "full_name": "x/y", "name": "main" }),
        &[],
        &[],
    )
    .await;
    both_parents(
        &pool,
        &Declarations::empty(),
        "gitea",
        "repo",
        "tidewater/other",
        serde_json::json!({ "full_name": "tidewater/other", "name": "other" }),
        &[],
        &[],
    )
    .await;
}

// -- the path line (#596) ------------------------------------------------------
//
// A row's path line is the parent chain its kind declares: each parent's first
// name, outermost first, joined by the one separator. `declared_parent_path!`
// is that rule in SQL and `parent_path` over `resolve_parents` in Rust; both
// stand on the pair above, so the shape branch is still in one place.
// `path_line_read!`, the expression every row reads, is that chain and nothing
// else since #646 retired the Confluence ancestor read it fell back to.

/// What one item's path line is, through each implementation: the declared
/// chain in SQL, the same in Rust, and the whole line a row reads.
async fn path_lines(
    pool: &PgPool,
    declarations: &Declarations,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> (Option<String>, Option<String>, Option<String>) {
    const LINES: &str = concat!(
        "select ",
        knobas_core::declared_parent_path!("$1"),
        " as declared, ",
        knobas_core::path_line_read!("$1"),
        " as line from sync.live_item i where i.entity_id = $2"
    );
    item(pool, source, kind, key, &payload).await;
    let row = sqlx::query(LINES)
        .bind(declarations.as_param())
        .bind(format!("{source}:{key}"))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{key}'s path line must resolve: {e}"));
    let empty = KindPaths::default();
    let declared = declarations.get(source, kind).unwrap_or(&empty);
    let rust = parent_path(&resolve_parents(&payload, &declared.parent));
    (row.get("declared"), rust, row.get("line"))
}

/// Gitea's one parent draws as its first name, `owner/repo`, not as every
/// name it answers to; a Confluence chain draws outermost first, a parent with
/// no usable name left out rather than drawn as an empty segment.
#[tokio::test]
async fn a_path_line_is_each_parents_first_name_in_sql_and_in_rust() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    let commit = path_lines(
        &pool,
        &declarations,
        "gitea",
        "commit",
        "tidewater/payout-service@line",
        serde_json::json!({
            "repository": { "full_name": "tidewater/payout-service", "name": "payout-service" }
        }),
    )
    .await;
    let repo = Some("tidewater/payout-service".to_owned());
    assert_eq!(commit, (repo.clone(), repo.clone(), repo));

    let page = path_lines(
        &pool,
        &declarations,
        "confluence",
        "page",
        "4801",
        serde_json::json!({
            "title": "SEPA retries",
            "ancestors": [{ "title": "Runbooks" }, { "title": " " }, { "title": "Payments" }]
        }),
    )
    .await;
    let chain = Some("Runbooks \u{203a} Payments".to_owned());
    assert_eq!(page, (chain.clone(), chain.clone(), chain));
}

/// The misses: a kind that declares no parent, an `at` that leads to neither
/// shape, and a parent that answers to no name all draw no line. A record
/// that carries Confluence's `ancestors` is no exception where nothing is
/// declared: #284's ancestor read was the row's fallback until #646 retired
/// it, so the line is the declaration's and nothing else's.
#[tokio::test]
async fn a_path_line_misses_in_both_and_reads_nothing_undeclared() {
    let pool = scratch().await;
    let declarations = parent_declarations();

    for (key, payload) in [
        (
            "tidewater/payout-service@bare",
            serde_json::json!({ "repository": "tidewater/payout-service" }),
        ),
        (
            "tidewater/payout-service@nameless",
            serde_json::json!({ "repository": { "id": 4 } }),
        ),
    ] {
        let lines = path_lines(&pool, &declarations, "gitea", "commit", key, payload).await;
        assert_eq!(lines, (None, None, None), "{key}");
    }

    let repo = path_lines(
        &pool,
        &declarations,
        "gitea",
        "repo",
        "tidewater/payout-service",
        serde_json::json!({ "full_name": "tidewater/payout-service", "name": "payout-service" }),
    )
    .await;
    assert_eq!(repo, (None, None, None), "a repo declares no parent");

    let undeclared = path_lines(
        &pool,
        &Declarations::empty(),
        "confluence",
        "page",
        "4802",
        serde_json::json!({ "title": "SEPA retries", "ancestors": [{ "title": "Engineering" }] }),
    )
    .await;
    assert_eq!(
        undeclared,
        (None, None, None),
        "a page whose kind declares nothing draws no line, ancestors or not"
    );
}

// -- the state chip (#666) -------------------------------------------------------
//
// A row's state chip is the first declared candidate whose value is in that
// candidate's own value map: the chip's text (the value itself unless the map
// renames it) and its tone. `declared_state!` is the rule in SQL, and
// `resolve_state` in Rust; every case goes through both.

/// Gitea's pull request and TeamCity's build, as each adapter declares them:
/// the merged flag before the state word, and a build's in-flight `state`
/// before its finished `status`.
fn state_declarations() -> Declarations {
    let value = |value: serde_json::Value, tone: Tone, text: Option<&str>| StateValue {
        value,
        tone,
        text: text.map(str::to_owned),
    };
    Declarations::empty()
        .with(
            "gitea",
            vec![KindPaths {
                kind: "pr".to_owned(),
                state: vec![
                    StatePath {
                        at: PayloadPath::of(["merged"]),
                        values: vec![value(serde_json::json!(true), Tone::Ok, Some("merged"))],
                    },
                    StatePath {
                        at: PayloadPath::of(["state"]),
                        values: vec![
                            value(serde_json::json!("open"), Tone::Info, None),
                            value(serde_json::json!("closed"), Tone::Muted, None),
                        ],
                    },
                ],
                ..KindPaths::default()
            }],
        )
        .with(
            "teamcity",
            vec![KindPaths {
                kind: "build".to_owned(),
                state: vec![
                    StatePath {
                        at: PayloadPath::of(["state"]),
                        values: vec![
                            value(serde_json::json!("running"), Tone::Info, None),
                            value(serde_json::json!("queued"), Tone::Info, None),
                        ],
                    },
                    StatePath {
                        at: PayloadPath::of(["status"]),
                        values: vec![
                            value(serde_json::json!("SUCCESS"), Tone::Ok, None),
                            value(serde_json::json!("FAILURE"), Tone::Err, None),
                        ],
                    },
                ],
                ..KindPaths::default()
            }],
        )
}

/// One item's chip, through the database and in Rust.
async fn chips(
    pool: &PgPool,
    declarations: &Declarations,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> (Option<StateChip>, Option<StateChip>) {
    const CHIP: &str = concat!(
        "select ",
        knobas_core::declared_state!("$1"),
        " as state from sync.live_item i where i.entity_id = $2"
    );
    item(pool, source, kind, key, &payload).await;
    let row = sqlx::query(CHIP)
        .bind(declarations.as_param())
        .bind(format!("{source}:{key}"))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{key}'s state must resolve: {e}"));
    let sql: Option<sqlx::types::Json<StateChip>> = row.get("state");
    let empty = KindPaths::default();
    let declared = declarations.get(source, kind).unwrap_or(&empty);
    (sql.map(|j| j.0), resolve_state(&payload, &declared.state))
}

fn chip(text: &str, tone: Tone) -> Option<StateChip> {
    Some(StateChip {
        text: text.to_owned(),
        tone,
    })
}

/// The first candidate whose value its map knows wins, renamed where the map
/// says so: a merged pull request is `merged` though its `state` is `closed`,
/// and a finished build falls through `state` to its outcome.
#[tokio::test]
async fn a_state_chip_is_the_first_mapped_value_in_sql_and_in_rust() {
    let pool = scratch().await;
    let declarations = state_declarations();
    let cases = [
        ("gitea", "pr", "r/p#1", serde_json::json!({ "merged": true, "state": "closed" }), chip("merged", Tone::Ok)),
        ("gitea", "pr", "r/p#2", serde_json::json!({ "merged": false, "state": "open" }), chip("open", Tone::Info)),
        ("gitea", "pr", "r/p#3", serde_json::json!({ "merged": false, "state": "closed" }), chip("closed", Tone::Muted)),
        ("teamcity", "build", "1", serde_json::json!({ "state": "running", "status": "SUCCESS" }), chip("running", Tone::Info)),
        ("teamcity", "build", "2", serde_json::json!({ "state": "finished", "status": "FAILURE" }), chip("FAILURE", Tone::Err)),
        ("teamcity", "build", "3", serde_json::json!({ "state": "finished", "status": " SUCCESS " }), chip("SUCCESS", Tone::Ok)),
    ];
    for (source, kind, key, payload, expected) in cases {
        let (sql, rust) = chips(&pool, &declarations, source, kind, key, payload).await;
        assert_eq!(sql, expected, "SQL chip for {key}");
        assert_eq!(rust, expected, "Rust chip for {key}");
    }
}

/// A value no map names, a value that is not a string or a boolean, a kind
/// that declares no state and a read bound to no declarations: all four miss,
/// in both, rather than drawing a chip nobody declared.
#[tokio::test]
async fn a_state_chip_misses_in_both_and_never_guesses() {
    let pool = scratch().await;
    let declarations = state_declarations();
    let cases = [
        ("teamcity", "build", "4", serde_json::json!({ "state": "finished", "status": "UNHEARD" }), &declarations),
        ("teamcity", "build", "5", serde_json::json!({ "state": { "name": "running" }, "status": ["FAILURE"] }), &declarations),
        ("gitea", "pr", "r/p#4", serde_json::json!({ "merged": "true", "state": "draft" }), &declarations),
        ("gitea", "pr", "r/p#5", serde_json::json!({ "merged": false, "state": "  " }), &declarations),
        ("gitea", "commit", "r/p@abc", serde_json::json!({ "merged": true, "state": "open" }), &declarations),
    ];
    for (source, kind, key, payload, declared) in cases {
        let (sql, rust) = chips(&pool, declared, source, kind, key, payload).await;
        assert_eq!(sql, None, "SQL chip for {key}");
        assert_eq!(rust, None, "Rust chip for {key}");
    }
    let (sql, rust) = chips(
        &pool,
        &Declarations::empty(),
        "gitea",
        "pr",
        "r/p#6",
        serde_json::json!({ "merged": true, "state": "closed" }),
    )
    .await;
    assert_eq!((sql, rust), (None, None), "no declarations, no chip");
}
