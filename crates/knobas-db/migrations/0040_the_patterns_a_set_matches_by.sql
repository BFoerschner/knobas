-- 0040_the_patterns_a_set_matches_by.sql -- one column on
-- `knobas.link_set_member`, and nothing else (issue #868, spec #808 *Link
-- sets*, requirements R-7.2, R-7.7, R-7.11).
--
-- Single-writer, like every migration: a later ticket that needs more schema
-- takes the next number and never edits this file. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/contract/10.8/868-1008-migration-0040-link-set-patterns-and-member-rows.md`).
--
-- ## Why a second column beside `query`
--
-- `0039` made room for a pattern: a member whose `query` is a stored query in
-- the search grammar (`project:Payouts name:Payout_*`). The job reads a set's
-- members in one static SQL text (`knobas_core::reach_walk!`), shared by the
-- job's read and the `reach:` filter, and SQL cannot parse the grammar: the
-- grammar is parsed in one place, `knobas_search::query` (ruling P2). So the
-- save parses the query once and stores what it read here, as jsonb
-- (`knobas_search::Pattern`): `{"sources": [...], "projects": [...],
-- "under": [...], "names": [...]}`, each lowercased. The read matches that
-- (`knobas_core::link_set_pattern_hits!`) when it runs, so a pipeline that
-- appears in the project later is on every job the set applies to without the
-- set being edited.
--
-- `query` stays what the member is and what the editor shows; `pattern` is
-- its reading, written only beside it by `save_link_set`. A fixed member has
-- neither.

alter table knobas.link_set_member add column pattern jsonb;

alter table knobas.link_set_member
  add constraint link_set_member_pattern_with_query_chk
  check ((query is null) = (pattern is null));

comment on column knobas.link_set_member.pattern is
  'A pattern member''s query as the grammar read it when it was saved (#868): sources, projects, under and names, each lowercased; matched when a job is read.';
