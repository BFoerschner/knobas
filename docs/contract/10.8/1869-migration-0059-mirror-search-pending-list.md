- **Migration `0059` — issue #1869 (2026-10-23): `item_fts_idx` merges its pending list at
  256 kB.** No IPC change, no command, no event, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added to either append-only barrel
  or to `commands/` / `ipc/`. Ruled by the implementer on Opus under Björn's 2026-10-22 waiver;
  the ruling is in `docs/archive/2026-10-unattended-rulings.md` under `## #1869`.

  **The migration** (`0059_the_mirror_search_keeps_a_short_pending_list.sql`). `alter index
  sync.item_fts_idx set (gin_pending_list_limit = 256)`, then
  `gin_clean_pending_list('sync.item_fts_idx')`, which merges what an upgraded mirror has
  pending. No table rewrite, no index rebuild, no new object. `fastupdate` stays on. The
  re-index button's `reindex index concurrently` keeps the setting.

  **Why.** `0001`'s index took the default 4 MB limit. The planner charges every pending page as a
  random read, and a manual `analyze` merges nothing. The responsiveness probe's mirror (23,500
  items, never analyzed) was left with 367 pages pending after its sync, and `payment retry`
  walked every entity instead of reading the index: 25 to 36 ms, against 2.0 to 2.6 ms merged.
  The plan flipped between 213 and 225 pages. At 256 kB the list holds at most 33 pages. Writing
  23,500 items' `fts` through the index alone costs 1.6 to 2.7 s against 0.6 to 1.0 s at 4 MB;
  `fastupdate = off` cost 3.6 to 9.8 s.

  **Not capped.** `note_fts_idx` (`0001`), `asset_fts_idx` (`0017`) and `route_fts_idx` (`0018`)
  keep the default. Read on an unvacuumed database at the default limit and at 256 kB, with
  10,000 notes, 3,303 assets and 1,500 routes, the lists cost 0.1 to 0.4 ms a search.

  **Backups.** A backup dumps the `knobas` schema only (`knobas_db::backup::OWNED_SCHEMA`); the
  index and its storage parameter are on the mirror and in no archive. `RENAMED_COLUMNS` needs no
  row.

  Pinned by `crates/knobas-db/tests/schema.rs`'
  `zero_fifty_nine_caps_the_mirror_search_pending_list`: read after every 20 rows, the list holds
  at most 33 pages through a load the default limit lets reach 334, and the migration rerun over
  such a list merges it.
