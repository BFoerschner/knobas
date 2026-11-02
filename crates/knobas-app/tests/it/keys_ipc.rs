//! Keys (#2429, spec #2419 stories 43 to 49): a saved list's chord and an
//! action's chord per kind, through the commands' `_inner` bodies against a
//! real database.
//!
//! The keys are global state, so each test takes a scratch database of its
//! own and can say what the whole table is.

use knobas_app::IpcErrorCode;
use knobas_app::commands::search::{
    bind_action_key_inner, bind_list_key_inner, create_smart_list_inner, delete_smart_list_inner,
    keys_inner,
};
use knobas_app::sources::Registry;
use knobas_search::{ActionKey, ListKey};

async fn pool(label: &str) -> sqlx::PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

async fn saved(pool: &sqlx::PgPool, query: &str, label: &str) -> String {
    create_smart_list_inner(pool, &Registry::for_process(), label, query)
        .await
        .expect("save a list")
        .id
}

fn list_keys(lists: &[ListKey]) -> Vec<(&str, &str, &str, &str)> {
    lists
        .iter()
        .map(|key| {
            (
                key.list_id.as_str(),
                key.chord.as_str(),
                key.label.as_str(),
                key.text.as_str(),
            )
        })
        .collect()
}

fn action_keys(actions: &[ActionKey]) -> Vec<(&str, &str, &str)> {
    actions
        .iter()
        .map(|key| (key.kind.as_str(), key.chord.as_str(), key.action_id.as_str()))
        .collect()
}

#[tokio::test]
async fn a_list_holds_one_chord_and_a_chord_one_list() {
    let pool = pool("keys_list_chord").await;
    let sepa = saved(&pool, "#t sepa", "Sepa").await;
    let mine = saved(&pool, "#p @me", "Mine").await;

    bind_list_key_inner(&pool, &sepa, Some("⌘1")).await.expect("bind ⌘1");
    let keys = keys_inner(&pool).await.unwrap();
    assert_eq!(
        list_keys(&keys.lists),
        vec![(sepa.as_str(), "⌘1", "Sepa", "#t sepa")],
        "the read carries the list's name and text, so a chord can open it"
    );

    // A new chord on the same list replaces its old one.
    bind_list_key_inner(&pool, &sepa, Some("⌘⇧S")).await.unwrap();
    assert_eq!(
        list_keys(&keys_inner(&pool).await.unwrap().lists),
        vec![(sepa.as_str(), "⌘⇧S", "Sepa", "#t sepa")]
    );

    // The same chord on another list moves to it.
    bind_list_key_inner(&pool, &mine, Some("⌘⇧S")).await.unwrap();
    assert_eq!(
        list_keys(&keys_inner(&pool).await.unwrap().lists),
        vec![(mine.as_str(), "⌘⇧S", "Mine", "#p @me")]
    );

    // No chord removes the key.
    bind_list_key_inner(&pool, &mine, None).await.unwrap();
    assert!(keys_inner(&pool).await.unwrap().lists.is_empty());
}

#[tokio::test]
async fn deleting_a_list_forgets_its_key_and_an_unknown_list_is_not_found() {
    let pool = pool("keys_list_gone").await;
    let sepa = saved(&pool, "#t sepa", "Sepa").await;
    bind_list_key_inner(&pool, &sepa, Some("⌥2")).await.unwrap();
    delete_smart_list_inner(&pool, &Registry::for_process(), &sepa).await.unwrap();
    assert!(keys_inner(&pool).await.unwrap().lists.is_empty());

    let error = bind_list_key_inner(&pool, "nobody-saved-this", Some("⌘3"))
        .await
        .expect_err("a list nobody saved has no key");
    assert_eq!(error.code, IpcErrorCode::NotFound);
}

#[tokio::test]
async fn one_chord_means_another_action_on_another_kind() {
    let pool = pool("keys_action_chord").await;
    bind_action_key_inner(&pool, "repo", "open-vscode", Some("⌘⇧O")).await.unwrap();
    bind_action_key_inner(&pool, "pr", "approve", Some("⌘⇧O")).await.unwrap();
    assert_eq!(
        action_keys(&keys_inner(&pool).await.unwrap().actions),
        vec![("pr", "⌘⇧O", "approve"), ("repo", "⌘⇧O", "open-vscode")]
    );

    // On one kind the chord moves to the action bound last...
    bind_action_key_inner(&pool, "pr", "copy", Some("⌘⇧O")).await.unwrap();
    // ...and an action holds one chord: a new one replaces the old.
    bind_action_key_inner(&pool, "repo", "open-vscode", Some("⌃9")).await.unwrap();
    assert_eq!(
        action_keys(&keys_inner(&pool).await.unwrap().actions),
        vec![("pr", "⌘⇧O", "copy"), ("repo", "⌃9", "open-vscode")]
    );

    bind_action_key_inner(&pool, "pr", "copy", None).await.unwrap();
    assert_eq!(
        action_keys(&keys_inner(&pool).await.unwrap().actions),
        vec![("repo", "⌃9", "open-vscode")]
    );
}

#[tokio::test]
async fn a_chord_nothing_can_press_is_refused() {
    let pool = pool("keys_bad_chord").await;
    let sepa = saved(&pool, "#t sepa", "Sepa").await;
    for bad in ["O", "⇧O", "⌘", "⌘OO", "⌘/", "⌘o", "⇧⌘O", ""] {
        let error = bind_action_key_inner(&pool, "repo", "clone", Some(bad))
            .await
            .expect_err(bad);
        assert_eq!(error.code, IpcErrorCode::Invalid, "{bad:?} as an action's chord");
        let error = bind_list_key_inner(&pool, &sepa, Some(bad)).await.expect_err(bad);
        assert_eq!(error.code, IpcErrorCode::Invalid, "{bad:?} as a list's chord");
    }
    let keys = keys_inner(&pool).await.unwrap();
    assert!(keys.lists.is_empty() && keys.actions.is_empty());
}
