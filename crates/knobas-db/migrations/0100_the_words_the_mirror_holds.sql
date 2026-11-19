-- 0100_the_words_the_mirror_holds.sql -- every word the mirror's titles and
-- bodies hold, as written, so a word nobody wrote can be read as the nearest
-- one somebody did (the fuzzy-search pass, 2026-11-19, D10; PR #2479;
-- ADR-0045; `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- PostgreSQL's words find a word only from its start and only as typed:
-- `shceduler`, `payuot` and `idempotncy` find nothing, and `proxy` does not
-- find a page that says `ADProxy`. The launcher reads a typed word against
-- a list of the words the mirror holds, in memory
-- (`knobas_search::picker::Picker::reading`), and asks its statements for the
-- real words it finds: the nearest written word for one nobody wrote, and
-- the longer words a typed word is part of. This table is that list.
--
-- ## What is stored
--
-- **A word** is a token of `to_tsvector('simple', title || ' ' || body)`
-- of 3 to 40 characters that starts with a letter and holds letters and
-- digits only (the database's ctype is C.UTF-8, so `ä` is a letter).
-- `simple` lower-cases and stems nothing: a typo is measured against a word
-- as written, which no stem is, and Rust has no stemmer. No trigram index:
-- the list is matched in memory. **Words are never taken out**: a word no
-- row holds any more matches nothing when it is asked for.
--
-- **`sync.word_whole`**, one row once every row mirrored before this
-- migration has had its words read: at once on a mirror that holds no item,
-- else from the fill's last batch. The launcher reads the list only then.
--
-- **`sync.word_filled_to`**, one row: the last entity id the fill read, in
-- id order. A word does not say which row gave it, so the fill keeps its own
-- place.
--
-- ## When it is written
--
-- * **A new item**: a statement trigger after insert, over the new rows.
-- * **An item whose words changed**: a row trigger `after update of fts`,
--   which PostgreSQL fires for an update naming a column `fts` is computed
--   from, with `when (old.fts is distinct from new.fts)`, as `0077`'s are. A
--   stamp of `synced_at` fires neither.
-- * **Every item mirrored before**: `sync.fill_words_beside`, 150 items a
--   batch from bring-up (`knobas_core::word_list::fill`), after the other
--   fills, each batch its own transaction, its items taken `for share skip
--   locked`. The place moves only past items it read: a batch stops before
--   the first item a sync holds, and the fill reads it again next time.
-- * **The mark**: `sync.mark_words_whole`, once no item is past the place.
--
-- The list depends on `sync.item` and on nothing else (ADR-0019). **A
-- backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12); the re-sync's inserts write the words.
--
-- ## What it costs
--
-- 0.095 ms a row parsed, measured on Björn's scale (39,770 titles and bodies
-- in 3.76 s), so about 0.1 s for a run that changed 1,000 rows' text, and
-- 3.8 s of one core behind the app at the first start. The scale mirror's
-- bodies are built from 130 words: 831 words. A real team's vocabulary was
-- stood in for by macOS's word list, 234,739 words, 7.9 MB in memory.

create table sync.word (
  word text collate "C" primary key
);

create table sync.word_whole (
  whole boolean primary key default true check (whole)
);
insert into sync.word_whole (whole)
select true where not exists (select from sync.item);

create table sync.word_filled_to (
  one boolean primary key default true check (one),
  entity_id text collate "C" not null
);

create function sync.words_of(title text, body text) returns setof text
language sql immutable parallel safe as $fn$
  select w
    from unnest(tsvector_to_array(to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(body, '')))) w
   where w ~ '^[[:alpha:]][[:alnum:]]{2,39}$'
$fn$;

create function sync.words_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.word (word)
  select distinct w.w
    from new_rows n
   cross join lateral sync.words_of(n.title, n.body_text) w(w)
  on conflict do nothing;
  return null;
end
$fn$;

create trigger word_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.words_inserted();

create function sync.words_changed() returns trigger
language plpgsql as $fn$
begin
  insert into sync.word (word)
  select distinct w.w from sync.words_of(new.title, new.body_text) w(w)
  on conflict do nothing;
  return null;
end
$fn$;

create trigger word_on_update after update of fts on sync.item
  for each row when (old.fts is distinct from new.fts)
  execute function sync.words_changed();

-- One batch of the fill: the next `n` items past the place, in id order,
-- up to the first one a sync holds. `read` is how many it read, `held`
-- whether it stopped at one a sync holds.
create function sync.fill_words_beside(n integer, out read integer, out held boolean)
language plpgsql as $fn$
declare
  after text;
  looked text[];
  locked text[];
  upto integer;
begin
  select f.entity_id into after from sync.word_filled_to f;
  looked := array(select i.entity_id from sync.item i
                   where i.entity_id > coalesce(after, '')
                   order by i.entity_id limit n);
  read := 0;
  held := false;
  if cardinality(looked) = 0 then
    return;
  end if;
  locked := array(select i.entity_id from sync.item i
                   where i.entity_id = any (looked)
                   order by i.entity_id
                   for share skip locked);
  -- The items before the first one a sync holds.
  upto := 0;
  while upto < cardinality(looked) and upto < cardinality(locked)
        and looked[upto + 1] = locked[upto + 1] loop
    upto := upto + 1;
  end loop;
  held := upto < cardinality(looked);
  if upto = 0 then
    return;
  end if;
  insert into sync.word (word)
  select distinct w.w
    from sync.item i
   cross join lateral sync.words_of(i.title, i.body_text) w(w)
   where i.entity_id = any (looked[1:upto])
  on conflict do nothing;
  insert into sync.word_filled_to (entity_id) values (looked[upto])
  on conflict (one) do update set entity_id = excluded.entity_id;
  read := upto;
end
$fn$;

create function sync.mark_words_whole() returns boolean
language plpgsql as $fn$
begin
  if exists (select from sync.item i
              where i.entity_id > coalesce((select f.entity_id from sync.word_filled_to f), '')) then
    return false;
  end if;
  insert into sync.word_whole default values on conflict do nothing;
  return true;
end
$fn$;
