- **Migration `0058`: two partial expression indexes on `knobas.entity` — issue #1850
  (2026-10-23): `reach:`'s start set is found through an index.** No IPC change, no command, no
  event, no DTO field, no grammar key, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added to either append-only
  barrel or to `commands/` / `ipc/`. What `reach:` answers is unchanged. Reserved for Björn and
  ruled by the implementer on Opus under his 2026-10-22 waiver; the ruling is in
  `docs/decisions/2026-10-unattended-rulings.md` under `## #1850, #1851 and #1855`.

  **The migration** (`0058_a_ticket_is_found_by_its_key_in_any_case.sql`). Two btree indexes,
  each `where kind = 'ticket'`, nothing dropped and no earlier migration edited:
  `entity_ticket_id_lower_idx` on `knobas.entity (lower(id))` and `entity_ticket_key_lower_idx`
  on `knobas.entity (lower(substr(id, strpos(id, ':') + 1)))`. The number follows #1864's
  `0057`.

  **Why.** `knobas_search::sql::reaches` starts the walk from every ticket whose id or key (the
  id past its namespace) is one of the typed values, both compared lowercased, since the values
  arrive lowercased by the server (#1652). Neither the primary key nor `0047`'s
  `entity_id_bytes_idx` compares lowercased, so the start set was a scan of every entity: once
  per statement since #1653, and once per saved list holding `reach:` on the rail. On the
  responsiveness probe's 23,500 entities the scan read 5.7 ms of a 21.5 ms statement and the
  `BitmapOr` of these two reads 0.04 ms (14.4 ms in all). On the search crate's 114,803 it was
  10.6 ms of 16.5 ms, against 0.04 ms of 3.9 ms.

  **The reader.** `reaches` spells both halves exactly as the indexes do; its doc comment says
  so. No other read compares either expression.

  **Write cost.** Two btree inserts per ticket written; none for any other kind.

  **Backups.** A restore loads data into a migrated schema (`knobas_db::backup`), which builds
  the indexes, so no archive carries them and `RENAMED_COLUMNS` needs no row.

  Pinned by `crates/knobas-search/tests/it/reach.rs`' `the_start_set_is_found_through_an_index`:
  over 4,000 entities, never analyzed and analyzed, the plan's start set has no sequential scan,
  reads the `entity_ticket_*` indexes, and finds the two tickets a typed id and a bare key name.
