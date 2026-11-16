-- 0093_the_letters_a_row_is_named_by.sql -- the first character of each word
-- a mirror row is named by, one row a character in `sync.item_initial`, in
-- the order one typed character is answered in (the one-letter pass,
-- 2026-11-15; PR #2479; ADR-0019's pattern;
-- `docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- One character typed into the launcher under *Everything* took 132 ms a key
-- in knobas's own window on Björn's PC, 166 at CPU 4x and 260 at 6x, against
-- lines of 100, 100 and 200; the search alone was 121 ms. A character matches
-- every row that holds a word beginning with it, 35,232 of 39,770 on his
-- scale, and each match was ranked by how many such words it holds: a number
-- that depends on the character, so it was never stored, and that says
-- nothing a person meant by one key.
--
-- One character is now answered by names: the rows a word of whose title or
-- key begins with it (what *Titles* matches), those whose name begins with it
-- first, then the newest. That order depends on nothing typed, so it is an
-- index, and the answer is the index's first rows of each kind: a hundred
-- index entries where there were 35,000 vectors.
--
-- ## What is stored
--
-- **`sync.item_initial`**: for each mirror row, one row for each distinct
-- first character of the lexemes of its weight-A vector
-- (`sync.item_search.fts_a`, `ts_filter(fts, '{a}')`): its title, the title
-- with `/` as a space, a ticket's key, a pull request's number and its key
-- cell (`0072`'s expression). So `initial = c` is true exactly where
-- `fts_a @@ 'c':*` is, for every character `c` the parser reads as a word.
-- One it does not (the `-` of a signed number, the `/` of a path, a tenth
-- of the rows) is stored and never asked: the launcher asks only the word a
-- typed character lexes to.
--
-- * `leads`: a name of the row begins with the character: the first lexeme
--   of its title, of the text after the title's last `/` (`0056`'s tail), of
--   a ticket's key or of its key cell, or a pull request's number (`0072`).
--   `The payout service` leads with `p`: a stop word is no lexeme.
-- * `kind`, `source_id`, `item_updated_at`: `sync.item`'s, for the cut by
--   kind, the liveness and source tests, and the order.
--
-- **`item_initial_newest_idx (initial, kind, leads desc, item_updated_at
-- desc nulls last, entity_id)`**: the answer's order within a kind, so each
-- kind's first rows are read in order and the read stops at the last.
--
-- ## When it is written
--
-- * **A row inserted**: `sync.item_initial_inserted`, the statement's rows.
-- * **A row whose title, key cell, kind, source or date an update changed**:
--   `sync.item_initial_changed`, a statement trigger that writes those rows'
--   characters again. A stamp of `synced_at` or an edit of the body changes
--   none of the five and writes nothing here.
-- * **A row deleted**: the foreign key cascades.
-- * **Every row mirrored before this migration**: by bring-up's fill, a
--   batch at a time behind the app (`0095`'s `sync.fill_item_initial_beside`,
--   `knobas_core::item_initial`). Until it has passed over every row the
--   launcher ranks a typed character as it did before this migration.
--
-- The stored rows depend on `sync.item` and on nothing else (ADR-0019).
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts write it.
--
-- **A migration that changes what weight A holds** (`sync.item.fts`'s
-- expression) rewrites `sync.item` without firing a trigger, so it must
-- write this table again, as the pass below does.
--
-- ## What it costs
--
-- 253,585 rows on the 39,770-row mirror, 6.4 a row: a table of 23 MB and
-- 33 MB of indexes, of a database of 839 MB. Each write of a mirror row
-- whose name or date changed lexes its key and its key cell once more and
-- writes about six narrow rows.
--
-- **Nothing at the first start.** Until 2026-11-16 this migration wrote
-- every mirrored row's characters in one pass, 2.6 s on that mirror with the
-- machine busy, before the window opened. The long-words pass took the pass
-- out: the table is made empty, with its key and its index, and bring-up
-- fills it behind the app (`0095`). A database that ran the migration as it
-- was holds the rows that pass wrote, and the fill finds each of them done
-- once it has written the row `0095` adds
-- (`docs/decisions/2026-11-16-a-word-many-rows-hold.md`).

-- The characters a row is named by, from its weight-A vector, and whether a
-- name of it begins with each. The first lexeme of a text is the one at its
-- lowest position; the title's is the vector's own, since the title is lexed
-- first (a row with no title lexeme leads with its key's). A plain `select`,
-- so the planner inlines it into the statement that calls it: one plan for
-- the statement's rows, where a call a row took seven times as long.
create function sync.name_initials(fts_a tsvector, title text, kind text, entity_id text, key_cell text)
returns table (initial text, leads boolean)
language sql immutable parallel safe as $fn$
  with l as materialized (
    select array_remove(array[
      (select left(u.lexeme, 1) from unnest(fts_a) u order by u.positions[1], u.lexeme limit 1),
      case when strpos(coalesce(title, ''), '/') > 0 then
        (select left(u.lexeme, 1)
           from unnest(to_tsvector('english', split_part(title, '/', -1))) u
          order by u.positions[1], u.lexeme limit 1) end,
      case when kind = 'ticket' then
        (select left(u.lexeme, 1)
           from unnest(to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))) u
          order by u.positions[1], u.lexeme limit 1) end,
      case when kind = 'pr' then left(substring(entity_id from '#([0-9]+)$'), 1) end,
      (select left(u.lexeme, 1)
         from unnest(to_tsvector('english', coalesce(key_cell, ''))) u
        order by u.positions[1], u.lexeme limit 1)], null) as of
  )
  select w.c, w.c = any (l.of)
    from l, (select distinct left(x, 1) as c from unnest(tsvector_to_array(fts_a)) x) w
$fn$;

create table sync.item_initial (
  entity_id       text collate "C" not null,
  initial         text collate "C" not null,
  leads           boolean not null,
  kind            text not null,
  source_id       text not null,
  item_updated_at timestamptz
);

-- The key and the index, over a table that holds nothing yet: the rows
-- mirrored before this migration are written by bring-up's fill (`0095`).
alter table sync.item_initial
  add primary key (entity_id, initial),
  add foreign key (entity_id) references sync.item (entity_id) on delete cascade;
create index item_initial_newest_idx on sync.item_initial
  (initial, kind, leads desc, item_updated_at desc nulls last, entity_id);

create function sync.item_initial_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_initial (entity_id, initial, leads, kind, source_id, item_updated_at)
  select n.entity_id, x.initial, x.leads, n.kind, n.source_id, n.item_updated_at
    from new_rows n
   cross join lateral sync.name_initials(ts_filter(n.fts, '{a}'), n.title, n.kind,
                                         n.entity_id, n.key_cell) x
      on conflict (entity_id, initial) do update
     set leads = excluded.leads, kind = excluded.kind, source_id = excluded.source_id,
         item_updated_at = excluded.item_updated_at;
  return null;
end
$fn$;
create trigger item_initial_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.item_initial_inserted();

-- The characters of the rows an update changed the name, kind, source or
-- date of, written again: the rows it held go, and its rows now are
-- written. The changed rows are found in one join of the statement's rows,
-- so an update that changed none of the five (a stamp of 10,000 rows) reads
-- them once and writes nothing. `execute`d, so each statement is planned on
-- this statement's row counts (`0075`'s lesson, `0084`'s shape).
create function sync.item_initial_changed() returns trigger
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
     cross join lateral sync.name_initials(ts_filter(n.fts, '{a}'), n.title, n.kind,
                                           n.entity_id, n.key_cell) x
  $q$ using changed;
  return null;
end
$fn$;
create trigger item_initial_on_change after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.item_initial_changed();
