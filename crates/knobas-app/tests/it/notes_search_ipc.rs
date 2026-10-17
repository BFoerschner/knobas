//! Notes leave the launcher's search while *Show notes and quick capture* is
//! off (#940, R-13a.6, D10), at `search_inner`: the seam every launcher query,
//! `note:` and the `kind:` suggestions go through. The notes themselves stay.

use knobas_app::capture::SHOWN_KEY;
use knobas_app::commands::entity::create_note_inner;
use knobas_app::commands::search::{
    create_smart_list_inner, filter_suggestions_inner, launcher_home_inner, search_inner,
    smart_lists_inner,
};
use knobas_search::{SearchFilters, SearchQuery};

async fn pool() -> sqlx::PgPool {
    knobas_db::test_util::scratch_database("notes-search")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 20,
        filters: SearchFilters::default(),
    }
}

/// The ids of every row `raw` returns, whatever group it is in.
async fn ids(pool: &sqlx::PgPool, raw: &str) -> Vec<String> {
    search_inner(pool, query(raw))
        .await
        .expect("a search")
        .groups
        .into_iter()
        .flat_map(|group| group.hits)
        .map(|hit| hit.row.entity_id)
        .collect()
}

/// One note, found by a word nothing else holds, with the setting off and
/// then on; and the `note:` prefix and the `kind:` suggestions with it.
#[tokio::test]
async fn a_hidden_note_is_not_found_and_a_shown_one_is() {
    let pool = pool().await;
    let note = create_note_inner(
        &pool,
        Some("Zephyrine standup"),
        Some("zephyrine body"),
        &[],
    )
    .await
    .expect("a note");
    let id = note.note.id;

    // Off is the default: no row has been written.
    assert!(
        !ids(&pool, "zephyrine").await.contains(&id),
        "a hidden note is in the plain search"
    );
    assert!(
        ids(&pool, "note:zephyrine").await.is_empty(),
        "a hidden note is behind the note: prefix"
    );
    assert!(
        !launcher_home_inner(&pool).await.expect("home").notes_shown,
        "launcher_home says notes are shown while they are hidden (#1036)"
    );
    let kinds = filter_suggestions_inner(&pool, "", false)
        .await
        .expect("suggestions")
        .kinds;
    assert!(
        kinds.iter().all(|kind| kind.kind != "note"),
        "kind: still offers note"
    );

    knobas_app::settings::write(&pool, SHOWN_KEY, Some("true"))
        .await
        .expect("notes shown");
    assert!(
        ids(&pool, "zephyrine").await.contains(&id),
        "a shown note is in the plain search"
    );
    assert!(
        ids(&pool, "note:zephyrine").await.contains(&id),
        "a shown note is behind the note: prefix"
    );
    assert!(
        launcher_home_inner(&pool).await.expect("home").notes_shown,
        "launcher_home says notes are hidden while they are shown (#1036)"
    );
    let kinds = filter_suggestions_inner(&pool, "", false)
        .await
        .expect("suggestions")
        .kinds;
    assert!(kinds.iter().any(|kind| kind.kind == "note"));
    let list = create_smart_list_inner(&pool, "Zephyrine notes", "note:zephyrine")
        .await
        .expect("a saved list");
    assert_eq!(list.count, 1, "the saved list counts the shown note");

    // Off again: the note is still there, only the search leaves it out.
    knobas_app::settings::write(&pool, SHOWN_KEY, None)
        .await
        .expect("notes hidden");
    assert!(!ids(&pool, "zephyrine").await.contains(&id));
    let rail = smart_lists_inner(&pool).await.expect("the rail");
    let row = rail.iter().find(|row| row.id == list.id).expect("the list");
    assert_eq!(row.count, 0, "the saved list counts a hidden note");
    let kept: i64 = sqlx::query_scalar("select count(*) from knobas.note where id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 1, "hiding notes deletes none");
}
