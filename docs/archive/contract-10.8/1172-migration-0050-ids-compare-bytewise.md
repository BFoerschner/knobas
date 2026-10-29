- **Migration `0050` — issue #1172 (2026-10-18): every column that holds an entity id takes the
  `C` collation, and `0047`'s five second indexes go.** Backlog follow-up of #1163 (PR #1171).
  ADR-0027. No command, no IPC schema change, no event, no DTO field, no `WriteOp`, no grammar
  key, no new module in either barrel or in `commands/`. What every read answers is unchanged,
  except that a sort by id alone is bytewise (no such sort is one a user reads).

  **The migration.** No earlier migration edited.
  - Drops `entity_id_bytes_idx`, `link_from_bytes_idx`, `link_to_bytes_idx`,
    `item_entity_bytes_idx` and `item_parent_entity_bytes_idx` (`0047`). The keys' own indexes
    are bytewise now.
  - `alter column ... type text collate "C"` on 34 columns: `knobas.entity.id`, every column a
    chain of single-column foreign keys leads from to it (21 with `entity.id`), and 13 entity-id
    columns with no foreign key. `knobas-db`'s
    `every_entity_id_compares_bytewise_and_a_title_by_the_database` holds the list.
  - Drops and creates again `sync.live_item`, `knobas.confirmed_link` and
    `knobas.proposed_link`, column for column as they were: PostgreSQL will not change a column
    under a view.
  - A stored generated column of `sync.item` that reads `entity_id` (`fts`, since `0048`) is
    dropped with its indexes and added back with the same expression and indexes, read from the
    catalog in a `do` block: PostgreSQL will not change a column under a generated one.

  **Why.** `initdb` takes the database's collation from its environment, `en_US.UTF-8` on every
  machine measured, and a btree probe on an id calls `strcoll` at each step: about 4.7 µs a probe
  at 22k items against 0.75 µs bytewise. `0047` served only `has:`'s walk, which had to write
  `collate "C"` on both sides. With the column itself in `C`, every read's `=` on an id is bytewise
  and uses the key's own index. Equality is unchanged, both collations being deterministic.

  **Cost.** No table is rewritten for a collation change; every index on a changed column is
  rebuilt and the foreign keys are re-checked. On the 22k probe's mirror (23,500 entities,
  42,000 parent rows) the file ran in 1.70 s with `0048` applied, the drop and re-add of `fts`
  rewriting `sync.item` (the same `fts` for every row, by md5), and in 0.86 s without it. Each write saves the
  second btree insert `0047` cost.

  **The readers.** `knobas_core::signals` compares with plain `=` (`same_key!` and
  `live_by_bytes!` are gone; `kind_of!`, `item_by_key!` and `link_ends!` stay as probes) and hands
  nothing back `collate "default"`. `knobas-app`'s build tile (`LIVE_LATEST`) likewise.
  `knobas_core::link_set_rule_tickets!` casts its recursive CTE's first term `collate "C"`, which
  PostgreSQL requires once the recursive term reads an id column.

  Pinned by `knobas-db`'s `tests/schema.rs`
  (`every_entity_id_compares_bytewise_and_a_title_by_the_database`), `knobas-app`'s
  `tests/it/read_plans.rs` (every planned read, `has:`'s walk included, in both statistics states:
  a probe without an index is a scan) and the link-set suites (`link_sets_ipc`,
  `link_set_patterns_ipc`, `backup_ipc`), which fail at plan time without the cast.
