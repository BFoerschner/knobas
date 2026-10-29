- **Migration `0057` — issue #1864 (2026-10-23): `item_body_trgm_idx` merges its pending list at
  256 kB.** No IPC change, no command, no event, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added to either append-only barrel
  or to `commands/` / `ipc/`. Ruled by the implementer on Opus under Björn's 2026-10-22 waiver;
  the ruling is in `docs/archive/2026-10-unattended-rulings.md` under `## #1864`.

  **The migration** (`0057_a_mention_index_keeps_a_short_pending_list.sql`). `alter index
  sync.item_body_trgm_idx set (gin_pending_list_limit = 256)`, then
  `gin_clean_pending_list('sync.item_body_trgm_idx')`, which merges what an upgraded mirror has
  pending. No table rewrite, no index rebuild, no new object. `fastupdate` stays on.

  **Why.** `0049`'s index took the default 4 MB limit. The planner charges every pending page as a
  random read, a manual `analyze` merges nothing, and autovacuum may not have reached a freshly
  synced mirror. On `read_plans.rs`' mirror a load that ended with 411 pages pending priced the
  mention rule's index read at about 1,700, above a scan of every prose body, and the rule read
  every body (4.8 ms, about 1,100 to 1,350 buffers) where the merged index answers in 0.1 ms.
  At 256 kB the list holds at most 33 pages. Indexing 5,000 bodies row by row costs 0.5 to 0.7 s
  against 0.3 to 0.4 s at 4 MB; `fastupdate = off` cost 3.5 to 10.8 s.

  **Backups.** A backup dumps the `knobas` schema only (`knobas_db::backup::OWNED_SCHEMA`); the
  index and its storage parameter are on the mirror and in no archive. `RENAMED_COLUMNS` needs no
  row.

  Pinned by `crates/knobas-db/tests/schema.rs`'
  `zero_fifty_seven_caps_the_body_trigram_pending_list` (read after every 20 rows, the list holds at
  most 33 pages through a load the default limit lets reach about 480, and the migration rerun over
  such a list merges it), and by `crates/knobas-app/tests/it/read_plans.rs`, whose mirror now
  carries the probe's common word and whose mention rule must read `item_body_trgm_idx` in both
  statistics states.
