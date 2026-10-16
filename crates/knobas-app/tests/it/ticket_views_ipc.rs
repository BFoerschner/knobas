//! Ticket views (#812) through the bodies of their three commands,
//! `ticket_views`, `save_ticket_view` and `delete_ticket_view`, over the
//! embedded PostgreSQL with Jira tickets stored as the adapter stores them.
//!
//! This file's database is its own (#572), but its tests share it: each one
//! uses a project key and view names nothing else holds, and reads back only
//! the views it saved.

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::mini_board_inner;
use knobas_app::ticket_views::{self, TicketView, TicketViewDraft, TicketViewRow};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat', '{\"username\": \"mara.lindqvist\"}')
         on conflict (id) do update set kind = excluded.kind, enabled = true,
                                         config = excluded.config",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// A project key nothing else in this database holds: letters only.
fn project() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..10]
        .chars()
        .map(|c| char::from(b'A' + u8::try_from(c.to_digit(16).unwrap()).unwrap()))
        .collect();
    format!("V{letters}")
}

/// One issue in `status`, stored the way the Jira adapter stores it.
async fn issue(pool: &sqlx::PgPool, key: &str, status: &str, summary: &str) {
    let project = key.split('-').next().unwrap();
    let payload = serde_json::json!({
        "key": key,
        "fields": {
            "summary": summary,
            "status": { "name": status },
            "issuetype": { "name": "Story" },
            "project": { "key": project, "name": "Payments" },
        },
    });
    let id = format!("jira:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(summary)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'jira','ticket',$2,$2,$3, now())",
    )
    .bind(&id)
    .bind(summary)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

fn draft(name: &str, query: &str) -> TicketViewDraft {
    TicketViewDraft {
        id: None,
        name: name.to_owned(),
        group: "Team".to_owned(),
        position: None,
        query: query.to_owned(),
        layout: "board".to_owned(),
        columns: Vec::new(),
        lanes: "none".to_owned(),
        card_fields: vec!["type".to_owned(), "signals".to_owned()],
        list_columns: vec!["key".to_owned(), "summary".to_owned(), "status".to_owned()],
        list_sort: None,
        jira_ref: None,
    }
}

/// The rows `ticket_views` answers for these ids, in its order.
async fn listed(pool: &sqlx::PgPool, ids: &[&str]) -> Vec<TicketViewRow> {
    ticket_views::list(pool)
        .await
        .expect("ticket_views")
        .into_iter()
        .filter(|row| ids.contains(&row.view.id.as_str()))
        .collect()
}

/// A view's count is the number of tickets its board draws: the same text
/// through `mini_board`, summed over the columns.
#[tokio::test]
async fn a_views_count_is_the_tickets_its_board_draws() {
    let pool = pool().await;
    let p = project();
    issue(&pool, &format!("{p}-1"), "To Do", "Retry failed payouts").await;
    issue(&pool, &format!("{p}-2"), "In Progress", "Retry the ledger").await;
    issue(&pool, &format!("{p}-3"), "Done", "Payout report").await;
    let text = format!("project:{p} retry");

    let saved = ticket_views::save(&pool, draft(&format!("Retries {p}"), &text))
        .await
        .expect("save");
    let rows = listed(&pool, &[&saved.id]).await;
    let board = mini_board_inner(&pool, None, &[], None, None, Some(&text))
        .await
        .expect("mini_board");
    let drawn: u32 = board.columns.iter().map(|column| column.total).sum();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].count, Some(2), "{rows:?}");
    assert_eq!(rows[0].count, Some(u64::from(drawn)));
    assert_eq!(rows[0].unreadable, None);
}

/// Saved as sent, read back as stored, with no cap: more views than smart
/// lists may hold, each name its own address, and a second view of the same
/// name a second address.
#[tokio::test]
async fn a_view_keeps_what_it_was_saved_with_and_there_is_no_cap() {
    let pool = pool().await;
    let p = project();
    let mut ids = Vec::new();
    for n in 0..knobas_search::saved::MAX_SAVED_LISTS + 4 {
        let saved = ticket_views::save(&pool, draft(&format!("{p} view {n}"), &format!("project:{p}")))
            .await
            .unwrap_or_else(|error| panic!("view {n}: {error:?}"));
        ids.push(saved.id);
    }
    let twin = ticket_views::save(&pool, draft(&format!("{p} view 0"), ""))
        .await
        .expect("a second view of the same name");
    assert_eq!(ids[0], format!("{}-view-0", p.to_lowercase()));
    assert_eq!(twin.id, format!("{}-2", ids[0]));

    let mut whole = draft(&format!("{p} Whole"), "  project:X   updated:7d ");
    whole.layout = "list".to_owned();
    whole.columns = vec![ticket_views::ViewColumn {
        name: "Doing".to_owned(),
        statuses: vec!["In Progress".to_owned(), "In Review".to_owned()],
        limit: Some(4),
    }];
    whole.lanes = "assignee".to_owned();
    whole.list_sort = Some(ticket_views::ListSort {
        column: "updated".to_owned(),
        direction: "ascending".to_owned(),
    });
    whole.jira_ref = Some("board:21".to_owned());
    let saved = ticket_views::save(&pool, whole.clone()).await.expect("save");
    assert_eq!(
        saved,
        TicketView {
            id: saved.id.clone(),
            name: whole.name.clone(),
            group: whole.group.clone(),
            position: saved.position,
            query: "  project:X   updated:7d ".to_owned(),
            layout: whole.layout.clone(),
            columns: whole.columns.clone(),
            lanes: whole.lanes.clone(),
            card_fields: whole.card_fields.clone(),
            list_columns: whole.list_columns.clone(),
            list_sort: whole.list_sort.clone(),
            jira_ref: whole.jira_ref.clone(),
        }
    );

    let mut all: Vec<&str> = ids.iter().map(String::as_str).collect();
    all.push(&twin.id);
    all.push(&saved.id);
    let rows = listed(&pool, &all).await;
    assert_eq!(rows.len(), all.len(), "every view is listed");
    assert_eq!(rows.last().unwrap().view, saved, "the newest is last, and read back as stored");
    let positions: Vec<i32> = rows.iter().map(|row| row.view.position).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{positions:?}");
}

/// A text the grammar cannot read is listed as that text, with the part it
/// could not read and no count; and reading it rewrites nothing.
#[tokio::test]
async fn an_unreadable_text_is_listed_as_text_and_reading_rewrites_nothing() {
    let pool = pool().await;
    let p = project();
    issue(&pool, &format!("{p}-1"), "To Do", "Retry failed payouts").await;
    let text = format!("project:{p} sprint:next  retry");
    let saved = ticket_views::save(&pool, draft(&format!("Next sprint {p}"), &text))
        .await
        .expect("a text the grammar cannot read still saves");

    for _ in 0..2 {
        let rows = listed(&pool, &[&saved.id]).await;
        assert_eq!(rows[0].count, None);
        assert_eq!(rows[0].unreadable.as_deref(), Some("sprint:next"));
        assert_eq!(rows[0].view.query, text);
    }
    let stored: String = sqlx::query_scalar("select query from knobas.ticket_view where id = $1")
        .bind(&saved.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, text);
}

/// Saving over a view keeps its place; deleting it and saving it back (*Undo*)
/// puts it where it was; deleting what is not there is `not_found`.
#[tokio::test]
async fn a_deleted_view_comes_back_where_it_was() {
    let pool = pool().await;
    let p = project();
    let first = ticket_views::save(&pool, draft(&format!("{p} first"), "")).await.unwrap();
    let second = ticket_views::save(&pool, draft(&format!("{p} second"), "")).await.unwrap();
    let third = ticket_views::save(&pool, draft(&format!("{p} third"), "")).await.unwrap();

    let renamed = ticket_views::save(
        &pool,
        TicketViewDraft {
            id: Some(second.id.clone()),
            ..draft(&format!("{p} second, renamed"), "assignee:@me")
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.id, second.id);
    assert_eq!(renamed.position, second.position, "saving over a view keeps its place");

    ticket_views::delete(&pool, &second.id).await.expect("delete");
    let ids = [first.id.as_str(), second.id.as_str(), third.id.as_str()];
    assert_eq!(
        listed(&pool, &ids).await.iter().map(|row| row.view.id.clone()).collect::<Vec<_>>(),
        [first.id.clone(), third.id.clone()]
    );
    let gone = ticket_views::delete(&pool, &second.id).await.unwrap_err();
    assert_eq!(gone.code, IpcErrorCode::NotFound);

    let back = ticket_views::save(
        &pool,
        TicketViewDraft {
            id: Some(renamed.id.clone()),
            position: Some(renamed.position),
            ..draft(&renamed.name, &renamed.query)
        },
    )
    .await
    .unwrap();
    assert_eq!(back, renamed);
    assert_eq!(
        listed(&pool, &ids).await.iter().map(|row| row.view.id.clone()).collect::<Vec<_>>(),
        [first.id, second.id, third.id]
    );
}

/// A display word the Tickets screen does not know is `invalid`, and nothing
/// is written.
#[tokio::test]
async fn a_word_the_screen_does_not_know_is_refused() {
    let pool = pool().await;
    let p = project();
    let error = ticket_views::save(
        &pool,
        TicketViewDraft {
            lanes: "who".to_owned(),
            ..draft(&format!("{p} lanes"), "")
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, IpcErrorCode::Invalid);
    let written: i64 = sqlx::query_scalar("select count(*) from knobas.ticket_view where name = $1")
        .bind(format!("{p} lanes"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(written, 0);
}
