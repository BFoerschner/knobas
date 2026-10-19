//! The suggestion commands, over the real Tidewater corpus.
//!
//! `knobas-core`'s own battery proves each rule against a corpus it built by
//! hand. What that cannot prove is that the rules find anything in the dataset
//! knobas actually ships: a rule keyed on a `body_text` no adapter fills, or on
//! a payload path no source writes, passes every unit test and proposes nothing
//! on a real machine. So this loads the demo corpus through
//! `demo_load_inner` -- the same path *Load demo data* takes -- and asserts what
//! the tray then holds.
//!
//! A database of its own, for the reason `knobas_core`'s battery gives: a
//! detection pass reads the whole mirror, so it would propose into every other
//! test's corpus in the shared one.

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::{
    accept_suggestion_inner, create_link_inner, detect_suggestions_inner, dismiss_suggestion_inner,
    get_entity_inner, proposal_evidence_inner, room_suggestions_inner,
};
use knobas_app::sources::Registry;
use knobas_app::suggestion_pass::SuggestionPass;
use sqlx::PgPool;

/// The demo corpus in a database nothing else touches.
async fn demo() -> PgPool {
    let pool = knobas_db::test_util::scratch_database("suggest_ipc")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .expect("the demo corpus loads");
    pool
}

/// Detection over the shipped dataset proposes real, readable suggestions.
///
/// The assertions are about *shape and quality*, not about a fixed count: the
/// fixture is allowed to grow. What may not happen is a pass that finds nothing,
/// or one that proposes something the tray cannot draw.
#[tokio::test]
async fn a_pass_over_the_demo_corpus_proposes_suggestions_a_reader_can_judge() {
    let pool = demo().await;

    let written = detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    assert!(
        written > 0,
        "not one rule fires on the corpus knobas ships -- a rule keyed on a \
         column no adapter fills passes every unit test and proposes nothing here"
    );

    let page = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    assert_eq!(page.total, i64::from(written));
    // The tray draws at most three rows per item (#774), so the page may hold
    // fewer rows than the pass wrote. Every proposal it left out touches an
    // item already holding three drawn rows, and that item says so in `more`.
    let mut held = std::collections::HashMap::<&str, usize>::new();
    for entry in &page.rows {
        for end in [&entry.from.entity_id, &entry.to.entity_id] {
            *held.entry(end.as_str()).or_default() += 1;
        }
    }
    assert!(
        held.values().all(|&n| n <= 3),
        "at most three rows per item"
    );
    let stored: Vec<(uuid::Uuid, String, String)> =
        sqlx::query_as("select id, from_id, to_id from knobas.proposed_link")
            .fetch_all(&pool)
            .await
            .unwrap();
    for (id, from, to) in &stored {
        if page.rows.iter().any(|e| e.link.id == *id) {
            continue;
        }
        let full = [from, to]
            .into_iter()
            .find(|end| held.get(end.as_str()) == Some(&3))
            .unwrap_or_else(|| panic!("{from} -> {to} was left out with room on both ends"));
        assert!(
            page.more.iter().any(|m| m.item.entity_id == *full),
            "{full} holds rows the tray left out, and `more` must say so"
        );
    }

    for entry in &page.rows {
        assert!(
            entry.link.confirmed_at.is_none(),
            "the tray holds proposals only"
        );
        let reason = entry
            .link
            .reason
            .as_deref()
            .expect("a suggestion whose reason cannot be shown is not shippable");
        assert!(!reason.trim().is_empty());
        assert!(entry.link.rule_class.is_some(), "and it is classified");
        // Both ends are drawable: a row with a blank end is one the reader
        // cannot open before deciding.
        for end in [&entry.from, &entry.to] {
            assert!(!end.entity_id.is_empty());
            assert!(!end.kind.is_empty());
        }
        assert_ne!(entry.from.entity_id, entry.to.entity_id);
    }

    // Every class the engine knows is reachable on this corpus, which is what
    // makes "exact and speculative are distinguishable" true in practice rather
    // than only in the schema.
    let classes: std::collections::HashSet<_> =
        page.rows.iter().filter_map(|e| e.link.rule_class).collect();
    assert!(
        classes.len() >= 2,
        "the demo corpus exercises only {classes:?} -- a user cannot calibrate \
         trust on one class"
    );

    // Idempotent on the real corpus too, not only on a hand-built one.
    assert_eq!(
        detect_suggestions_inner(&SuggestionPass::beside(&pool))
            .await
            .unwrap(),
        0
    );
}

/// Accepting moves a proposal out of the tray and into the links panel, and
/// writes one activity line saying so.
#[tokio::test]
async fn accepting_moves_a_proposal_into_the_panel_and_records_it() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    let page = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    let entry = page
        .rows
        .first()
        .expect("the demo corpus proposes something");
    let id = entry.link.id.to_string();
    let (from, to) = (entry.from.entity_id.clone(), entry.to.entity_id.clone());
    let reason = entry.link.reason.clone().unwrap();

    let written = accept_suggestion_inner(&pool, &id)
        .await
        .unwrap()
        .expect("there was a proposal to accept");
    assert_eq!(written.activity.verb, "accepted");
    assert_eq!(written.activity.actor, "user");
    assert_eq!(
        written.activity.detail["reason"],
        serde_json::json!(reason),
        "the line says what was accepted, not merely that something was"
    );

    // In the panel, on both ends -- the same read every other link arrives
    // through.
    for end in [&from, &to] {
        let detail = get_entity_inner(&pool, &Registry::for_process(), end)
            .await
            .unwrap();
        assert!(
            detail.links.iter().any(|e| e.link.id == entry.link.id),
            "an accepted suggestion is an ordinary link on {end}"
        );
        assert!(
            detail.links.iter().all(|e| e.link.confirmed_at.is_some()),
            "and the panel still shows nothing unconfirmed"
        );
    }

    // ...and out of the tray.
    let after = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    assert!(!after.rows.iter().any(|e| e.link.id == entry.link.id));
    assert_eq!(after.total, page.total - 1);

    // Idempotent: pressing it twice writes one line.
    assert!(accept_suggestion_inner(&pool, &id).await.unwrap().is_none());
}

/// Dismissing removes a proposal and a later pass does not bring it back.
#[tokio::test]
async fn dismissing_is_remembered_across_a_later_pass() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    let page = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    let entry = page
        .rows
        .first()
        .expect("the demo corpus proposes something");
    let id = entry.link.id.to_string();

    let written = dismiss_suggestion_inner(&pool, &id)
        .await
        .unwrap()
        .expect("there was a proposal to dismiss");
    assert_eq!(written.activity.verb, "dismissed");

    // A full re-sync of the source, then another pass: the corpus is back
    // exactly as it was, so the rule that proposed this has every reason to do
    // it again.
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .unwrap();
    assert_eq!(
        detect_suggestions_inner(&SuggestionPass::beside(&pool))
            .await
            .unwrap(),
        0,
        "a dismissal survives a re-sync"
    );
    let after = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    assert!(!after.rows.iter().any(|e| e.link.id == entry.link.id));
    assert_eq!(after.total, page.total - 1);
    assert!(
        dismiss_suggestion_inner(&pool, &id)
            .await
            .unwrap()
            .is_none()
    );
}

/// A malformed id is a bad address, and an id nothing carries is a missing
/// thing -- the two want different words on screen.
#[tokio::test]
async fn a_bad_suggestion_id_is_invalid_and_an_unknown_one_is_not_found() {
    let pool = demo().await;

    for refused in [
        accept_suggestion_inner(&pool, "not-a-uuid")
            .await
            .unwrap_err(),
        dismiss_suggestion_inner(&pool, "not-a-uuid")
            .await
            .unwrap_err(),
    ] {
        assert_eq!(refused.code, IpcErrorCode::Invalid);
    }
    let unknown = uuid::Uuid::new_v4().to_string();
    for refused in [
        accept_suggestion_inner(&pool, &unknown).await.unwrap_err(),
        dismiss_suggestion_inner(&pool, &unknown).await.unwrap_err(),
    ] {
        assert_eq!(refused.code, IpcErrorCode::NotFound);
    }
}

/// Drawing by hand the link knobas had already proposed accepts it, rather than
/// telling the user "already linked" about a pair whose panel is empty.
#[tokio::test]
async fn linking_a_proposed_pair_by_hand_accepts_it_instead_of_refusing() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    let page = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    // A proposal whose relation is the one *Link to…* defaults to, since that
    // is the collision a user can actually walk into.
    let entry = page
        .rows
        .iter()
        .find(|e| e.link.relation == knobas_app::commands::entity::DEFAULT_RELATION)
        .expect("the corpus proposes a plain related link");

    let written = create_link_inner(
        &pool,
        &entry.from.entity_id,
        &entry.to.entity_id,
        None,
        None,
    )
    .await
    .expect("drawing a proposed link is accepting it, not a conflict");
    assert_eq!(written.link.id, entry.link.id, "the same row, promoted");
    assert!(written.link.confirmed_at.is_some());
    assert_eq!(written.activity.verb, "linked");

    // ...and a second attempt is the conflict it has always been.
    let refused = create_link_inner(
        &pool,
        &entry.from.entity_id,
        &entry.to.entity_id,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, IpcErrorCode::Conflict);

    assert!(
        written.superseded.is_none(),
        "a same-direction promotion displaces nothing"
    );
}

/// Drawing the **reverse** of a proposal withdraws it and writes the link the
/// user drew -- it does not strand the proposal in the tray (#70).
///
/// This is the second symptom recorded on #70, found by PR #132's merge-manager:
/// the promotion path was direction-exact and the unique index was directed, so
/// the two cancelled out and the reversed hand-drawn link simply *succeeded* --
/// leaving a stale proposal sitting in the tray beside a confirmed link for one
/// pair. Both halves are asserted here, because either alone still passes on a
/// broken build: the tray has to empty **and** the link has to exist, pointing
/// the way the user drew it.
#[tokio::test]
async fn drawing_the_reverse_of_a_proposal_empties_the_tray_row_and_keeps_the_users_direction() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    let page = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    let entry = page
        .rows
        .iter()
        .find(|e| e.link.relation == knobas_app::commands::entity::DEFAULT_RELATION)
        .expect("the corpus proposes a plain related link");

    // Reversed: the user draws the proposal's `to` end as the `from` end.
    let written = create_link_inner(
        &pool,
        &entry.to.entity_id,
        &entry.from.entity_id,
        None,
        None,
    )
    .await
    .expect("the user's direction wins over a proposal's");

    assert_ne!(
        written.link.id, entry.link.id,
        "the proposal is withdrawn, not confirmed -- confirming would store the \
         opposite of what the user drew"
    );
    assert_eq!(written.link.from_id, entry.to.entity_id);
    assert_eq!(written.link.to_id, entry.from.entity_id);
    assert!(written.link.confirmed_at.is_some());
    assert_eq!(written.activity.verb, "linked");

    // The proposal leaving the tray is a mutation, so it gets its own line.
    let dismissal = written
        .superseded
        .expect("the displaced proposal must be announced, not vanish quietly");
    assert_eq!(dismissal.verb, "dismissed");

    // Gone from the tray, and one link for the pair rather than two rows.
    let after = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    assert!(
        !after.rows.iter().any(|e| e.link.id == entry.link.id),
        "the proposal must not be left standing beside the link that replaced it"
    );

    // And a third attempt, either way round, is the conflict it has always been.
    for (a, b) in [
        (&entry.from.entity_id, &entry.to.entity_id),
        (&entry.to.entity_id, &entry.from.entity_id),
    ] {
        let refused = create_link_inner(&pool, a, b, None, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code, IpcErrorCode::Conflict);
    }
}

/// The tray is scoped to the room, and the count is the room's.
#[tokio::test]
async fn the_tray_is_scoped_to_the_room_it_is_drawn_in() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();

    let everywhere = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    let demo_room = room_suggestions_inner(&pool, &["mock".to_owned()], None, 500)
        .await
        .unwrap();
    let nowhere = room_suggestions_inner(&pool, &["no-such-source".to_owned()], None, 500)
        .await
        .unwrap();

    assert_eq!(
        demo_room.total, everywhere.total,
        "the demo corpus is one source, so its room holds all of it"
    );
    assert_eq!(nowhere.total, 0, "and a room with no items holds none");
    assert!(
        everywhere.total > 0,
        "an empty scope is every source, not no source"
    );

    // `limit` caps the rows and not the count -- the heading answers "how many
    // are waiting", which a capped number would not.
    let capped = room_suggestions_inner(&pool, &[], None, 1).await.unwrap();
    assert_eq!(capped.rows.len(), 1);
    assert_eq!(capped.total, everywhere.total);
}

/// Every proposal a pass makes over the shipped corpus has evidence a reader
/// can judge (#836): an exact key is quoted where the item names it, with the
/// key marked; shared wording is quoted from both ends, the shared words
/// marked; and *Speaks against it* always says something. The tray's page is
/// the same before and after the reads: evidence changes no ranking.
#[tokio::test]
async fn every_proposal_over_the_demo_corpus_has_evidence_with_the_match_marked() {
    let pool = demo().await;
    detect_suggestions_inner(&SuggestionPass::beside(&pool))
        .await
        .unwrap();
    let before = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();

    let mut keyed = 0;
    let mut worded = 0;
    for entry in &before.rows {
        let id = entry.link.id.to_string();
        let evidence = proposal_evidence_inner(&pool, &id).await.unwrap();
        assert_eq!(evidence.link_id, entry.link.id);
        assert_eq!(
            evidence.reason, entry.link.reason,
            "the stored reason, unchanged"
        );
        assert!(
            !evidence.against.is_empty(),
            "{id}: *Speaks against it* is never blank"
        );
        let marked: Vec<&str> = evidence
            .excerpts
            .iter()
            .flat_map(|e| e.parts.iter().filter(|p| p.marked).map(|p| p.text.as_str()))
            .collect();
        match entry.link.rule.as_deref() {
            Some(
                "branch_name_key" | "commit_message_key" | "build_parameter_key" | "page_text_key",
            ) => {
                let key = entry.to.entity_id.split_once(':').unwrap().1;
                assert!(
                    !marked.is_empty() && marked.iter().all(|m| *m == key),
                    "{id}: the key {key} is what is marked, and only it: {marked:?}"
                );
                assert!(
                    evidence
                        .excerpts
                        .iter()
                        .all(|e| e.entity_id == entry.from.entity_id),
                    "{id}: the key is quoted from the item that names it"
                );
                keyed += 1;
            }
            Some("similar_text") => {
                assert!(!marked.is_empty(), "{id}: the shared words are marked");
                assert!(
                    evidence
                        .support
                        .iter()
                        .any(|s| s.starts_with("Shared words: ")),
                    "{id}: {:?}",
                    evidence.support
                );
                worded += 1;
            }
            _ => {}
        }
    }
    assert!(keyed > 0, "the demo corpus has exact-key proposals to read");
    assert!(worded > 0, "and shared-wording ones");

    let after = room_suggestions_inner(&pool, &[], None, 500).await.unwrap();
    let ids = |page: &knobas_app::commands::entity::SuggestionPage| {
        page.rows.iter().map(|e| e.link.id).collect::<Vec<_>>()
    };
    assert_eq!(
        ids(&before),
        ids(&after),
        "reading evidence changes no ranking"
    );
}

/// The evidence read addresses a proposal as the answers do.
#[tokio::test]
async fn evidence_for_a_bad_id_is_invalid_and_for_an_unknown_one_not_found() {
    let pool = demo().await;
    assert_eq!(
        proposal_evidence_inner(&pool, "not-a-uuid")
            .await
            .unwrap_err()
            .code,
        IpcErrorCode::Invalid
    );
    assert_eq!(
        proposal_evidence_inner(&pool, &uuid::Uuid::new_v4().to_string())
            .await
            .unwrap_err()
            .code,
        IpcErrorCode::NotFound
    );
}
