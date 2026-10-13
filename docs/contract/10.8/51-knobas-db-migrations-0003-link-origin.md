- `crates/knobas-db/migrations/0003_link_origin.sql`, issue #51 (2026-08-28): the first migration after
  the freeze, and the only schema change Links v1 (#40) asks for -- "adds only a CHECK constraint
  closing the origin vocabulary, matching the repo's precedent for closed text vocabularies,
  cross-checked against the Rust enum by a schema test. Nothing else changes in the schema."
  `link_origin_chk` closes `knobas.link.origin` to the five spellings `0001` wrote in a comment and
  left unenforced -- the same discipline `source_config_auth_state_chk` and the run log's two
  vocabularies got in `0002`, and it bites harder here, because `knobas_core::link::Origin`'s decoder
  *refuses* a spelling it does not know: an unlisted value is a link that can never be read back, not
  a label that looks wrong. Additive, and the boot path depends on that: `ALTER TABLE ... ADD
  CONSTRAINT` validates the rows already there, and knobas has never written an origin outside the
  five, which a test proves by winding a scratch database back to before `0003`, filling it with one
  link of every origin and letting `migrate::run` apply the migration to it for real. Pinned from
  three sides -- that test and the live-catalog one in `knobas-db`'s schema battery, and `Origin::ALL`
  walked against this file in `knobas_core::link`. Ratified by the
  orchestrator as issue #51 itself, which specifies the migration and its acceptance criteria.
  **`0004` is the next free number**; `0001`-`0003` are never edited.

  Outside the frozen list, and noted here only because it is what makes the cross-check honest:
  `closed_vocabulary!` moved from `knobas-sync` to `knobas-core` (the crate both sides depend on) and
  `Origin` is now declared with it, so its `ALL` is generated from the same variant list as the enum
  rather than hand-written beside it. No wire spelling, column value or DTO shape changes.

