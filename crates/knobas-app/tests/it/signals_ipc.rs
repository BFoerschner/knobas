//! Signals on tickets (#815) through the commands that read them: the cards
//! `mini_board` draws, and `has:` through the same command's `query` and
//! through `search`. Each signal is seeded on its own ticket, over the
//! declarations the compiled-in adapters make (`Registry::for_process`), and the
//! read and `has:` are held to one answer.
//!
//! This file's database is its own (#572); each test uses a project key
//! nothing else holds, and scopes every query by it.

use knobas_app::commands::entity::mini_board_inner;
use knobas_app::commands::search::search_inner;
use knobas_app::sources::Registry;
use knobas_core::mini_board::MiniBoardCard;
use knobas_search::{SearchFilters, SearchQuery};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, name, config) in [
        ("jira", "Jira", r#"{"username": "mara.lindqvist"}"#),
        // A user the server and Rust fold differently (#1975): `İlker`. Only
        // `a_review_asked_of_a_dotted_capital_i_user_is_on_the_card_and_in_has`
        // asks them for a review.
        ("gitea", "Gitea", r#"{"username": "İlker"}"#),
        ("teamcity", "TeamCity", "{}"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $1, $2, 'http://x', 'Pat', $3::jsonb)
             on conflict (id) do update set kind = excluded.kind, enabled = true,
                                             config = excluded.config",
        )
        .bind(id)
        .bind(name)
        .bind(config)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// A project key nothing else in this database holds: letters only.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect();
    format!("S{letters}")
}

/// One mirrored item: its entity row and its record.
async fn item(
    pool: &sqlx::PgPool,
    id: &str,
    source: &str,
    kind: &str,
    payload: serde_json::Value,
    minutes_ago: i32,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $1)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1, $2, $3, $1, $1, $4, now() - make_interval(mins => $5))",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

/// A Jira issue in project `p`, as the Jira adapter stores it.
async fn ticket(pool: &sqlx::PgPool, key: &str) -> String {
    let project = key.split('-').next().unwrap();
    let id = format!("jira:{key}");
    item(
        pool,
        &id,
        "jira",
        "ticket",
        serde_json::json!({
            "key": key,
            "fields": {
                "summary": key, "status": { "name": "To Do" },
                "project": { "key": project, "name": "Payments" },
            },
            "sprints": [],
        }),
        0,
    )
    .await;
    id
}

/// A Gitea pull request, `open` or merged, with these requested reviewers.
async fn pull_request(pool: &sqlx::PgPool, id: &str, state: &str, reviewers: &[&str]) {
    item(
        pool,
        id,
        "gitea",
        "pr",
        serde_json::json!({
            "state": if state == "merged" { "closed" } else { state },
            "merged": state == "merged",
            "requested_reviewers": reviewers.iter().map(|r| serde_json::json!({ "login": r }))
                .collect::<Vec<_>>(),
        }),
        0,
    )
    .await;
}

/// A finished TeamCity build of configuration `config`.
async fn build(pool: &sqlx::PgPool, id: &str, config: &str, status: &str, minutes_ago: i32) {
    item(
        pool,
        id,
        "teamcity",
        "build",
        serde_json::json!({
            "state": "finished", "status": status,
            "buildType": { "id": config, "name": config, "projectId": "P" },
        }),
        minutes_ago,
    )
    .await;
}

/// A link, confirmed or only proposed.
async fn link(pool: &sqlx::PgPool, from: &str, to: &str, relation: &str, confirmed: bool) {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at,
                                  rule, rule_class, reason)
         values ($1, $2, $3, case when $4 then 'manual' else 'suggested' end, 'user',
                 case when $4 then now() end,
                 case when not $4 then 'key_mention' end, case when not $4 then 'exact_key' end,
                 case when not $4 then 'the key is in the title' end)",
    )
    .bind(from)
    .bind(to)
    .bind(relation)
    .bind(confirmed)
    .execute(pool)
    .await
    .unwrap();
}

async fn cards(pool: &sqlx::PgPool, query: &str) -> Vec<MiniBoardCard> {
    let board = mini_board_inner(
        pool,
        &knobas_app::sources::Registry::for_process(),
        None,
        None,
        None,
        None,
        Some(query),
    )
    .await
    .unwrap();
    let mut cards: Vec<MiniBoardCard> = board.columns.into_iter().flat_map(|c| c.cards).collect();
    cards.sort_by(|a, b| a.key.cmp(&b.key));
    cards
}

/// The signals a card carries, as `kind text tone` lines.
fn drawn(card: &MiniBoardCard) -> Vec<String> {
    card.signals
        .iter()
        .map(|s| {
            format!(
                "{} {} {}",
                s.kind.as_str(),
                s.text,
                serde_json::to_value(s.tone).unwrap().as_str().unwrap()
            )
        })
        .collect()
}

/// The keys `search` answers for `raw`, sorted.
async fn searched(pool: &sqlx::PgPool, raw: &str) -> Vec<String> {
    let response = search_inner(
        pool,
        &Registry::for_process(),
        SearchQuery {
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw}: {:?}",
        response.interpreted.unknown_tokens
    );
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.replace("jira:", "")))
        .collect();
    out.sort();
    out
}

/// Each signal seeded on a ticket of its own, plus one ticket with none: the
/// card says it in its words and tone, and `has:` through `mini_board` and
/// through `search` admits exactly the tickets whose cards show it.
#[tokio::test]
async fn each_signal_is_drawn_on_its_card_and_has_admits_exactly_those_tickets() {
    let pool = pool().await;
    let p = project();
    let key = |n: u32| format!("{p}-{n}");
    let (red, blocked, open, review, quiet) = (
        ticket(&pool, &key(1)).await,
        ticket(&pool, &key(2)).await,
        ticket(&pool, &key(3)).await,
        ticket(&pool, &key(4)).await,
        ticket(&pool, &key(5)).await,
    );

    // A red build reached through the ticket's merged pull request: one hop.
    let merged = format!("gitea:tide/{p}#7");
    pull_request(&pool, &merged, "merged", &[]).await;
    link(&pool, &red, &merged, "related", true).await;
    let failed = format!("teamcity:{p}-b1");
    build(&pool, &failed, &format!("{p}-cfg"), "FAILURE", 5).await;
    link(&pool, &merged, &failed, "related", true).await;

    link(&pool, &red, &blocked, "blocks", true).await;

    let pr = format!("gitea:tide/{p}#142");
    pull_request(&pool, &pr, "open", &["jonas.becker"]).await;
    link(&pool, &open, &pr, "related", true).await;

    let asked = format!("gitea:tide/{p}#150");
    pull_request(&pool, &asked, "open", &["Mara.Lindqvist"]).await;
    link(&pool, &asked, &review, "related", true).await;

    let board = cards(&pool, &format!("project:{p}")).await;
    let lines: Vec<(String, Vec<String>)> =
        board.iter().map(|c| (c.key.clone(), drawn(c))).collect();
    assert_eq!(
        lines,
        vec![
            (key(1), vec!["failing-build build failed err".to_owned()]),
            (key(2), vec![format!("blocker blocked by {} err", key(1))]),
            (key(3), vec!["open-pr PR #142 open info".to_owned()]),
            (
                key(4),
                vec![
                    "open-pr PR #150 open info".to_owned(),
                    "review-for-me review asked of you warn".to_owned(),
                ]
            ),
            (key(5), vec![]),
        ]
    );
    let _ = quiet;

    for (has, expected) in [
        ("failing-build", vec![key(1)]),
        ("blocker", vec![key(2)]),
        ("open-pr", vec![key(3), key(4)]),
        ("review-for-me", vec![key(4)]),
    ] {
        let raw = format!("project:{p} has:{has}");
        let on_board: Vec<String> = cards(&pool, &raw)
            .await
            .into_iter()
            .map(|c| c.key)
            .collect();
        assert_eq!(on_board, expected, "{raw} on the board");
        assert_eq!(searched(&pool, &raw).await, expected, "{raw} in search");
        let shown: Vec<String> = board
            .iter()
            .filter(|c| c.signals.iter().any(|s| s.kind.as_str() == has))
            .map(|c| c.key.clone())
            .collect();
        assert_eq!(shown, expected, "the cards showing {has}");

        let negated = format!("project:{p} -has:{has}");
        let rest: Vec<String> = cards(&pool, &negated)
            .await
            .into_iter()
            .map(|c| c.key)
            .collect();
        assert_eq!(rest.len(), 5 - expected.len(), "{negated}");
        assert!(rest.iter().all(|k| !expected.contains(k)), "{negated}");
    }
}

/// A red build linked to the ticket itself is one hop, and a ticket linked
/// to that ticket reaches it in two: `has:failing-build` admits both, as
/// their cards say (#939, whose `has:` starts from the red builds and walks
/// back). A ticket three hops away carries none.
#[tokio::test]
async fn has_failing_build_finds_a_red_build_one_and_two_hops_back() {
    let pool = pool().await;
    let p = project();
    let key = |n: u32| format!("{p}-{n}");
    let (direct, beside, far) = (
        ticket(&pool, &key(1)).await,
        ticket(&pool, &key(2)).await,
        ticket(&pool, &key(3)).await,
    );
    let red = format!("teamcity:{p}-b1");
    build(&pool, &red, &format!("{p}-cfg"), "FAILURE", 5).await;
    link(&pool, &red, &direct, "related", true).await;
    link(&pool, &beside, &direct, "related", true).await;
    link(&pool, &far, &beside, "related", true).await;

    let expected = vec![key(1), key(2)];
    let raw = format!("project:{p} has:failing-build");
    assert_eq!(searched(&pool, &raw).await, expected, "{raw}");
    let shown: Vec<String> = cards(&pool, &format!("project:{p}"))
        .await
        .into_iter()
        .filter(|c| c.signals.iter().any(|s| s.kind.as_str() == "failing-build"))
        .map(|c| c.key)
        .collect();
    assert_eq!(shown, expected);
}

/// What does **not** make a signal: a newer green run of the same
/// configuration, a proposed link, a review asked of somebody else, a
/// pull request that is merged, and a `blocks` link read from its other end.
#[tokio::test]
async fn a_resolved_or_unconfirmed_fact_makes_no_signal() {
    let pool = pool().await;
    let p = project();
    let key = |n: u32| format!("{p}-{n}");
    let (green, guessed, merged, blocker) = (
        ticket(&pool, &key(1)).await,
        ticket(&pool, &key(2)).await,
        ticket(&pool, &key(3)).await,
        ticket(&pool, &key(4)).await,
    );
    let cfg = format!("{p}-cfg");
    let (old, new) = (format!("teamcity:{p}-b1"), format!("teamcity:{p}-b2"));
    build(&pool, &old, &cfg, "FAILURE", 30).await;
    build(&pool, &new, &cfg, "SUCCESS", 5).await;
    link(&pool, &green, &old, "related", true).await;
    link(&pool, &green, &new, "related", true).await;

    let pr = format!("gitea:tide/{p}#9");
    pull_request(&pool, &pr, "open", &["mara.lindqvist"]).await;
    link(&pool, &guessed, &pr, "related", false).await;

    let done = format!("gitea:tide/{p}#10");
    pull_request(&pool, &done, "merged", &["mara.lindqvist"]).await;
    link(&pool, &merged, &done, "related", true).await;

    // The blocker's own card: it blocks, it is not blocked.
    let other = ticket(&pool, &key(5)).await;
    link(&pool, &blocker, &other, "blocks", true).await;

    for card in cards(&pool, &format!("project:{p}")).await {
        if card.key == key(5) {
            assert_eq!(
                drawn(&card),
                vec![format!("blocker blocked by {} err", key(4))]
            );
        } else {
            assert_eq!(drawn(&card), Vec::<String>::new(), "{}", card.key);
        }
    }
    for has in ["failing-build", "open-pr", "review-for-me"] {
        assert_eq!(
            searched(&pool, &format!("project:{p} has:{has}")).await,
            Vec::<String>::new(),
            "has:{has}"
        );
    }
}

/// A room is never walked through (ADR-0008): a ticket in the same room as a
/// red build is not two hops from it. And a withdrawn blocker blocks nothing.
/// The walk tests both by a probe of the entity (#1163).
#[tokio::test]
async fn a_room_shares_no_build_and_a_withdrawn_blocker_blocks_nothing() {
    let pool = pool().await;
    let p = project();
    let key = |n: u32| format!("{p}-{n}");
    let (red, roommate, blocked, gone) = (
        ticket(&pool, &key(1)).await,
        ticket(&pool, &key(2)).await,
        ticket(&pool, &key(3)).await,
        ticket(&pool, &key(4)).await,
    );
    let failed = format!("teamcity:{p}-b1");
    build(&pool, &failed, &format!("{p}-cfg"), "FAILURE", 5).await;
    link(&pool, &red, &failed, "related", true).await;
    let room = knobas_core::context::create_adhoc(&pool, &format!("{p} room"))
        .await
        .unwrap();
    link(&pool, &room.id, &roommate, "related", true).await;
    link(&pool, &room.id, &failed, "related", true).await;

    link(&pool, &gone, &blocked, "blocks", true).await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();

    let lines: Vec<(String, Vec<String>)> = cards(&pool, &format!("project:{p}"))
        .await
        .iter()
        .map(|c| (c.key.clone(), drawn(c)))
        .collect();
    assert_eq!(
        lines,
        vec![
            (key(1), vec!["failing-build build failed err".to_owned()]),
            (key(2), vec![]),
            (key(3), vec![]),
        ]
    );
    for (has, expected) in [("failing-build", vec![key(1)]), ("blocker", vec![])] {
        let raw = format!("project:{p} has:{has}");
        assert_eq!(searched(&pool, &raw).await, expected, "{raw}");
    }
}

/// A value `has:` does not know is reported, never matched as a word.
#[tokio::test]
async fn an_unknown_has_value_is_reported() {
    let pool = pool().await;
    let response = search_inner(
        &pool,
        &Registry::for_process(),
        SearchQuery {
            raw: "has:dragons".to_owned(),
            limit: 5,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
    assert_eq!(response.interpreted.unknown_tokens, vec!["has:dragons"]);
}

/// `filter_suggestions` offers `has:`'s four values, which every
/// installation has.
#[tokio::test]
async fn the_filter_suggestions_offer_the_four_signals() {
    let pool = pool().await;
    let lists = knobas_app::commands::search::filter_suggestions_inner(
        &pool,
        &Registry::for_process(),
        "",
        false,
    )
    .await
    .unwrap();
    let offered: Vec<&str> = lists
        .fields
        .iter()
        .filter(|f| f.key == "has")
        .map(|f| f.value.as_str())
        .collect();
    assert_eq!(
        offered,
        ["failing-build", "blocker", "open-pr", "review-for-me"]
    );
}

/// A card carries its labels (#813) in the record's order, the first being
/// what the *Label* lanes split by; a ticket with none carries none.
#[tokio::test]
async fn a_card_carries_its_labels_in_the_records_order() {
    let pool = pool().await;
    let p = project();
    let labelled = format!("{p}-1");
    let id = ticket(&pool, &labelled).await;
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{fields,labels}', '[\"sepa\", \"api\"]')
          where entity_id = $1",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    ticket(&pool, &format!("{p}-2")).await;
    let labels: Vec<(String, Vec<String>)> = cards(&pool, &format!("project:{p}"))
        .await
        .into_iter()
        .map(|c| (c.key, c.labels))
        .collect();
    assert_eq!(
        labels,
        vec![
            (labelled, vec!["sepa".to_owned(), "api".to_owned()]),
            (format!("{p}-2"), vec![]),
        ]
    );
}

/// A review asked of `İlker`, configured as `İlker` (#1975): Rust folds the
/// name to `i̇lker` and the server to `ilker`, so both the card and `has:`
/// fold it with the server's `lower()`, as they fold the reviewer.
#[tokio::test]
async fn a_review_asked_of_a_dotted_capital_i_user_is_on_the_card_and_in_has() {
    let pool = pool().await;
    let p = project();
    let asked = ticket(&pool, &format!("{p}-1")).await;
    ticket(&pool, &format!("{p}-2")).await;
    let pr = format!("gitea:tide/{p}#9");
    pull_request(&pool, &pr, "open", &["İlker"]).await;
    link(&pool, &pr, &asked, "related", true).await;

    let expected = vec![format!("{p}-1")];
    let raw = format!("project:{p} has:review-for-me");
    assert_eq!(searched(&pool, &raw).await, expected, "{raw}");
    let shown: Vec<String> = cards(&pool, &format!("project:{p}"))
        .await
        .into_iter()
        .filter(|c| c.signals.iter().any(|s| s.kind.as_str() == "review-for-me"))
        .map(|c| c.key)
        .collect();
    assert_eq!(shown, expected, "the cards showing review-for-me");
}
