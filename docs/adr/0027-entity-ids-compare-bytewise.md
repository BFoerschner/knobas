---
status: accepted
---

# Entity ids compare bytewise

Every id in the mirror was `text` in the database's collation, so a btree probe on an id called `strcoll` at each step: about 4.7 µs per probe of `knobas.entity` against 0.75 µs on an index ordered `collate "C"` (#1163). Decided 2026-10-18, unattended (#1172, `docs/archive/2026-10-unattended-rulings.md`): **every column that holds an entity id is `collate "C"`. Every other text column keeps the database's collation.**

- **Which columns.** `knobas.entity.id`, every column a chain of foreign keys leads from to it, and the entity-id columns with no foreign key. `knobas-db`'s `every_entity_id_compares_bytewise_and_a_title_by_the_database` holds the list. A migration that adds an entity-id column declares it `collate "C"` and, if no foreign key reaches it, adds it to that list.
- **Reads say nothing.** PostgreSQL resolves `C` against the default collation in favour of `C`, so `e.id = $1` compares bytewise and uses the key's own index. No macro, no second index.
- **Equality is unchanged.** Both collations are deterministic.
- **A sort by id is bytewise.** No sort a user reads is by id alone. Titles, names and paths keep the database's collation.
- **New clusters** (amended 2026-10-24, #2013) are created with `--locale-provider=builtin --builtin-locale=C.UTF-8` and the C library's collation and ctype fixed per platform (`C.UTF-8` on macOS and Linux, `en-US` on Windows; `embedded::LIBC_LOCALE`), because an app started from Finder has no locale and `initdb` came out libc `C`, where `lower()`, `to_tsvector` and `pg_trgm` leave non-ASCII letters alone. Titles on a new cluster sort by code point and no sort gets an explicit collation to undo that. Existing clusters are left as they were made, so the same titles can sort differently on an old install and a new one. The id columns are `collate "C"` on both.

## Rejected

- Bytewise comparisons read by read (`same_key!`) on `0047`'s indexes. A read that forgot was correct but slow, so nothing would catch it.
- A `C` collation for the whole database. An existing database cannot change its collation. Reversed for new clusters only by #2013, above.

## Consequences

- Migration `0050` dropped `0047`'s five indexes and changed 34 columns; migration `0060` (#1708) changed the six it missed (`knobas.context.id`, `timer.context_id`, `block.context_id`, `write_queue.link_to`, `run_preset.pipeline_id`, `ticket_view.saved_view`) and gave `knobas.context.id` a foreign key to `knobas.entity`.
- A recursive CTE whose first term is default-collated text and whose recursive term reads an id column fails at plan time. The first term casts to `collate "C"`, as `link_set_rule_tickets!` does.
- An explicit `collate "default"` on an id uses `strcoll` again and loses the index. No read writes one.
