-- 0096_the_state_words_the_mirror_has_seen.sql -- the words a source's items
-- hold at an open state candidate, counted and kept (the state-colours bug of
-- PR #2479, 2026-11-17; spec #2463).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Settings › Appearance › State colours offers every status the mirror has
-- seen on a ticket. The read found them by walking every live ticket's
-- payload through the declarations, once per ticket: 2.5 s over Björn's
-- 20,007 Jira tickets on a quiet Mac, 11 to 14 s on a loaded one, for four
-- words. A set and a reset answered with the same read, as did the Backup
-- section and the launcher's legend. Björn's rule: directness and speed over
-- storing little.
--
-- ## What is stored
--
-- * `sync.state_word_seen`: per source, kind and word, how many of the
--   source's live items hold the word, split by the tone those items fall
--   back to (Jira's status category, `''` for none). Across sources the read
--   takes the most common tone, as the walk's `mode()` did.
-- * `sync.state_word_source`: per source, the open candidates and fallback
--   declarations the count was made under, so a source counted under other
--   declarations (an adapter that changed them) is counted again.
--
-- ## When it is written
--
-- * **After a run of the source that wrote to the mirror**, and after an
--   item refresh, in the background (`knobas-app`'s `sources::seen_words`).
-- * **On a read** that finds a source never counted, or counted under other
--   declarations, before it answers; bring-up asks for that read in the
--   background, so a first open after this migration finds it done.
-- * Removing the source cascades. A backup rebuilds it, never carries it:
--   the `sync` schema is not in an archive (section 16.12).
--
-- ## What it costs
--
-- Nothing at the migration: two empty tables. A count is one pass over the
-- source's items of a kind with an open candidate, grouped before any tone
-- is resolved: 270 ms over 20,007 Jira tickets.

create table sync.state_word_seen (
  source_id text   not null references knobas.source_config (id) on delete cascade,
  kind      text   not null,
  word      text   not null,
  tone      text   not null,
  items     bigint not null,
  primary key (source_id, kind, word, tone),
  constraint state_word_seen_tone_chk check (tone in ('', 'ok', 'warn', 'err', 'info', 'muted'))
);

create table sync.state_word_source (
  source_id  text        primary key references knobas.source_config (id) on delete cascade,
  paths      jsonb       not null,
  counted_at timestamptz not null default now()
);
