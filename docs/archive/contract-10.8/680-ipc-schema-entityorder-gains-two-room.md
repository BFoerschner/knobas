- **The IPC schema — issue #680 (2026-10-12): `EntityOrder` gains the two room tile orders, and
  `EntityPage` counts what they drew.** Spec #660's problem statement and story 15: a room's Code
  and Builds tiles show what needs attention, not the newest rows of every kind. The ticket's own
  criterion 5 routes any row-shape or command change here under #660's delegation of 2026-10-12;
  this is that entry.

  **`EntityOrder` (`list_entities`' filter), two members, additive:**

  1. `attention`. A row whose declared state chip (#666) has tone `ok` or `muted` (settled: a
     merged or a closed pull request) is not in the set. The rest are ordered by tone, `err`,
     `warn`, `info`, then rows with no state, each run `item_updated_at desc nulls last,
     entity_id`.
  2. `latest_per_parent`. For a row whose kind declares a state
     (`knobas_core::declares_field!`, new beside `declared_state!`: the declaration names at
     least one candidate), rows are grouped by `(source_id, kind, parent key)`, the parent key
     being every name of every parent the declared chain names, in order
     (`array_agg` over `declared_parent_names!`), and only the newest of each group is kept. A
     kind that declares no state and a row whose parent misses are not grouped. Every tone is
     kept, ordered `err`, `warn`, `info`, `ok`, `muted`, then no state, each run newest first.

  Both are driven by the declared state, parent and tone only; no kind or adapter is named in
  either statement. A kind with no declared state reads exactly as `updated_desc` under both,
  which is §3a's fallback, and the ADR-0007 miss fails toward drawing a row, never hiding one.
  Both read `sync.live_item` only: with `include_deleted: true` they are refused as `invalid`.
  The statements are two more `&'static str` constants in `commands/entity.rs`, chosen by the
  same `match`, with the same five nullable filters and the declarations at `$8`. Unlike the plain
  orders they are sent as the unnamed statement (`persistent(false)`), so PostgreSQL plans each
  read with its values: a named one went generic at its sixth read of a connection and took the
  Builds tile of a 20,000-build room from 0.45 s to 11-12 s (#685's review).

  **`EntityPage.tally: KindTally[] | null`**, with `KindTally { kind: string, tone: Tone | null,
  count: number }`: under a tile order, the whole ordered set counted by kind and tone (not the
  page, as `total` is not), `[]` for an empty page; `null` under the two plain orders. The tile
  header phrases it (*3 open pull requests · 5 branches*, *2 failing · 7 build
  configurations*). The TypeScript mirror is `app/src/lib/ipc/entity.ts`; `entity_mirror.rs`'s
  `the_entity_page_shape_matches_its_typescript_mirror` pins both shapes (the null and a tally
  with a toneless line) and `the_entity_orders_match_their_typescript_mirror` the union.

  No command, event, module, barrel or migration moves. The shell: `kinds.ts` gives each tile an
  `order` (Code `attention`, drawing `pr` and `branch` while still claiming `commit` and `repo`;
  Builds `latest_per_parent`; every other tile `updated_desc`), a maximised tile reads the same
  order with a longer page, and `tile-count.ts` phrases the header.

  Pinned by `crates/knobas-app/tests/it/entity.rs`:
  `the_code_tile_draws_open_pull_requests_first_and_no_settled_one`,
  `the_builds_tile_draws_each_configurations_latest_build_failing_first`,
  `a_kind_with_no_declared_state_falls_back_to_recency`,
  `the_tile_orders_refuse_the_withdrawn` and
  `the_tile_orders_are_never_named_prepared_statements`, over real PostgreSQL with the built-in Gitea and
  TeamCity declarations; and the frontend's `kinds.test.ts`, `tile-count.test.ts` and
  `Tile.test.svelte.ts`.

  *Amended 2026-10-12 (#688): `latest_per_parent`'s parent key is read from `sync.item_parent`,
  not walked per row.* The walk through `declared_parent_names!` was about 300 ms of a 310 ms
  Builds tile on a 20,000-build room. The key is now the row's child-arm rows in
  `sync.item_parent` (migrations `0028`/`0029`, ADR-0019) under its own source's adapter, matched
  on `knobas.source_config` as every reader of that table matches it. Those rows are a set, so the
  key is the **sorted `(parent kind, name)` pairs**, not the chain's names in order. Two
  configurations that share a display name stay two because each also names its own id. What a
  set cannot tell apart is the same pairs arranged differently (one configuration named `A` with
  id `B`, another named `B` with id `A`). Whether a kind declares a state is asked once per
  source and kind. A row that names no parent stands alone, as one whose parent missed did.
  Nothing on the wire moves: no `EntityOrder` member, no field, no command, no migration. The
  statement is still sent unnamed. Pinned by the tests above, unchanged, and by
  `a_builds_configuration_is_the_parent_its_own_adapter_says_it_names` (a row another adapter
  wrote, or a `parent`-arm row, does not split a configuration). `entity.rs`'s `seeded()` now
  writes the stored declarations first, as bring-up does. The tile now depends on that write:
  bring-up makes it before the first sync and before the pool is handed to any command, and if it
  fails (logged, not fatal) every build draws ungrouped -- too many rows, never too few. Measured
  on one connection, five reads each: before 298-318 ms, after about 80 ms, against 23-30 ms for
  `updated_desc`.

