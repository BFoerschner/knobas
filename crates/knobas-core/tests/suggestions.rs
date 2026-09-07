//! The suggestion engine, against a real PostgreSQL.
//!
//! What is asserted here is **which suggestions are proposed and what happens
//! when the user acts** -- never how a detector scanned text. Each rule gets a
//! positive and a negative case in one test, because a rule that fires for
//! everything is as broken as one that never fires and only the negative case
//! catches the first.
//!
//! # Why every test gets a database of its own
//!
//! Detection is a pass over the **whole** mirror, and that is the point of it:
//! a rule reads every branch, not the branches one caller nominated. The
//! binary's shared database therefore cannot isolate these tests the way
//! unique ids isolate the link battery -- test A's `detect` run scans test B's
//! corpus and proposes into it, so B's `detect` then finds its own suggestion
//! already there and *correctly* writes nothing. Every negative assertion in
//! this file would be decided by scheduling.
//!
//! `scratch_database` costs one `create database` per test on the same
//! postmaster -- no second server -- and buys assertions that mean what they
//! say. It is also what lets the fixtures read as `PAY-231` rather than as a
//! uuid.

use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::suggest::{self, RuleClass};
use knobas_core::{CoreError, link};
use sqlx::PgPool;
use uuid::Uuid;

/// The source every fixture below is mirrored under.
const SOURCE: &str = "jira";

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("suggest")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// Put one live mirror item in the corpus, and return its address.
///
/// Both halves, because every rule reads `sync.live_item`, which is the join of
/// the two: an entity row alone is linkable but invisible to detection, which
/// is exactly the state a note or a context is in.
async fn item(pool: &PgPool, kind: &str, key: &str, title: &str, body: &str) -> String {
    from(pool, SOURCE, kind, key, title, body, serde_json::json!({})).await
}

/// As [`item`], with a payload -- what the source-recorded rule reads.
async fn with_payload(
    pool: &PgPool,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    from(pool, SOURCE, kind, key, title, "", payload).await
}

/// An entity that exists but has never been mirrored -- knobas' own kinds, and
/// anything a link may point at that detection cannot see.
async fn entity_only(pool: &PgPool, namespace: &str, kind: &str, key: &str) -> String {
    let id = EntityRef::new(namespace, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// One mirrored item, in the source named.
async fn from(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
) -> String {
    let id = entity_only(pool, source, kind, key).await;
    sqlx::query("update knobas.entity set title = $2 where id = $1")
        .bind(&id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The source the monitors below are mirrored under.
///
/// Not [`SOURCE`]: a monitor is an Uptime Kuma item, and mirroring one under
/// `jira` would let a rule that reads the wrong column still pass.
const KUMA: &str = "kuma";

/// One mirrored monitor, with the address it watches -- the payload shape
/// `knobas_source_kuma::map` writes, where **every key is present** and a
/// monitor with no URL carries `null` rather than omitting it.
async fn monitor(pool: &PgPool, key: &str, name: &str, url: Option<&str>) -> String {
    let payload = serde_json::json!({
        "id": key,
        "name": name,
        "url": url,
        "hostname": serde_json::Value::Null,
        "state": "up",
    });
    from(pool, KUMA, "monitor", key, name, "", payload).await
}

/// One asset in the estate's tree: the entity row and the asset row, written
/// here rather than through the store because the store is
/// `knobas_app::assets` and this crate cannot depend on it.
async fn asset(pool: &PgPool, name: &str, hostname: Option<&str>) -> String {
    let id = EntityRef::new("asset", name).to_string();
    let properties = match hostname {
        Some(host) => serde_json::json!({ "hostname": host }),
        None => serde_json::json!({}),
    };
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, properties) values ($1,'container',$2,$3)",
    )
    .bind(&id)
    .bind(name)
    .bind(properties)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A route the asset exposes -- the other half of what the monitor rule reads.
async fn route(pool: &PgPool, asset_id: &str, name: &str, url: &str) -> String {
    let id = EntityRef::new("route", name).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'route',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.route (id, asset_id, name, url) values ($1,$2,$3,$4)")
        .bind(&id)
        .bind(asset_id)
        .bind(name)
        .bind(url)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// Run one named rule. Panics on a name no rule carries, which is a typo in a
/// test rather than a failure worth a message.
async fn run_rule(pool: &PgPool, name: &str) -> u64 {
    let rule = suggest::rule(name).expect("a rule by that name");
    suggest::detect_rule(pool, rule).await.unwrap()
}

/// Every proposal in the corpus, newest first.
async fn tray(pool: &PgPool) -> Vec<suggest::SuggestionEntry> {
    suggest::proposals(pool, &[], None, 200).await.unwrap()
}

/// The `(from, to)` pairs the tray holds, as a set.
fn pairs(entries: &[suggest::SuggestionEntry]) -> std::collections::BTreeSet<(String, String)> {
    entries
        .iter()
        .map(|e| (e.link.from_id.clone(), e.link.to_id.clone()))
        .collect()
}

/// The one proposal connecting `a` and `b`, in either direction.
fn between<'e>(
    entries: &'e [suggest::SuggestionEntry],
    a: &str,
    b: &str,
) -> Option<&'e suggest::SuggestionEntry> {
    entries.iter().find(|e| {
        (e.link.from_id == a && e.link.to_id == b) || (e.link.from_id == b && e.link.to_id == a)
    })
}

// -- one test per rule, each with its negative control ----------------------

#[tokio::test]
async fn a_branch_name_proposes_the_ticket_it_names_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let named = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    // Four negative controls. The first is the cheap one -- a branch with no
    // key at all is caught by any rule that reads the right column. The other
    // three are what the pattern's word boundaries buy, and each fails
    // differently:
    //
    // * `PAY-2311` is a *different ticket*, and the greedy digits already
    //   swallow it whole, so this one is about the join and not the anchors.
    // * `PAY-231x` needs `\M`: the digits stop at a letter on their own, so
    //   without it the match ends mid-token and reads as PAY-231.
    // * `xPAY-231` needs `\m`: the match simply starts at the first capital.
    let unnamed = item(&pool, "branch", "b2", "feature/retry-storm", "").await;
    let longer = item(&pool, "branch", "b3", "feature/PAY-2311-other", "").await;
    let suffixed = item(&pool, "branch", "b4", "feature/PAY-231x-retry", "").await;
    let prefixed = item(&pool, "branch", "b5", "feature/xPAY-231-retry", "").await;

    run_rule(&pool, "branch_name_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(named.clone(), ticket.clone())].into_iter().collect(),
        "only the branch that names PAY-231 proposes it"
    );
    assert!(between(&entries, &unnamed, &ticket).is_none());
    for (branch, why) in [
        (&longer, "PAY-2311 is not PAY-231"),
        (&suffixed, "PAY-231x is not PAY-231"),
        (&prefixed, "xPAY-231 is not PAY-231"),
    ] {
        assert!(between(&entries, branch, &ticket).is_none(), "{why}");
    }

    let proposal = &entries[0];
    assert_eq!(
        proposal.link.reason.as_deref(),
        Some("the branch name contains PAY-231"),
        "the reason names the key, not the rule"
    );
    assert_eq!(proposal.link.rule.as_deref(), Some("branch_name_key"));
    assert_eq!(proposal.link.rule_class, Some(RuleClass::ExactKey));
    assert_eq!(proposal.link.origin, Origin::Suggested);
    assert_eq!(proposal.link.confirmed_at, None);
    assert_eq!(proposal.from.entity_id, named);
    assert_eq!(proposal.to.title, "Payout retry storm");
}

#[tokio::test]
async fn a_commit_message_proposes_the_ticket_it_mentions_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    // The key is in the trailer, not the subject: a rule reading only the title
    // would pass a test written the other way round.
    let names = item(
        &pool,
        "commit",
        "c1",
        "Retry the payout once",
        "Refs PAY-231, and nothing else.",
    )
    .await;
    let silent = item(
        &pool,
        "commit",
        "c2",
        "Retry the payout once",
        "No ticket was harmed.",
    )
    .await;

    run_rule(&pool, "commit_message_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(names.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &silent, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("the commit message mentions PAY-231")
    );
    assert_eq!(entries[0].link.rule_class, Some(RuleClass::ExactKey));
}

#[tokio::test]
async fn a_builds_parameters_propose_the_ticket_they_name_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    // What an adapter puts in a build's `body_text`: the branch it ran on and
    // its status text.
    let onto = item(
        &pool,
        "build",
        "b1",
        "Payments :: Verify #4821",
        "SUCCESS\nrefs/heads/feature/PAY-231-retry",
    )
    .await;
    let elsewhere = item(
        &pool,
        "build",
        "b2",
        "Payments :: Verify #4822",
        "SUCCESS\nrefs/heads/main",
    )
    .await;

    run_rule(&pool, "build_parameter_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(onto.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &elsewhere, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("this build's parameters name PAY-231")
    );
}

#[tokio::test]
async fn a_page_proposes_the_ticket_its_text_mentions_and_nothing_else() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let documents = item(
        &pool,
        "page",
        "ENG:Retries",
        "Retry policy",
        "The storm behaviour is tracked in PAY-231.",
    )
    .await;
    let unrelated = item(
        &pool,
        "page",
        "ENG:Onboarding",
        "Onboarding",
        "Where the coffee is.",
    )
    .await;

    run_rule(&pool, "page_text_key").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(documents.clone(), ticket.clone())].into_iter().collect()
    );
    assert!(between(&entries, &unrelated, &ticket).is_none());
    assert_eq!(
        entries[0].link.reason.as_deref(),
        Some("this page mentions PAY-231")
    );
}

/// A key found by the wrong rule is not found at all.
///
/// The four exact-key rules are one mechanism pointed at four kinds, and the
/// cheapest way for that to go wrong is a `where kind = ...` that drifts.
///
/// **A corpus per rule, deliberately.** Running the four in turn against one
/// corpus proves nothing about the last of them: by the time it runs, the
/// suppression has already claimed every pair its neighbours found, so a page
/// rule that had quietly grown `or kind = 'commit'` writes nothing extra and
/// the test stays green. Found by mutating exactly that. Each rule therefore
/// gets an untouched corpus holding one item of every kind, all naming
/// PAY-231, and must claim exactly its own.
#[tokio::test]
async fn each_exact_key_rule_reads_only_its_own_kind() {
    for (rule, kind) in [
        ("branch_name_key", "branch"),
        ("commit_message_key", "commit"),
        ("build_parameter_key", "build"),
        ("page_text_key", "page"),
    ] {
        let pool = scratch().await;
        let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
        let mut by_kind = std::collections::BTreeMap::new();
        by_kind.insert(
            "branch",
            item(&pool, "branch", "b1", "feature/PAY-231", "").await,
        );
        by_kind.insert(
            "commit",
            item(&pool, "commit", "c1", "Fix PAY-231", "").await,
        );
        by_kind.insert(
            "build",
            item(&pool, "build", "d1", "Verify #1", "PAY-231").await,
        );
        by_kind.insert("page", item(&pool, "page", "p1", "Notes", "PAY-231").await);

        run_rule(&pool, rule).await;

        assert_eq!(
            pairs(&tray(&pool).await),
            [(by_kind[kind].clone(), ticket.clone())]
                .into_iter()
                .collect(),
            "{rule} must propose its own kind and nothing else"
        );
    }
}

#[tokio::test]
async fn a_relation_the_source_records_becomes_a_proposal_with_the_sources_own_wording() {
    let pool = scratch().await;
    let blocker = with_payload(
        &pool,
        "ticket",
        "PAY-231",
        "Payout retry storm",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
             "outwardIssue": {"key": "PAY-232"}}
        ]}}),
    )
    .await;
    let blocked = item(&pool, "ticket", "PAY-232", "Ledger drift", "").await;
    // Negative control: an issue whose payload records no relation at all.
    let alone = item(&pool, "ticket", "PAY-233", "Unrelated", "").await;

    run_rule(&pool, "source_recorded_relation").await;

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(blocker.clone(), blocked.clone())].into_iter().collect(),
        "the direction follows the outward issue"
    );
    assert!(between(&entries, &alone, &blocker).is_none());

    let proposal = &entries[0];
    assert_eq!(proposal.link.relation, "blocks");
    assert_eq!(
        proposal.link.reason.as_deref(),
        Some("jira already records this link (Blocks)")
    );
    assert_eq!(proposal.link.rule_class, Some(RuleClass::SourceRelation));
    assert_eq!(
        proposal.link.origin,
        Origin::Source,
        "a relation the source states came from the source, however it is confirmed"
    );
    assert_eq!(
        proposal.link.confirmed_at, None,
        "evidence is still not consent: nothing enters the graph unconfirmed"
    );
}

/// The inward side of a Jira link is the same relation read backwards, and it
/// must produce the same edge rather than a second one spelled the other way.
#[tokio::test]
async fn an_inward_source_relation_is_stored_as_the_outward_phrase() {
    let pool = scratch().await;
    let blocked = with_payload(
        &pool,
        "ticket",
        "PAY-241",
        "Ledger drift",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
             "inwardIssue": {"key": "PAY-240"}}
        ]}}),
    )
    .await;
    let blocker = item(&pool, "ticket", "PAY-240", "Retry storm", "").await;

    run_rule(&pool, "source_recorded_relation").await;

    let entries = tray(&pool).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].link.from_id, blocker,
        "the blocker is the from end"
    );
    assert_eq!(entries[0].link.to_id, blocked);
    assert_eq!(
        entries[0].link.relation, "blocks",
        "one relationship, one spelling -- never the inward phrase as a second relation"
    );
}

/// The other end of a source-recorded relation is addressed in the **same**
/// source, so two Jiras cannot cross-link by bare key.
#[tokio::test]
async fn a_source_relation_never_reaches_into_another_source() {
    let pool = scratch().await;
    let issuer = with_payload(
        &pool,
        "ticket",
        "PAY-251",
        "Ours",
        serde_json::json!({"fields": {"issuelinks": [
            {"type": {"name": "Blocks", "outward": "blocks"},
             "outwardIssue": {"key": "PAY-252"}}
        ]}}),
    )
    .await;
    // The same bare key in a different namespace: a different ticket in a
    // different system that happens to be numbered alike.
    let impostor = from(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-252",
        "Theirs",
        "",
        serde_json::json!({}),
    )
    .await;

    run_rule(&pool, "source_recorded_relation").await;

    assert!(
        between(&tray(&pool).await, &issuer, &impostor).is_none(),
        "PAY-252 in another Jira is not the PAY-252 this issue links to"
    );
}

#[tokio::test]
async fn text_that_reads_alike_is_proposed_and_labelled_as_a_guess() {
    let pool = scratch().await;
    let incident = item(
        &pool,
        "ticket",
        "PAY-261",
        "Payout retry storm floods the ledger",
        "The payout retry storm floods the ledger with duplicate transfers.",
    )
    .await;
    let echo = item(
        &pool,
        "page",
        "ENG:Storm",
        "Retry storm runbook",
        "When the payout retry storm floods the ledger, duplicate transfers appear.",
    )
    .await;
    // Negative control: prose with nothing in common. Same source, same
    // corpus, same pass -- only the words differ.
    let unrelated = item(
        &pool,
        "page",
        "ENG:Coffee",
        "Kitchen rota",
        "Whoever finishes the milk buys the milk.",
    )
    .await;

    run_rule(&pool, "similar_text").await;

    let entries = tray(&pool).await;
    let found = between(&entries, &incident, &echo).expect("the two documents read alike");
    assert_eq!(found.link.rule_class, Some(RuleClass::Similarity));
    assert_eq!(found.link.rule.as_deref(), Some("similar_text"));
    let reason = found.link.reason.clone().unwrap();
    assert!(
        reason.starts_with("both mention "),
        "a similarity reason names the overlap: {reason}"
    );
    assert!(
        reason.contains("payout") || reason.contains("ledger") || reason.contains("storm"),
        "the reason must be specific rather than 'these look related': {reason}"
    );

    assert!(
        between(&entries, &incident, &unrelated).is_none(),
        "a rule that fires for everything is as broken as one that never fires"
    );
    assert!(between(&entries, &echo, &unrelated).is_none());
}

/// Two documents that merely share a couple of words are not similar.
///
/// The floor is the whole of this rule's judgement, and a floor of one would
/// pass every assertion in the test above.
#[tokio::test]
async fn a_couple_of_shared_words_is_below_the_similarity_floor() {
    let pool = scratch().await;
    let one = item(&pool, "ticket", "PAY-271", "Payout ledger", "").await;
    let two = item(&pool, "page", "ENG:Two", "Payout ledger", "").await;

    run_rule(&pool, "similar_text").await;

    assert!(
        between(&tray(&pool).await, &one, &two).is_none(),
        "two shared stems is under the floor of {}",
        suggest::SIMILARITY_FLOOR
    );
}

/// A monitor watching a host proposes the asset that *is* that host, and no
/// other asset in the estate.
///
/// Five negative controls, and each fails a different wrong implementation:
///
/// * an asset whose hostname nothing watches, so the rule cannot be "every
///   asset, every monitor";
/// * a monitor whose host no asset carries, the same test from the other end;
/// * `gitea.example.com` against a monitor on `gitea` -- host **equality**,
///   not a prefix, a suffix or a `like`;
/// * an asset whose hostname is `3000`, which a split on the wrong colon piece
///   would match;
/// * an asset whose hostname is `http`, which a URL read that forgot to strip
///   the scheme would match;
/// * a **repository** carrying the very same URL in its payload, so the rule
///   is about monitors and not about every mirrored item with an address.
#[tokio::test]
async fn a_monitor_proposes_the_asset_whose_hostname_it_watches_and_nothing_else() {
    let pool = scratch().await;
    let watching = monitor(&pool, "1", "knobas-gitea", Some("http://gitea:3000")).await;
    let off_estate = monitor(&pool, "2", "status page", Some("https://status.invalid/")).await;
    let addressless = monitor(&pool, "3", "a ping check", None).await;
    let not_a_monitor = from(
        &pool,
        "gitea",
        "repo",
        "knobas",
        "knobas",
        "",
        serde_json::json!({ "url": "http://gitea:3000" }),
    )
    .await;

    let gitea = asset(&pool, "knobas-gitea", Some("gitea")).await;
    let namesake = asset(&pool, "gitea-mirror", Some("gitea.example.com")).await;
    let porty = asset(&pool, "three-thousand", Some("3000")).await;
    let schemey = asset(&pool, "scheme", Some("http")).await;
    let hostless = asset(&pool, "no-hostname", None).await;

    let written = run_rule(&pool, "monitor_url_host").await;
    assert_eq!(written, 1, "one host match, one proposal");

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(gitea.clone(), watching.clone())].into_iter().collect(),
        "only the asset whose hostname the monitor watches is proposed"
    );

    let proposal = between(&entries, &gitea, &watching).expect("the monitor watches this asset");
    assert_eq!(
        proposal.link.relation, "monitored-by",
        "the asset is monitored by the monitor, not the other way round"
    );
    assert_eq!(proposal.link.from_id, gitea, "the asset is the subject");
    assert_eq!(proposal.link.to_id, watching);
    assert_eq!(proposal.link.rule.as_deref(), Some("monitor_url_host"));
    assert_eq!(proposal.link.rule_class, Some(RuleClass::ExactKey));
    assert_eq!(proposal.link.origin, Origin::Suggested);
    assert!(proposal.link.confirmed_at.is_none());
    let reason = proposal.link.reason.clone().unwrap();
    assert!(
        reason.contains("gitea") && reason.contains("hostname"),
        "the reason names the host it matched and where the asset states it: {reason}"
    );

    for (other, why) in [
        (&namesake, "gitea.example.com is not gitea"),
        (&porty, "3000 is the port, not the host"),
        (&schemey, "http is the scheme, not the host"),
        (&hostless, "an asset with no hostname matches nothing"),
    ] {
        assert!(between(&entries, other, &watching).is_none(), "{why}");
    }
    for (id, why) in [
        (
            &off_estate,
            "a monitor whose host no asset carries proposes nothing",
        ),
        (&addressless, "a monitor with no URL proposes nothing"),
        (
            &not_a_monitor,
            "a repository is not a monitor, whatever its payload says",
        ),
    ] {
        assert!(
            entries
                .iter()
                .all(|e| e.link.from_id != *id && e.link.to_id != *id),
            "{why}"
        );
    }
}

/// The other half of the rule: the host may be stated by a route the asset
/// exposes rather than by a hostname property.
///
/// The negative is a route on a *different* asset, so a rule that joined
/// routes to the estate rather than to the asset that exposes them would
/// propose the wrong end.
#[tokio::test]
async fn a_monitor_proposes_the_asset_exposing_the_route_it_watches() {
    let pool = scratch().await;
    let watching = monitor(
        &pool,
        "1",
        "kuma-status",
        Some("https://kuma.knobas.test/status"),
    )
    .await;

    let proxy = asset(&pool, "caddy", None).await;
    route(&pool, &proxy, "kuma", "https://kuma.knobas.test/dashboard").await;
    let bystander = asset(&pool, "postgres", None).await;
    route(&pool, &bystander, "pgadmin", "https://pgadmin.knobas.test/").await;

    let written = run_rule(&pool, "monitor_url_host").await;
    assert_eq!(written, 1, "one route host match, one proposal");

    let entries = tray(&pool).await;
    assert_eq!(
        pairs(&entries),
        [(proxy.clone(), watching.clone())].into_iter().collect(),
        "the asset that exposes the route is the one proposed"
    );
    let reason = between(&entries, &proxy, &watching)
        .unwrap()
        .link
        .reason
        .clone()
        .unwrap();
    assert!(
        reason.contains("kuma.knobas.test") && reason.contains("route"),
        "the reason names the host and the route that states it: {reason}"
    );
    assert!(between(&entries, &bystander, &watching).is_none());
}

/// Confirming a monitor suggestion draws the link; dismissing one is
/// remembered, and a later pass does not propose it back.
///
/// Both halves in one corpus so that the pass which must resurrect nothing is
/// the same pass that must leave the confirmed link alone.
#[tokio::test]
async fn a_monitor_suggestion_is_confirmed_or_dismissed_like_any_other() {
    let pool = scratch().await;
    let kept = monitor(&pool, "1", "knobas-gitea", Some("http://gitea:3000")).await;
    let refused = monitor(&pool, "2", "knobas-redis", Some("http://redis:6379")).await;
    let gitea = asset(&pool, "knobas-gitea", Some("gitea")).await;
    let redis = asset(&pool, "knobas-redis", Some("redis")).await;

    assert_eq!(run_rule(&pool, "monitor_url_host").await, 2);
    let entries = tray(&pool).await;
    let accept_me = between(&entries, &gitea, &kept).unwrap().link.clone();
    let dismiss_me = between(&entries, &redis, &refused).unwrap().link.clone();

    suggest::accept(&pool, accept_me.id)
        .await
        .unwrap()
        .expect("there was a proposal to accept");
    suggest::dismiss(&pool, dismiss_me.id)
        .await
        .unwrap()
        .expect("there was a proposal to dismiss");

    // The confirmed one is an ordinary link, on the asset's panel and on the
    // monitor's.
    let on_asset = link::entries_of(&pool, &EntityRef::parse(&gitea).unwrap())
        .await
        .unwrap();
    assert_eq!(
        on_asset
            .iter()
            .map(|e| (e.link.id, e.link.relation.clone()))
            .collect::<Vec<_>>(),
        vec![(accept_me.id, "monitored-by".to_owned())],
        "confirm draws the monitored-by link"
    );
    assert_eq!(
        link::entries_of(&pool, &EntityRef::parse(&kept).unwrap())
            .await
            .unwrap()
            .len(),
        1,
        "and the monitor is linked from its end too"
    );
    assert!(
        link::entries_of(&pool, &EntityRef::parse(&redis).unwrap())
            .await
            .unwrap()
            .is_empty(),
        "dismissing draws nothing"
    );

    // A second pass over the same mirror re-proposes neither.
    assert_eq!(
        run_rule(&pool, "monitor_url_host").await,
        0,
        "an answered pair is not proposed again"
    );
    assert!(
        tray(&pool).await.is_empty(),
        "the tray is empty: one pair is a link, the other is dismissed"
    );
}

// -- idempotence: the property most likely to be quietly broken -------------

/// Running detection twice proposes each suggestion once.
#[tokio::test]
async fn a_second_pass_over_an_unchanged_mirror_proposes_nothing() {
    let pool = scratch().await;
    item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    item(&pool, "commit", "c1", "Fix PAY-231", "").await;

    let first = suggest::detect(&pool).await.unwrap();
    assert!(first > 0, "the first pass has something to propose");
    let after_first = pairs(&tray(&pool).await);

    let second = suggest::detect(&pool).await.unwrap();

    assert_eq!(
        second, 0,
        "a second pass over the same mirror proposes nothing"
    );
    assert_eq!(
        pairs(&tray(&pool).await),
        after_first,
        "and the tray is unchanged"
    );
}

/// Re-running detection after a sync does not resurrect a dismissed proposal.
///
/// The sync is real in the way that matters: the branch row is written again,
/// exactly as an incremental run would rewrite it, so the pass that follows has
/// every reason to propose the same thing.
#[tokio::test]
async fn a_dismissed_suggestion_is_not_resurrected_by_a_later_pass() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;

    suggest::detect(&pool).await.unwrap();
    let proposal = between(&tray(&pool).await, &branch, &ticket)
        .expect("the branch proposes its ticket")
        .link
        .id;

    let dismissed = suggest::dismiss(&pool, proposal)
        .await
        .unwrap()
        .expect("the proposal was there to dismiss");
    assert_eq!(dismissed.id, proposal);

    // The mirror moves under it, the way a sync moves it.
    sqlx::query("update sync.item set synced_at = now(), title = title where entity_id = $1")
        .bind(&branch)
        .execute(&pool)
        .await
        .unwrap();

    let written = suggest::detect(&pool).await.unwrap();

    assert_eq!(written, 0, "a dismissal is remembered across a re-sync");
    assert!(
        between(&tray(&pool).await, &branch, &ticket).is_none(),
        "the same suggestion must never be proposed twice"
    );
}

/// A link the user removed is never proposed back (#41 story 9).
///
/// The tombstone `unlink` leaves and the tombstone `dismiss` leaves are the
/// same row in the same state: one mechanism, so removing a link means
/// something to the detector too.
#[tokio::test]
async fn a_link_the_user_unlinked_is_never_proposed_back() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;

    let drawn = link::create(
        &pool,
        &EntityRef::parse(&branch).unwrap(),
        &EntityRef::parse(&ticket).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
    link::unlink(&pool, drawn.id).await.unwrap();

    let written = suggest::detect(&pool).await.unwrap();

    assert_eq!(written, 0, "detection may not undo an unlink");
    assert!(between(&tray(&pool).await, &branch, &ticket).is_none());
}

/// The suppression is undirected: removing `A -> B` also suppresses `B -> A`.
///
/// The unique index was directed until migration `0011` made it agree (#70),
/// but the *fact* "these two are connected" never was, and a detector that
/// re-proposed the mirror image of a link the user
/// removed would be exactly the silent resurrection the withdrawal memory
/// exists to prevent.
#[tokio::test]
async fn the_suppression_does_not_care_which_way_round_the_removed_link_was() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;

    // Drawn ticket -> branch; detection proposes branch -> ticket.
    let drawn = link::create(
        &pool,
        &EntityRef::parse(&ticket).unwrap(),
        &EntityRef::parse(&branch).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
    link::unlink(&pool, drawn.id).await.unwrap();

    suggest::detect(&pool).await.unwrap();

    assert!(
        between(&tray(&pool).await, &branch, &ticket).is_none(),
        "the same pair the other way round is the same pair"
    );
}

/// A pair that is already linked produces no suggestion (#41 story 17).
#[tokio::test]
async fn a_pair_that_is_already_linked_produces_no_suggestion() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    link::create(
        &pool,
        &EntityRef::parse(&branch).unwrap(),
        &EntityRef::parse(&ticket).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();

    let written = suggest::detect(&pool).await.unwrap();

    assert_eq!(written, 0, "the tray is not filled with things I have done");
    assert!(tray(&pool).await.is_empty());
}

/// Two rules that see the same connection propose it once, in one pass.
///
/// The suppression reads the snapshot its statement started from, so a pass
/// where a branch *and* a commit both name PAY-231 in the same source could
/// write two rows for one pair if the driver did not also de-duplicate within
/// the statement and between rules.
#[tokio::test]
async fn one_connection_seen_by_two_rules_is_proposed_once() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    // A page that both names the key and reads like the ticket: the exact-key
    // rule and the similarity rule are both entitled to propose this pair.
    let page = item(
        &pool,
        "page",
        "ENG:Storm",
        "Payout retry storm floods the ledger",
        "PAY-231: the payout retry storm floods the ledger with duplicate transfers.",
    )
    .await;
    sqlx::query("update sync.item set body_text = $2 where entity_id = $1")
        .bind(&ticket)
        .bind("The payout retry storm floods the ledger with duplicate transfers.")
        .execute(&pool)
        .await
        .unwrap();

    suggest::detect(&pool).await.unwrap();

    let entries = tray(&pool).await;
    let touching: Vec<&suggest::SuggestionEntry> = entries
        .iter()
        .filter(|e| {
            [&e.link.from_id, &e.link.to_id].contains(&&page)
                && [&e.link.from_id, &e.link.to_id].contains(&&ticket)
        })
        .collect();
    assert_eq!(touching.len(), 1, "one connection, one proposal");
    assert_eq!(
        touching[0].link.rule.as_deref(),
        Some("page_text_key"),
        "the specific evidence wins the pair, so the reason is the better one"
    );
}

/// Detection never writes a confirmed link, whatever it proposes.
#[tokio::test]
async fn detection_never_writes_a_confirmed_link() {
    let pool = scratch().await;
    item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    item(&pool, "commit", "c1", "Fix PAY-231", "").await;
    item(&pool, "page", "ENG:Storm", "Runbook", "See PAY-231.").await;

    let written = suggest::detect(&pool).await.unwrap();
    assert!(written > 0);

    let (confirmed,): (i64,) =
        sqlx::query_as("select count(*) from knobas.link where confirmed_at is not null")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        confirmed, 0,
        "nothing enters the graph without the user's say-so"
    );
    let (proposals,): (i64,) = sqlx::query_as("select count(*) from knobas.proposed_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(proposals, written as i64);
}

// -- what happens when the user answers -------------------------------------

/// An accepted suggestion is an ordinary link, and appears in the links panel.
#[tokio::test]
async fn an_accepted_suggestion_is_indistinguishable_from_a_hand_drawn_link() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    // A hand-drawn link on the same branch, to compare against.
    let other = item(&pool, "ticket", "PAY-999", "Something else", "").await;
    let hand = link::create(
        &pool,
        &EntityRef::parse(&branch).unwrap(),
        &EntityRef::parse(&other).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();

    suggest::detect(&pool).await.unwrap();
    let proposal = between(&tray(&pool).await, &branch, &ticket)
        .expect("the branch proposes its ticket")
        .link
        .clone();

    let accepted = suggest::accept(&pool, proposal.id)
        .await
        .unwrap()
        .expect("there was a proposal to accept");

    assert_eq!(accepted.id, proposal.id, "the row does not move");
    assert!(accepted.confirmed_at.is_some());

    // The panel, from both ends -- the same read every other link arrives
    // through.
    let from_branch = link::entries_of(&pool, &EntityRef::parse(&branch).unwrap())
        .await
        .unwrap();
    let ids: Vec<uuid::Uuid> = from_branch.iter().map(|e| e.link.id).collect();
    assert!(
        ids.contains(&proposal.id),
        "an accepted suggestion is a link"
    );
    assert!(ids.contains(&hand.id), "beside the hand-drawn one");
    let on_ticket = link::entries_of(&pool, &EntityRef::parse(&ticket).unwrap())
        .await
        .unwrap();
    assert_eq!(on_ticket.len(), 1, "and it is on the other end too");

    // Ordinary in the way that matters: it is unlinked by the panel's own
    // action, not by a suggestion-shaped one.
    link::unlink(&pool, proposal.id)
        .await
        .unwrap()
        .expect("an accepted suggestion unlinks like any link");
    assert!(
        link::entries_of(&pool, &EntityRef::parse(&ticket).unwrap())
            .await
            .unwrap()
            .is_empty()
    );

    // ...and the reason survives, because a link that can still say why knobas
    // thought so is more useful than one that cannot.
    assert_eq!(
        accepted.reason.as_deref(),
        Some("the branch name contains PAY-231")
    );
}

/// The links panel cannot show a proposal, and the tray cannot show a link --
/// asserted in both directions on one corpus.
#[tokio::test]
async fn the_panel_shows_no_proposal_and_the_tray_shows_no_link() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    let page = item(&pool, "page", "ENG:Storm", "Runbook", "").await;
    let drawn = link::create(
        &pool,
        &EntityRef::parse(&page).unwrap(),
        &EntityRef::parse(&ticket).unwrap(),
        "documents",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();

    suggest::detect(&pool).await.unwrap();

    let panel = link::entries_of(&pool, &EntityRef::parse(&ticket).unwrap())
        .await
        .unwrap();
    assert_eq!(
        panel.iter().map(|e| e.link.id).collect::<Vec<_>>(),
        vec![drawn.id],
        "the panel holds the drawn link and nothing knobas merely proposed"
    );
    assert!(
        panel.iter().all(|e| e.link.confirmed_at.is_some()),
        "a links panel showing an unconfirmed guess is a correctness bug"
    );

    let entries = tray(&pool).await;
    assert!(
        entries.iter().all(|e| e.link.confirmed_at.is_none()),
        "the tray holds proposals only"
    );
    assert!(
        !entries.iter().any(|e| e.link.id == drawn.id),
        "a link the user drew is not a suggestion"
    );
    assert!(
        between(&entries, &branch, &ticket).is_some(),
        "and the proposal is there"
    );
}

/// Accepting and dismissing are idempotent, and each says whether it did
/// anything -- which is what lets a caller write one activity line per
/// mutation.
#[tokio::test]
async fn accept_and_dismiss_report_whether_they_changed_anything() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    item(&pool, "commit", "c1", "Fix PAY-231", "").await;
    suggest::detect(&pool).await.unwrap();

    let entries = tray(&pool).await;
    let accepted_id = entries[0].link.id;
    let dismissed_id = entries[1].link.id;

    assert!(suggest::accept(&pool, accepted_id).await.unwrap().is_some());
    assert!(
        suggest::accept(&pool, accepted_id).await.unwrap().is_none(),
        "accepting twice accepts once"
    );
    assert!(
        suggest::dismiss(&pool, accepted_id)
            .await
            .unwrap()
            .is_none(),
        "a confirmed link is never tombstoned through the tray's door"
    );
    assert!(
        link::entries_of(&pool, &EntityRef::parse(&ticket).unwrap())
            .await
            .unwrap()
            .iter()
            .any(|e| e.link.id == accepted_id),
        "and it is still in the panel"
    );

    assert!(
        suggest::dismiss(&pool, dismissed_id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        suggest::dismiss(&pool, dismissed_id)
            .await
            .unwrap()
            .is_none(),
        "dismissing twice dismisses once"
    );
    assert!(
        suggest::accept(&pool, dismissed_id)
            .await
            .unwrap()
            .is_none(),
        "a dismissal is not undone by pressing the other button"
    );

    // An id nothing carries is a different answer from "nothing to do".
    let unknown = Uuid::new_v4();
    assert!(matches!(
        suggest::accept(&pool, unknown).await,
        Err(CoreError::LinkNotFound(_))
    ));
    assert!(matches!(
        suggest::dismiss(&pool, unknown).await,
        Err(CoreError::LinkNotFound(_))
    ));
}

/// Drawing by hand the link knobas had already proposed accepts it, rather
/// than being refused as a duplicate.
///
/// The unique index spans proposals and links alike, so without
/// [`suggest::resolve_edge`] the user is told "already linked" about a pair
/// whose links panel is empty.
#[tokio::test]
async fn drawing_a_proposed_link_by_hand_accepts_the_proposal() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    suggest::detect(&pool).await.unwrap();
    let proposal = between(&tray(&pool).await, &branch, &ticket)
        .expect("a proposal")
        .link
        .id;

    let from = EntityRef::parse(&branch).unwrap();
    let to = EntityRef::parse(&ticket).unwrap();
    let refused = link::create(&pool, &from, &to, "related", Origin::Manual, None, "user").await;
    assert!(matches!(refused, Err(CoreError::Duplicate)));

    let mut conn = pool.acquire().await.unwrap();
    let promoted = match suggest::resolve_edge(&mut conn, &from, &to, "related")
        .await
        .unwrap()
    {
        suggest::Edge::Promoted(row) => row,
        other => panic!("the blocker was a proposal, so it is accepted instead: {other:?}"),
    };
    assert_eq!(promoted.id, proposal);
    assert!(promoted.confirmed_at.is_some());
    assert!(
        matches!(
            suggest::resolve_edge(&mut conn, &from, &to, "related")
                .await
                .unwrap(),
            suggest::Edge::Open
        ),
        "a genuine duplicate is still a duplicate"
    );
}

/// Drawing the **reverse** of a live proposal withdraws it and lets the user's
/// link through -- the second symptom recorded on #70.
///
/// Before `0011` this pair was the hole where the directed index and the
/// direction-exact promotion cancelled out: the reversed hand-drawn link
/// bypassed both, so it simply *succeeded*, and left a stale proposal sitting
/// in the tray beside a confirmed link for the same pair.
///
/// **Withdrawn, not confirmed**, and the direction is why. Confirming would
/// store the opposite of what the user drew; story 7's inverse labels would then
/// render it faithfully back at them as the opposite claim. The tombstone is the
/// same one [`suggest::dismiss`] writes, so detection's undirected suppression
/// will not propose it straight back either.
#[tokio::test]
async fn drawing_the_reverse_of_a_proposal_withdraws_it_and_keeps_the_users_direction() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    suggest::detect(&pool).await.unwrap();
    let proposal = between(&tray(&pool).await, &branch, &ticket)
        .expect("a proposal")
        .link
        .id;

    // The proposal runs branch -> ticket; the user draws ticket -> branch.
    let from = EntityRef::parse(&ticket).unwrap();
    let to = EntityRef::parse(&branch).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    let withdrawn = match suggest::resolve_edge(&mut conn, &from, &to, "related")
        .await
        .unwrap()
    {
        suggest::Edge::Superseded(row) => row,
        other => panic!("the reversed proposal must be superseded, not {other:?}"),
    };
    assert_eq!(withdrawn.id, proposal);
    assert!(
        withdrawn.confirmed_at.is_none(),
        "it must not be confirmed on the way out -- that would store the opposite \
         of what the user drew"
    );
    // A tombstone, not a delete: `LinkRow` does not carry `deleted_at`, so the
    // column is read where it lives.
    let tombstoned: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select deleted_at from knobas.link where id = $1")
            .bind(proposal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        tombstoned.is_some(),
        "withdrawn is a tombstone, never a delete"
    );

    // Out of the tray, and the pair is free for the link the user is drawing.
    assert!(between(&tray(&pool).await, &branch, &ticket).is_none());
    let drawn = link::create(&pool, &from, &to, "related", Origin::Manual, None, "user")
        .await
        .expect("the withdrawal makes room for the user's own link");
    assert_eq!(
        drawn.from_id, ticket,
        "the user's direction is what is stored"
    );
    assert_eq!(drawn.to_id, branch);

    // And the tombstone is the withdrawal memory: detection does not bring it
    // back, in either direction.
    suggest::detect(&pool).await.unwrap();
    assert!(between(&tray(&pool).await, &branch, &ticket).is_none());
    assert!(between(&tray(&pool).await, &ticket, &branch).is_none());
}

/// A **confirmed** link in the way is nobody's to supersede: `Open`, and the
/// caller's insert is what reports the conflict.
///
/// `resolve_edge` may only ever touch a proposal. Widening it to a confirmed row
/// would make hand-drawing a link a way to silently delete one.
#[tokio::test]
async fn a_confirmed_link_is_never_superseded_from_either_direction() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-999", "Unrelated", "").await;
    let branch = item(&pool, "branch", "b9", "feature/none", "").await;
    let from = EntityRef::parse(&ticket).unwrap();
    let to = EntityRef::parse(&branch).unwrap();
    link::create(&pool, &from, &to, "related", Origin::Manual, None, "user")
        .await
        .unwrap();

    let mut conn = pool.acquire().await.unwrap();
    for (a, b) in [(&from, &to), (&to, &from)] {
        assert!(
            matches!(
                suggest::resolve_edge(&mut conn, a, b, "related")
                    .await
                    .unwrap(),
                suggest::Edge::Open
            ),
            "{a} -> {b} must leave the confirmed link alone"
        );
    }
    assert_eq!(
        link::entries_of(&pool, &from).await.unwrap().len(),
        1,
        "the link is untouched"
    );
}

// -- the tray is a query -----------------------------------------------------

/// The tray is scoped to the room it is drawn in, and a proposal belongs to a
/// room if **either** end does.
#[tokio::test]
async fn the_tray_shows_the_proposals_of_the_room_it_is_in() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = from(
        &pool,
        "gitea",
        "branch",
        "tidewater/payout#b1",
        "feature/PAY-231-retry",
        "",
        serde_json::json!({}),
    )
    .await;
    // A connection that touches neither room.
    let other_ticket = from(
        &pool,
        "jira-eu",
        "ticket",
        "EU-1",
        "Something else",
        "",
        serde_json::json!({}),
    )
    .await;
    let other_page = from(
        &pool,
        "jira-eu",
        "page",
        "EU:Notes",
        "Notes",
        "See EU-1.",
        serde_json::json!({}),
    )
    .await;

    suggest::detect(&pool).await.unwrap();

    for room in ["jira", "gitea"] {
        let entries = suggest::proposals(&pool, &[room.to_owned()], None, 50)
            .await
            .unwrap();
        assert_eq!(
            pairs(&entries),
            [(branch.clone(), ticket.clone())].into_iter().collect(),
            "{room} sees the proposal that touches it"
        );
        assert_eq!(
            suggest::proposal_count(&pool, &[room.to_owned()], None)
                .await
                .unwrap(),
            1,
            "and the count says so before the rows are drawn"
        );
    }

    let eu = suggest::proposals(&pool, &["jira-eu".to_owned()], None, 50)
        .await
        .unwrap();
    assert_eq!(
        pairs(&eu),
        [(other_page.clone(), other_ticket.clone())]
            .into_iter()
            .collect()
    );

    // An empty scope is every source, not no source.
    assert_eq!(tray(&pool).await.len(), 2);
    assert_eq!(suggest::proposal_count(&pool, &[], None).await.unwrap(), 2);
}

/// The membership scope #47 adds: a stored context's room passes its member
/// ids, and only proposals touching one of them come back. `Some(&[])` -- a
/// context with no members -- is an honestly empty tray, **not** an unscoped
/// one, because the difference between "no filter" and "a filter nothing
/// passes" is exactly the bug `= any('{}')` conventions exist to keep visible.
#[tokio::test]
async fn the_tray_scopes_by_membership_when_a_context_room_asks() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = from(
        &pool,
        "gitea",
        "branch",
        "tidewater/payout#b1",
        "feature/PAY-231-retry",
        "",
        serde_json::json!({}),
    )
    .await;
    let other_ticket = from(
        &pool,
        "jira-eu",
        "ticket",
        "EU-1",
        "Something else",
        "",
        serde_json::json!({}),
    )
    .await;
    from(
        &pool,
        "jira-eu",
        "page",
        "EU:Notes",
        "Notes",
        "See EU-1.",
        serde_json::json!({}),
    )
    .await;
    suggest::detect(&pool).await.unwrap();

    // Scoped to the ticket: the branch proposal touches it, EU's does not.
    let scoped = suggest::proposals(&pool, &[], Some(std::slice::from_ref(&ticket)), 50)
        .await
        .unwrap();
    assert_eq!(
        pairs(&scoped),
        [(branch.clone(), ticket.clone())].into_iter().collect()
    );
    assert_eq!(
        suggest::proposal_count(&pool, &[], Some(std::slice::from_ref(&ticket)))
            .await
            .unwrap(),
        1
    );

    // Scoped to an unrelated member: nothing, though proposals exist.
    let elsewhere = suggest::proposals(&pool, &[], Some(std::slice::from_ref(&branch)), 50)
        .await
        .unwrap();
    assert_eq!(
        pairs(&elsewhere),
        pairs(&scoped),
        "the branch end scopes too"
    );
    let none = suggest::proposals(&pool, &[], Some(&["note:unrelated".to_owned()]), 50)
        .await
        .unwrap();
    assert!(none.is_empty());

    // And the empty membership is empty, not everything.
    assert!(
        suggest::proposals(&pool, &[], Some(&[]), 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        suggest::proposal_count(&pool, &[], Some(&[]))
            .await
            .unwrap(),
        0,
        "{other_ticket} and friends must not leak into a memberless context"
    );
}

/// A proposal whose end the source withdrew is marked, not dropped.
///
/// The tray hydrates through `knobas.entity` for the reason the links panel
/// does: §5a keeps the entity so a link never dangles.
#[tokio::test]
async fn a_withdrawn_end_still_resolves_in_the_tray() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;
    suggest::detect(&pool).await.unwrap();

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&ticket)
        .execute(&pool)
        .await
        .unwrap();

    let entries = tray(&pool).await;
    let found = between(&entries, &branch, &ticket).expect("the proposal survives the tombstone");
    assert_eq!(found.to.title, "Payout retry storm");
    assert!(
        found.to.deleted_at.is_some(),
        "the withdrawal is a fact the reader is shown, not a filter"
    );
    assert!(found.from.deleted_at.is_none());
}

/// A proposal reaching an entity the mirror never had still resolves.
///
/// knobas' own kinds -- notes, contexts -- have a `knobas.entity` row and no
/// `sync.item`, so the tray's source join has to be a left join or they vanish.
#[tokio::test]
async fn a_proposal_touching_a_knobas_owned_entity_is_still_readable() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let note = entity_only(&pool, "note", "note", "7f2c").await;
    // No rule reaches a note -- they are not in the mirror -- so the proposal
    // is written here exactly the way the driver writes one.
    sqlx::query(
        "insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 confirmed_at, rule, rule_class, reason)
         values ($1, $2, 'related', 'suggested', 'knobas',
                 null, 'branch_name_key', 'exact_key', 'the note names PAY-231')",
    )
    .bind(&note)
    .bind(&ticket)
    .execute(&pool)
    .await
    .unwrap();

    let entries = tray(&pool).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].from.entity_id, note);
    assert_eq!(entries[0].from.kind, "note");
    assert_eq!(
        suggest::proposal_count(&pool, &["jira".to_owned()], None)
            .await
            .unwrap(),
        1,
        "the ticket's room holds it, through the end that has a source"
    );
}

/// A proposal without a reason is not storable, because it is not shippable.
#[tokio::test]
async fn the_database_refuses_a_proposal_that_cannot_say_why() {
    let pool = scratch().await;
    let ticket = item(&pool, "ticket", "PAY-231", "Payout retry storm", "").await;
    let branch = item(&pool, "branch", "b1", "feature/PAY-231-retry", "").await;

    let refused = sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
         values ($1, $2, 'related', 'suggested', 'knobas', null)",
    )
    .bind(&branch)
    .bind(&ticket)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "an unconfirmed row must carry a rule, a class and a reason"
    );
}
