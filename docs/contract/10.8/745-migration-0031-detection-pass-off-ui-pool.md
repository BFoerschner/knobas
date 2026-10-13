- **Migration `0031` — issue #745 (2026-10-13): the suggestion pass runs one at a time, off the UI
  pool, and `similar_text` compares only what changed.** Signed off under Björn's 2026-10-13
  delegation. No IPC change: `detect_suggestions` keeps its name, its arguments (none) and its
  `u32` answer.

  **Why.** The 2026-10-13 hang on Windows ("searches didn't find anything and it said there were no
  sources available anymore") reproduced on macOS: every finished sync made the tray run a
  detection pass on the five-connection UI pool with nothing stopping the next one, and
  `similar_text` compared every prose item with every other (55 s at 2,000 items, the same again
  with nothing new). `just ui-pool-probe` (five sources × 1,000 items, the real scheduler): the UI
  pool had no free connection in 173 of 179 one-second samples and 32 UI calls failed with a pool
  timeout before; 0 of 180 and none after.

  **Migration `0031`: one table, one index.** `sync.similar_text_seen (entity_id, digest, title_q,
  title_lex)`, `entity_id` referencing `sync.item` `on delete cascade`, and
  `similar_text_seen_lex_idx`, GIN on `title_lex`. A row says "this prose item was compared with
  every item beside it, with this text": `digest` is `md5(title || E'\n' || fts::text)`, `title_q`
  the stored title OR-query, `title_lex` every lexeme that query can match on. In `sync`, not
  `knobas`: it is derived from the mirror, rebuilt by one full pass if empty, and out of backups
  like the mirror (§16.12). No earlier migration is edited.

  **`knobas_core::suggest`.** `similar_text` judges only pairs with at least one end among items
  whose digest changed (or that have no row), in batches of `SIMILAR_TEXT_BATCH` (100), each batch
  and its marking one `repeatable read` transaction that also forgets items that left the live
  prose corpus. The set of proposals is unchanged: `similar_text_incremental.rs` holds batched
  passes (batch 1, 4, 1,000) to the pre-#745 statement, verbatim, over arrivals, rewrites,
  tombstones, a restore and a disabled source. `detect`/`detect_rule` take any `sqlx::Acquire`
  (callers passing `&PgPool` are unchanged); `detect_on(&mut PgConnection)` is new, for the app's
  pass. A `Rule` gained a private `scope`.

  **`knobas-app`.** `suggestion_pass::SuggestionPass`, on `AppState` as `suggestions`: at most one
  pass runs and one more is owed, every request during a pass is answered by the next one; a pass
  opens its own connection (the connector, as a sync run does) with a 120 s `statement_timeout`
  (`PASS_STATEMENT_TIMEOUT`), and a cancelled statement is logged and answered with an `Internal`
  error. `detect_suggestions_inner` now takes the pass instead of a pool. `Lifecycle::suggestions()`
  is new.

  **`knobas-db`.** `Connector::app_pool()` is the app's pool, its connections carrying
  `UI_STATEMENT_TIMEOUT` (30 s) from the first packet; the three places `EmbeddedDb` built its pool
  use it. `migrate::run` runs on a one-connection pool of its own with no limit, so a long
  migration is not cancelled and the cleared limit never enters the app pool. Two app-pool callers
  that grow with the mirror lift the limit for themselves (review, 2026-10-13): `knobas_sync::stats::reindex_fts`
  runs on a connection detached from the pool, and `knobas_search::Searcher::declaring` sets it
  `local` to its transaction (the declarations' recompute: 7.6 s at 100,000 items). No signature
  changes.

  **Frontend.** `SuggestionTray.svelte` asks for a pass only on a finished sync run; opening the
  tray, switching rooms and a link changing elsewhere read.

  **Not here:** logging every pool acquire timeout with the command's name is #747's (the same
  requirement, in its ticket); it landed with the app's log (`knobas-app/src/logging.rs`) before
  this change, which adds nothing beside it.
