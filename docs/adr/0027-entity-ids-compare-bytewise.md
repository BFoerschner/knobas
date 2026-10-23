---
status: accepted
---

# Entity ids compare bytewise

Every id in the mirror was `text` in the database's collation, and `initdb` takes that collation from its environment: `en_US.UTF-8` on every machine measured, the app's own database included. A btree probe on such a key calls `strcoll` at each step down the tree. On the responsiveness probe's 22,000-item mirror one probe of `knobas.entity` by id took about 4.7 µs, and 0.75 µs on an index ordered `collate "C"` (#1163). Migration `0047` added that index beside five keys, and only `has:`'s walk compared through it. Every other read still paid `strcoll`.

Decided 2026-10-18, unattended (#1172, recorded in `docs/decisions/2026-10-unattended-rulings.md`): **every column that holds an entity id is `collate "C"`. Every other text column keeps the database's collation.**

- **Which columns.** `knobas.entity.id`, every column that a chain of foreign keys leads from to it, and the entity-id columns that have no foreign key. `knobas-db`'s `every_entity_id_compares_bytewise_and_a_title_by_the_database` holds the list. A migration that adds an entity-id column declares it `collate "C"` and, if no foreign key reaches it, adds it to that list.
- **Reads say nothing.** PostgreSQL resolves `C` against the default collation in favour of `C`, so `e.id = $1` or `l.from_id = i.entity_id` compares bytewise and uses the key's own index. No macro, no second index.
- **Equality is unchanged.** Both collations are deterministic: two ids are equal exactly when their bytes are.
- **A sort by id is bytewise.** That is the order Rust's `str` sorts in. No sort a user reads is by id alone. Titles, names and paths keep the database's collation, so they stay sorted the way the user reads them.
- **New and existing databases get the same schema.** Migration `0050` changes the columns. `initdb` and `create database` are unchanged.

## Considered options

- **Bytewise comparisons read by read (`same_key!`) on `0047`'s indexes.** Rejected: each hot read would have to remember to say `collate "C"` on both sides, and a key without a second index would need one more migration. A read that forgot was still correct, only slower, so nothing would catch it.
- **Create databases with a `C` collation** (`--locale-provider=builtin --builtin-locale=C.UTF-8`, or `--lc-collate=C`). Rejected: an existing database cannot change its collation, so old and new installs would sort titles differently until a dump and restore. It would also make every title, name and path sort bytewise, and each of those reads would then need an explicit collation to stay sorted the way a user reads them.

## Consequences

- One migration: `0050` drops `0047`'s five indexes and changes 34 columns. On the probe's 22,000-item mirror it ran in 1.7 s. `sync.item`'s stored `fts` reads `entity_id` (since `0048`), so it is dropped and added back, which rewrites the table once; without that it took 0.9 s. PostgreSQL will not change a column under a view or a generated column, so the three views are dropped and created again.
- A recursive CTE whose first term is default-collated text and whose recursive term reads an id column fails at plan time (`recursive query ... has collation "default" in non-recursive term but collation "C" overall`). The first term casts to `collate "C"`: `link_set_rule_tickets!` does.
- An explicit `collate "default"` on an id makes that comparison use `strcoll` again and lose the key's index. No read writes one any more.
- `0050` missed six columns: `knobas.context.id`, `timer.context_id`, `block.context_id`, `write_queue.link_to`, `run_preset.pipeline_id` and `ticket_view.saved_view`. Migration `0060` (#1708) changes them and gives `knobas.context.id` a foreign key to `knobas.entity`, so the test's foreign-key walk reaches it and the five others are on its list.
