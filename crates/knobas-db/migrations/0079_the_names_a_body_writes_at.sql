-- 0079_the_names_a_body_writes_at.sql -- `sync.item_mention`, the name each
-- `@` and `[~` of a prose item's body is followed by, kept by the database,
-- so the inbox's mention rule finds the items that write a reader's name
-- without reading every body that holds its letters (the query audit of
-- 2026-11-12, finding 3; PR #2479).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A mention is a reader's username after `@` or `[~`, not followed by
-- another name character (`knobas_core::inbox`'s `mention_of!`). The rule
-- found its candidates through `item_body_trgm_idx` (`0049`), which answers
-- with every body holding the name's trigrams anywhere: 4,465 bodies on a
-- mirror of 39,770 items for a username that is also a word, 6,872 for
-- another, each fetched and matched again to keep the 59 and 72 that write
-- the name at somebody. That was 115 to 158 ms of every read of the inbox.
-- Here the rule reads the items that store the name, and of those the ones
-- that moved inside its window.
--
-- ## What is stored
--
-- For each item of a prose kind (`ticket`, `pr`, `page`, `note`, the kinds
-- the rule reads) and each `@` or `[~` in its `body_text`: the longest run of
-- name characters (`[A-Za-z0-9_.-]`) that follows it, once per item. A marker
-- followed by none stores the empty text.
--
-- **The rule finds every mention through it.** Its expression matches a name
-- `N` where a marker is followed by `N` and then by the end or a character
-- that is not a name character. The run stored for that marker is the
-- longest prefix of what follows the marker made of name characters: `N`
-- itself when `N` is made of them, and `N` up to its first other character
-- when it is not. So the items that write a name at somebody are among the
-- items storing that one text, `substring(N from '^[A-Za-z0-9_.-]*')`, and
-- the rule's two expressions then decide, as before, on those bodies alone.
-- `knobas-core`'s `tests/it/inbox.rs` holds the rule to its earlier answer.
--
-- The stored rows depend on `sync.item` and on nothing else (ADR-0019): no
-- reader's name is in them. **A backup rebuilds it, never carries it**: the
-- `sync` schema is not in an archive (§16.12).
--
-- ## When it is written
--
-- * **A row inserted**: by a statement trigger, for the statement's rows.
-- * **A row whose body or kind an update changed**: by a row trigger `after
--   update of body_text, kind`, which an update naming neither column never
--   reaches (a stamp of `synced_at`, `0064`), its `when` leaving out a row
--   both came out equal in.
-- * **A row deleted**: the foreign key cascades.
--
-- ## First start
--
-- One pass over the prose bodies: 0.8 s on the 39,770-item mirror, whose
-- 29,918 prose bodies write 7,182 names at somebody, and 1.2 MB.

create function sync.mention_tokens(body text) returns text[]
language sql immutable parallel safe as $fn$
  select coalesce(array_agg(distinct m[1]), '{}')
    from regexp_matches(body, '(?:\[~|@)([A-Za-z0-9_.-]*)', 'g') m
$fn$;

create table sync.item_mention (
  entity_id text collate "C" not null,
  token     text not null
);

insert into sync.item_mention (entity_id, token)
select i.entity_id, t.token
  from sync.item i
 cross join lateral unnest(sync.mention_tokens(i.body_text)) as t(token)
 where i.kind in ('ticket', 'pr', 'page', 'note');

alter table sync.item_mention
  add primary key (token, entity_id),
  add foreign key (entity_id) references sync.item (entity_id) on delete cascade;
create index item_mention_entity_idx on sync.item_mention (entity_id);
analyze sync.item_mention;

create function sync.item_mention_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_mention (entity_id, token)
  select n.entity_id, t.token
    from new_rows n
   cross join lateral unnest(sync.mention_tokens(n.body_text)) as t(token)
   where n.kind in ('ticket', 'pr', 'page', 'note')
      on conflict do nothing;
  return null;
end
$fn$;
create trigger item_mention_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.item_mention_inserted();

create function sync.item_mention_changed() returns trigger
language plpgsql as $fn$
begin
  delete from sync.item_mention m where m.entity_id = new.entity_id;
  if new.kind in ('ticket', 'pr', 'page', 'note') then
    insert into sync.item_mention (entity_id, token)
    select new.entity_id, t.token
      from unnest(sync.mention_tokens(new.body_text)) as t(token);
  end if;
  return null;
end
$fn$;
create trigger item_mention_on_update after update of body_text, kind on sync.item
  for each row
  when (old.body_text is distinct from new.body_text or old.kind is distinct from new.kind)
  execute function sync.item_mention_changed();
