- **Migration `0058`: two expression indexes on `knobas.entity` — issue #1850
  (2026-10-23): `reach:`'s start set is found through an index.** No IPC change, no command, no
  event, no DTO field, no grammar key, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added to either append-only
  barrel or to `commands/` / `ipc/`. What `reach:` answers is unchanged. Reserved for Björn and
  ruled by the implementer on Opus under Björn's 2026-10-22 waiver, then ruled again by the
  merge-manager on Opus (the indexes cover every entity, not only tickets); both rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1850, #1851 and #1855`.

  **The migration** (`0058_a_ticket_is_found_by_its_key_in_any_case.sql`). Two btree indexes
  over every entity, nothing dropped and no earlier migration edited: `entity_id_lower_idx` on
  `knobas.entity (lower(id))` and `entity_key_lower_idx` on
  `knobas.entity (lower(substr(id, strpos(id, ':') + 1)))`. The number follows #1864's `0057`.

  **Not partial.** The PR first made both `where kind = 'ticket'`. A partial index is also the
  list of every ticket, and on tables never analyzed the planner read that list whole, with no
  condition, once per key found, to stand in for `kind = 'ticket'` in the suggestion pass's
  four exact-key rules (`ticket_keyed!`): #770's quadratic shape, refused by `knobas-core`'s
  `suggest_plans::every_pass_statement_plans_by_index_on_tables_never_analyzed`. An index over
  every entity can be probed only through the two expressions, and only `reaches` spells them.

  **Why.** `knobas_search::sql::reaches` starts the walk from every ticket whose id or key (the
  id past its namespace) is one of the typed values, both compared lowercased, since the values
  arrive lowercased by the server (#1652). The primary key (bytewise since `0050`, which dropped
  `0047`'s `entity_id_bytes_idx`) does not compare lowercased, so the start set was a scan of
  every entity: once per statement since #1653, and once per saved list holding `reach:` on the
  rail. On the
  responsiveness probe's 23,500 entities the scan read 5.7 ms of a 21.5 ms statement and the
  `BitmapOr` of these two, in their first, partial form, read 0.04 ms (14.4 ms in all). On the
  search crate's 114,803 it was 10.6 ms of 16.5 ms, against 0.04 ms of 3.9 ms.

  **The reader.** `reaches` spells both halves exactly as the indexes do; its doc comment says
  so. No other read compares either expression.

  **Write cost.** Two btree inserts per entity written, of any kind, and two btree builds over
  `knobas.entity` at the boot that applies `0058` (not timed).

  **Backups.** A restore loads data into a migrated schema (`knobas_db::backup`), which builds
  the indexes, so no archive carries them and `RENAMED_COLUMNS` needs no row.

  Pinned by `crates/knobas-search/tests/it/reach.rs`' `the_start_set_is_found_through_an_index`:
  over 4,000 entities, never analyzed and analyzed, the plan's start set has no sequential scan
  and drops no row it read, probes an index on each half (an `Index Cond` on `lower(id)` and one on the
  lowercased key), and finds the two tickets a typed id and a bare key name.
