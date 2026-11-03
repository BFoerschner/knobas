//! Favourites (#2428, spec #2419 stories 39 and 40): the three commands and
//! the empty box's read, through their `_inner` bodies against a real
//! database.
//!
//! A favourite is global state on the board, so each test takes a scratch
//! database of its own and can say what the whole block is.

use knobas_app::IpcErrorCode;
use knobas_app::commands::search::{
    add_favourite_inner, create_smart_list_inner, delete_smart_list_inner, favourites_inner,
    launcher_home_inner, remove_favourite_inner,
};
use knobas_app::sources::Registry;
use knobas_search::FavouriteTarget;

async fn pool(label: &str) -> sqlx::PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// One mirrored ticket or pull request.
async fn mirror_row(pool: &sqlx::PgPool, id: &str, kind: &str, title: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat')
         on conflict (id) do nothing",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, item_updated_at, payload)
         values ($1,'jira',$2,$3, now() - interval '1 day', '{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
}

fn entity(id: &str) -> FavouriteTarget {
    FavouriteTarget::Entity(id.to_owned())
}

fn list(id: &str) -> FavouriteTarget {
    FavouriteTarget::List(id.to_owned())
}

/// `(kind, entity_id, title)` of every favourite, in the block's order.
async fn block(pool: &sqlx::PgPool) -> Vec<(String, String, String)> {
    favourites_inner(pool, &Registry::for_process())
        .await
        .expect("the favourites")
        .into_iter()
        .map(|row| (row.kind, row.entity_id, row.title))
        .collect()
}

/// **Stories 39 and 40**: lists and entities in one block, in the order they
/// were favourited, and the empty box carries the same block.
#[tokio::test]
async fn favourites_are_one_block_in_the_order_they_were_favourited() {
    let pool = pool("fav-order").await;
    mirror_row(&pool, "jira:PAY-1", "ticket", "Retry failed payouts").await;
    mirror_row(&pool, "jira:pr-7", "pr", "Backoff for the retry loop").await;
    let saved = create_smart_list_inner(&pool, &Registry::for_process(), "Payouts", "payouts")
        .await
        .expect("a saved list");

    for target in [entity("jira:pr-7"), list(&saved.id), entity("jira:PAY-1")] {
        add_favourite_inner(&pool, &Registry::for_process(), &target)
            .await
            .expect("favourite");
    }
    // Twice is the same row, and it keeps its place.
    add_favourite_inner(&pool, &Registry::for_process(), &entity("jira:pr-7"))
        .await
        .expect("favourite again");

    let expected = vec![
        (
            "pr".to_owned(),
            "jira:pr-7".to_owned(),
            "Backoff for the retry loop".to_owned(),
        ),
        ("list".to_owned(), saved.id.clone(), "Payouts".to_owned()),
        (
            "ticket".to_owned(),
            "jira:PAY-1".to_owned(),
            "Retry failed payouts".to_owned(),
        ),
    ];
    assert_eq!(block(&pool).await, expected);

    let home = launcher_home_inner(&pool, &Registry::for_process())
        .await
        .expect("the empty box");
    let carried: Vec<(String, String, String)> = home
        .favourites
        .iter()
        .map(|row| (row.kind.clone(), row.entity_id.clone(), row.title.clone()))
        .collect();
    assert_eq!(carried, expected, "the empty box draws the same block");
    // A list favourite is drawn as a list hit is: its text on the path line.
    let as_list = &home.favourites[1];
    assert_eq!(as_list.source_id, "list");
    assert_eq!(as_list.path.as_deref(), Some("payouts"));
    assert_eq!(as_list.state, None, "a healthy list needs no attention");
}

/// Unfavourite takes one thing out and leaves the others in their order; a
/// deleted list leaves the block with it.
#[tokio::test]
async fn unfavourite_and_a_deleted_list_leave_the_block() {
    let pool = pool("fav-remove").await;
    mirror_row(&pool, "jira:PAY-1", "ticket", "One").await;
    mirror_row(&pool, "jira:PAY-2", "ticket", "Two").await;
    let saved = create_smart_list_inner(&pool, &Registry::for_process(), "Mine", "one")
        .await
        .expect("a saved list");
    for target in [entity("jira:PAY-1"), list(&saved.id), entity("jira:PAY-2")] {
        add_favourite_inner(&pool, &Registry::for_process(), &target)
            .await
            .expect("favourite");
    }

    remove_favourite_inner(&pool, &Registry::for_process(), &entity("jira:PAY-1"))
        .await
        .expect("unfavourite");
    // Removing what is not a favourite is no error: the reader's intent holds.
    remove_favourite_inner(&pool, &Registry::for_process(), &entity("jira:PAY-1"))
        .await
        .expect("unfavourite again");
    let ids: Vec<String> = block(&pool).await.into_iter().map(|row| row.1).collect();
    assert_eq!(ids, vec![saved.id.clone(), "jira:PAY-2".to_owned()]);

    delete_smart_list_inner(&pool, &Registry::for_process(), &saved.id)
        .await
        .expect("delete the list");
    let ids: Vec<String> = block(&pool).await.into_iter().map(|row| row.1).collect();
    assert_eq!(ids, vec!["jira:PAY-2".to_owned()]);

    // A favourite added after a removal goes last, past every number used.
    add_favourite_inner(&pool, &Registry::for_process(), &entity("jira:PAY-1"))
        .await
        .expect("favourite again");
    let ids: Vec<String> = block(&pool).await.into_iter().map(|row| row.1).collect();
    assert_eq!(ids, vec!["jira:PAY-2".to_owned(), "jira:PAY-1".to_owned()]);
}

/// A list nobody saved is `not_found`; an entity the mirror does not hold yet
/// is kept and not drawn (a share export arrives before the sync does), and a
/// withdrawn one is not drawn either.
#[tokio::test]
async fn an_unknown_list_is_refused_and_an_unknown_entity_waits() {
    let pool = pool("fav-unknown").await;
    let refused = add_favourite_inner(&pool, &Registry::for_process(), &list("nobody-saved"))
        .await
        .expect_err("a list nobody saved");
    assert_eq!(refused.code, IpcErrorCode::NotFound, "{refused:?}");

    add_favourite_inner(&pool, &Registry::for_process(), &entity("jira:PAY-9"))
        .await
        .expect("an entity this machine has not synced yet");
    assert_eq!(block(&pool).await, Vec::new());
    mirror_row(&pool, "jira:PAY-9", "ticket", "Arrived").await;
    assert_eq!(
        block(&pool).await,
        vec![(
            "ticket".to_owned(),
            "jira:PAY-9".to_owned(),
            "Arrived".to_owned()
        )],
        "the favourite resolves once the item is synced"
    );

    sqlx::query("update knobas.entity set deleted_at = now() where id = 'jira:PAY-9'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        block(&pool).await,
        Vec::new(),
        "a withdrawn entity is not drawn"
    );
}

/// A saved list whose text today's grammar refuses is drawn with the refusal
/// as its state, as its `#l` hit is (#2423).
#[tokio::test]
async fn a_favourite_list_that_needs_attention_says_why() {
    let pool = pool("fav-attention").await;
    sqlx::query(
        "insert into knobas.smart_list (id, label, query) values ('old', 'Old', 'list:payouts')",
    )
    .execute(&pool)
    .await
    .unwrap();
    add_favourite_inner(&pool, &Registry::for_process(), &list("old"))
        .await
        .expect("favourite");
    let rows = favourites_inner(&pool, &Registry::for_process())
        .await
        .expect("the favourites");
    let state = rows[0].state.as_ref().expect("needs attention");
    assert_eq!(state.tone, Some(knobas_core::tone::Tone::Warn));
    assert!(!state.text.is_empty());
}
