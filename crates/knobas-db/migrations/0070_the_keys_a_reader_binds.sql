-- 0070_the_keys_a_reader_binds.sql -- two tables, and nothing else (#2429,
-- spec #2419 stories 43 to 49).
--
-- A **chord** is a modifier and one letter or digit, read by the key's code
-- and spelled as the spec spells it: the modifiers in the order ⌘ ⌃ ⌥ ⇧,
-- then the letter in capitals or the digit (`⌘⇧O`, `⌥1`). ⇧ alone is no
-- modifier: `⇧O` is typing. The check below holds that spelling, so an
-- archive or a hand-written row cannot store a chord nothing can press.
-- Which chords the launcher and the shell keep for themselves (⌘K, ⌘S, ⌘B,
-- ⌘D, ⌘T, ...) is the frontend's to refuse, where the keys are read.
--
-- ## A saved list's chord
--
-- `knobas.list_key`: one chord per list, one list per chord. The chord opens
-- the launcher on that list from anywhere, launcher closed or not. `list_id`
-- references `knobas.smart_list` with `on delete cascade`, as a list
-- favourite does (0069): deleting a list forgets its key.
--
-- ## An action's chord, per kind
--
-- `knobas.action_key`: a kind (`ticket`, `pr`, `repo`, `list`, `inbox`, ...)
-- and a chord name one action of that kind's Tab sheet, by the action's
-- stable id (`approve`, `open-vscode`, `fav`; `launcher/sheet.ts`). The same
-- chord means another action on another kind, so the chord is unique per
-- kind and not overall; an action holds at most one chord on a kind. No
-- foreign key: an action is a row of a table in code, and an id the code no
-- longer offers simply runs nothing.
--
-- Both travel in the share export's saved-lists part beside the favourites
-- (`backup::share`, `smart_lists`).

create table knobas.list_key (
  list_id    text primary key references knobas.smart_list (id) on delete cascade,
  chord      text not null unique,
  created_at timestamptz not null default now(),
  constraint list_key_chord_chk check (chord ~ '^⌘?⌃?⌥?⇧?[A-Z0-9]$' and chord !~ '^⇧?[A-Z0-9]$')
);

comment on table knobas.list_key is
  'A saved list''s chord (#2429): pressed anywhere, it opens the launcher on the list. One chord per list and one list per chord. Knobas-owned; travels in the share export with the saved lists.';

create table knobas.action_key (
  kind       text not null,
  chord      text not null,
  action_id  text not null,
  created_at timestamptz not null default now(),
  primary key (kind, chord),
  constraint action_key_one_chord_per_action unique (kind, action_id),
  constraint action_key_chord_chk check (chord ~ '^⌘?⌃?⌥?⇧?[A-Z0-9]$' and chord !~ '^⇧?[A-Z0-9]$')
);

comment on table knobas.action_key is
  'An action''s chord per kind (#2429): inside the launcher it runs the action with that id from the Tab sheet on the selected row of that kind. Knobas-owned; travels in the share export with the saved lists.';
