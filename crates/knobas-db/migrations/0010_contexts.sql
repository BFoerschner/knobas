-- 0010_contexts.sql -- contexts become writable, so their vocabulary and
-- their anchor uniqueness stop being comments.
--
-- Single-writer (orchestrator), like every migration: a stream that needs
-- more schema requests 0011 and never edits this file or its predecessors --
-- sqlx checksums applied migrations and an edit fails startup on every
-- existing database.
--
-- `knobas.context` has existed since 0001 and nothing has ever written it:
-- M1's rooms are derived (`contexts.ts` mints `all` and `src:<id>` only).
-- #47 makes the table real -- promote ticket -> context, ad-hoc contexts --
-- and the two invariants its code relies on move into the schema, where a
-- writer that forgets them is refused instead of trusted.
--
-- 1. `kind` carries a closed list that 0001 wrote in a comment
--    (`epic|ticket|adhoc`). The list also exists as
--    `knobas_core::context::ContextKind`, whose decoder *refuses* a spelling
--    it does not know, so a value written outside the list is not a label
--    that looks wrong -- it is a context that can never be read back. Same
--    discipline, same shape, as `link_origin_chk` (0003).
--
--    Keep the spellings on one line: `knobas_core::context`'s cross-check
--    reads this file and finds the vocabulary by the line that lists it, so
--    that neither list can grow without the other.
--
-- 2. Promoting is idempotent -- "promote PAY-231" twice is one context, not
--    two rooms racing to be it. The code path checks before it inserts, but a
--    check-then-insert is two statements, and the index below is what makes
--    the promise hold between them. Partial on `archived_at is null` so an
--    archived promotion can be re-promoted fresh, and on `anchor_id is not
--    null` because ad-hoc contexts have no anchor to be unique about.
--
-- Additive and re-entrant, the same discipline as 0003 and 0007: adding a
-- CHECK validates the rows already there (there are none anywhere yet), and
-- the index is new.
alter table knobas.context
  add constraint context_kind_chk
  check (kind in ('epic','ticket','adhoc'));

create unique index context_anchor_idx on knobas.context (anchor_id)
  where anchor_id is not null and archived_at is null;
