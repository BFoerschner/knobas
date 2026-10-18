- **Migration `0049` — issue #1170 (2026-10-18): a trigram index over the prose kinds' bodies,
  and `pg_trgm`, the first extension knobas installs.** Follow-up of #1164. No new command, no
  IPC schema change, no event, no `WriteOp`, no grammar key, no new module in either barrel or in
  `commands/`.

  **The migration.** `create extension if not exists pg_trgm` and a GIN index
  `item_body_trgm_idx` on `sync.item (body_text gin_trgm_ops) where kind in ('ticket', 'pr',
  'page', 'note')`, the mention rule's own kind list. `pg_trgm` ships in the bundled PostgreSQL's
  contrib. The backup (`pg_dump --schema knobas`) takes neither: the index is on the mirror.

  **The behaviour.** `knobas_core::inbox`'s mention rule looks for every name of `$1` at once in
  one expression with no subquery, which the index answers and the planner costs by the names'
  trigrams, and then decides with the per-name expression it had before. The items are the same
  for every reader; an empty `$1` still mentions nobody. On the 22k probe snapshot the rule goes
  from 12 ms to under 1 ms, and the stream for a reader who owns nothing from 36 to about 24 ms.
  A full rewrite of every prose body costs about 60 µs more a row (1.4–1.7 s → 2.6 s for 18,500
  rows).

  Pinned by `knobas-core`'s `a_name_with_pattern_characters_is_matched_literally` (and the
  existing mention tests) and by `knobas-app`'s `read_plans.rs`, which requires the mention rule
  to read `item_body_trgm_idx` in both statistics states.
