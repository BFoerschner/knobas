-- 0071_a_readers_tone_for_a_state_word.sql -- one table, and nothing else
-- (#2465, spec #2463).
--
-- A **tone override** is the reader's choice of tone, or of no colour, for
-- one state word of one entity kind: `FAILURE` on `build` drawn `warn`,
-- `Blocked` on `ticket` drawn `err`, `running` on `build` drawn with no tone.
-- Keyed by kind and word, so it holds for every source of the kind whichever
-- adapter it comes from (a Gitea and a mock `pr` alike). A missing row is no
-- override, and the adapter's tone stands.
--
-- `tone` is one of the five tones, or null for *No colour*: the word still
-- matches its candidate and draws its text, and the walk over candidates
-- stops there.
--
-- Read by `knobas_core::state_colours`, which merges it into the
-- declarations every read binds. It is never written into
-- `sync.parent_declaration`, so a change here recomputes no stored table.
-- Being in the `knobas` schema, it travels in the backup.

create table knobas.state_tone (
  kind   text not null,
  word   text not null,
  tone   text,
  set_at timestamptz not null default now(),
  primary key (kind, word),
  constraint state_tone_kind_chk check (btrim(kind) <> '' and kind = btrim(kind)),
  constraint state_tone_word_chk check (btrim(word) <> '' and word = btrim(word)),
  constraint state_tone_tone_chk check (tone in ('ok', 'warn', 'err', 'info', 'muted'))
);

comment on table knobas.state_tone is
  'A reader''s tone override (#2465): this kind''s state word draws this tone, or no colour for a null tone. Per entity kind, across every source of it. Knobas-owned.';
