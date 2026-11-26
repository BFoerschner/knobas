//! A search's future stays small (the ux21 Windows pass, 2026-11-19;
//! `docs/decisions/2026-11-19-a-search-fits-the-windows-main-thread.md`).
//!
//! On Windows a Tauri command's future is moved by value on the main thread,
//! whose stack is 1 MB, before it is spawned. ux21's launcher search future
//! was 37 kB, its frames there 444 kB, and the first letter typed into the
//! launcher ended the process with a stack overflow. Building a future polls
//! nothing, so no database is needed: the pool is never connected.

use knobas_search::picker::Picker;
use knobas_search::{FUTURE_MOST, SearchFilters, SearchQuery, Searcher};

fn searcher() -> Searcher {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://knobas@127.0.0.1:1/knobas")
        .unwrap();
    Searcher::new(pool).with_picker(Picker::default())
}

#[tokio::test]
async fn the_launcher_search_future_is_small() {
    let searcher = searcher();
    let query = SearchQuery {
        titles: false,
        raw: "p".to_owned(),
        limit: 40,
        filters: SearchFilters::default(),
    };
    let future = searcher.search(query);
    let bytes = std::mem::size_of_val(&future);
    assert!(
        bytes <= FUTURE_MOST,
        "the launcher's search future is {bytes} bytes, over {FUTURE_MOST}"
    );
}

#[tokio::test]
async fn the_docs_search_future_is_small() {
    let searcher = searcher();
    let future = searcher.page_search("p", None, 40);
    let bytes = std::mem::size_of_val(&future);
    assert!(
        bytes <= FUTURE_MOST,
        "the Docs search future is {bytes} bytes, over {FUTURE_MOST}"
    );
}
