- **Migration `0047` — issue #1163 (2026-10-18): a second index on each key the signals' walk
  probes, ordered bytewise.** Backlog follow-up of #1148 (PR #1161). No command, no IPC schema
  change, no event, no DTO field, no `WriteOp`, no grammar key, no new module in either barrel or
  in `commands/`. What `mini_board` and `search` answer is unchanged.

  **The migration.** Five expression indexes, nothing dropped and no earlier migration edited:
  `entity_id_bytes_idx` on `knobas.entity ((id collate "C"))`; `link_from_bytes_idx` and
  `link_to_bytes_idx` on `knobas.link ((from_id collate "C"))` and `((to_id collate "C"))`, each
  `where deleted_at is null` like `link_from_idx` and `link_to_idx`; `item_entity_bytes_idx` on
  `sync.item ((entity_id collate "C"))`; `item_parent_entity_bytes_idx` on `sync.item_parent
  ((entity_id collate "C"))`.

  **Why.** Every id is `text` in the database's collation, which `initdb` takes from its
  environment: `en_US.UTF-8` on the machines measured, the app's own database included. A btree
  probe on such a key calls `strcoll` at each step. On the 22k probe's mirror one probe of
  `knobas.entity` by id took about 4.7 µs, and 0.75 µs on an index ordered `collate "C"` (12,001
  probes: 56 ms against 14 ms). `has:failing-build` and `has:blocker` are a few thousand probes
  each, with nothing left to narrow (#1148). Both collations are deterministic, so equality is the
  same: two ids are equal exactly when their bytes are.

  **The reader.** `knobas_core::signals`' walk (`ticket_signals!`, `signal_ends!`, `err_builds!`,
  `open_prs!`) compares each key it probes `collate "C"` on both sides (`same_key!`), so these
  indexes serve it. It sorts the sets it deduplicates `collate "C"` too, and hands them back in the
  database's collation. No other read compares through them. Separately,
  `knobas_search::sql::ids_sql` (the board's matching ids) reads `sync.item` by
  `live_by_exclusion!` rather than through `sync.live_item`, so its plan no longer depends on
  how costly the walk looks. A probe written this way on a database without
  the index is a scan per row, and `knobas-app`'s `read_plans.rs` plans `has:`'s walk to catch
  that.

  **Write cost.** One more btree insert per entity, item and parent row, and two per link, each
  comparing bytewise.

  Pinned by `knobas-app`'s `tests/it/read_plans.rs` (`has: walk`, both statistics states: each
  dropped index is reported as a scan or an unkeyed walk per row) and `tests/it/signals_ipc.rs`
  (the read and `has:` hold one answer; `a_room_shares_no_build_and_a_withdrawn_blocker_blocks_nothing`),
  and `knobas-core`'s `tests/it/payload_paths.rs` (the state chip, a map naming a value twice
  included).
