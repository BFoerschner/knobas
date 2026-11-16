-- 0094_the_first_letters_a_row_is_named_by.sql -- the first two and the first
-- three characters of each word a mirror row is named by, beside the first
-- (`0093`), in `sync.item_initial` (the two-letters pass, 2026-11-16; PR
-- #2479; ADR-0019's pattern;
-- `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- `0093` made one typed character an index read: 10 ms where the ranking of
-- every match took 57. The second and the third character were still ranked
-- that way. `pa` matched 25,886 of the 39,770 rows of Björn's scale and `re`
-- 28,778, each ranked by how many words beginning with those letters it
-- holds: 75 and 106 ms of backend on the Mac, more on his PC, against a
-- line of 100 ms from the key to its rows. Two letters are still the start
-- of a name, and so are three.
--
-- ## What is stored
--
-- **`sync.item_initial`**, as `0093` made it, now with one row for each
-- distinct first character, first two characters and first three characters
-- of the lexemes of a row's weight-A vector (`sync.item_search.fts_a`). So
-- `initial = k` is true exactly where `fts_a @@ 'k':*` is, for a `k` of one
-- to three characters that begins with a letter or a digit. The column
-- keeps its name: it holds a name's first characters, up to three.
--
-- * `leads`: a name of the row begins with those characters: the first
--   lexeme of its title, of the text after the title's last `/`, of a
--   ticket's key or of its key cell, or a pull request's number.
-- * A lexeme that begins with `-`, `+` or `/` (the number of a ticket's key,
--   a path) gives no row of two or three characters: the launcher asks
--   those lengths of letters and digits alone. Its one character is stored
--   as `0093` stored it.
--
-- `item_initial_newest_idx` is the order every length is answered in, as it
-- was the one character's.
--
-- ## When it is written
--
-- As `0093` wrote it, by the same two statement triggers, whose functions
-- are replaced here to write the three lengths: with a new row, and again
-- for a row whose title, key cell, kind, source or date an update changed.
-- A stamp or an edit of the body writes nothing. `sync.name_prefixes`
-- replaces `sync.name_initials`, which is dropped: one function says what a
-- row's characters are, and its one-character rows are the ones `0093`
-- wrote, so those stay as they are.
--
-- **A migration that changes what weight A holds** must write this table
-- again, as `0093` says.
--
-- ## What it costs
--
-- 539,457 rows more on the 39,770-row mirror (267,306 of two characters,
-- 272,151 of three), 13.6 a row beside the 6.4 there were: the table goes
-- from 57 MB with its indexes to 181 MB (73 MB of rows, 46 MB of key, 62 MB
-- of index), in a database of about 900 MB. The pass below is one read of
-- the mirror and three builds: 3.3 to 3.8 s on that mirror with the Mac at a
-- load of 8 to 10 (the rows 1.5 s, the keys 0.6 s, the index 1.3 s), once,
-- at the first start after the upgrade. The key, the foreign key and the
-- index are dropped for the pass and built over the whole table after it:
-- written into them a row at a time, the same rows took 7.3 s and left the
-- indexes a third larger. A write of a mirror row whose name or date changed
-- now writes about twenty narrow rows where it wrote six.

-- The first characters a row is named by, from its weight-A vector, and
-- whether a name of it begins with each. The first lexeme of a text is the
-- one at its lowest position; the title's is the vector's own, since the
-- title is lexed first. A plain `select`, so the planner inlines it into
-- the statement that calls it (`0093`).
create function sync.name_prefixes(fts_a tsvector, title text, kind text, entity_id text, key_cell text)
returns table (initial text, leads boolean)
language sql immutable parallel safe as $fn$
  with l as materialized (
    select array(
      select left(o, n)
        from unnest(array[
          (select u.lexeme from unnest(fts_a) u order by u.positions[1], u.lexeme limit 1),
          case when strpos(coalesce(title, ''), '/') > 0 then
            (select u.lexeme
               from unnest(to_tsvector('english', split_part(title, '/', -1))) u
              order by u.positions[1], u.lexeme limit 1) end,
          case when kind = 'ticket' then
            (select u.lexeme
               from unnest(to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))) u
              order by u.positions[1], u.lexeme limit 1) end,
          case when kind = 'pr' then substring(entity_id from '#([0-9]+)$') end,
          (select u.lexeme
             from unnest(to_tsvector('english', coalesce(key_cell, ''))) u
            order by u.positions[1], u.lexeme limit 1)]) o,
             generate_series(1, 3) n
       where length(o) >= n) as of
  )
  select w.p, w.p = any (l.of)
    from l, (select distinct left(x, n) as p
               from unnest(tsvector_to_array(fts_a)) x, generate_series(1, 3) n
              where length(x) >= n
                and (n = 1 or left(x, 1) not in ('-', '+', '/'))) w
$fn$;

-- Every row mirrored before this migration: its rows of two and of three
-- characters, beside the one-character rows `0093` wrote. The key, the
-- foreign key and the index come off for the pass and are built once over
-- the whole table after it.
alter table sync.item_initial
  drop constraint item_initial_entity_id_fkey,
  drop constraint item_initial_pkey;
drop index sync.item_initial_newest_idx;

insert into sync.item_initial (entity_id, initial, leads, kind, source_id, item_updated_at)
select i.entity_id, n.initial, n.leads, i.kind, i.source_id, i.item_updated_at
  from sync.item i
  join sync.item_search s on s.entity_id = i.entity_id
 cross join lateral sync.name_prefixes(s.fts_a, i.title, i.kind, i.entity_id, i.key_cell) n
 where length(n.initial) > 1;

alter table sync.item_initial
  add primary key (entity_id, initial),
  add foreign key (entity_id) references sync.item (entity_id) on delete cascade;
create index item_initial_newest_idx on sync.item_initial
  (initial, kind, leads desc, item_updated_at desc nulls last, entity_id);
analyze sync.item_initial;

-- `0093`'s two trigger functions, writing the three lengths.
create or replace function sync.item_initial_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_initial (entity_id, initial, leads, kind, source_id, item_updated_at)
  select n.entity_id, x.initial, x.leads, n.kind, n.source_id, n.item_updated_at
    from new_rows n
   cross join lateral sync.name_prefixes(ts_filter(n.fts, '{a}'), n.title, n.kind,
                                         n.entity_id, n.key_cell) x
      on conflict (entity_id, initial) do update
     set leads = excluded.leads, kind = excluded.kind, source_id = excluded.source_id,
         item_updated_at = excluded.item_updated_at;
  return null;
end
$fn$;

create or replace function sync.item_initial_changed() returns trigger
language plpgsql as $fn$
declare
  changed text[];
begin
  execute $q$
    select array_agg(n.entity_id)
      from new_rows n
      join old_rows o on o.entity_id = n.entity_id
     where (o.title, o.key_cell, o.kind, o.source_id, o.item_updated_at)
           is distinct from
           (n.title, n.key_cell, n.kind, n.source_id, n.item_updated_at)
  $q$ into changed;
  if changed is null then
    return null;
  end if;
  execute $q$
    delete from sync.item_initial x
     using unnest($1) c (entity_id)
     where x.entity_id = c.entity_id
  $q$ using changed;
  execute $q$
    insert into sync.item_initial (entity_id, initial, leads, kind, source_id, item_updated_at)
    select n.entity_id, x.initial, x.leads, n.kind, n.source_id, n.item_updated_at
      from unnest($1) c (entity_id)
      join new_rows n on n.entity_id = c.entity_id
     cross join lateral sync.name_prefixes(ts_filter(n.fts, '{a}'), n.title, n.kind,
                                           n.entity_id, n.key_cell) x
  $q$ using changed;
  return null;
end
$fn$;

drop function sync.name_initials(tsvector, text, text, text, text);
