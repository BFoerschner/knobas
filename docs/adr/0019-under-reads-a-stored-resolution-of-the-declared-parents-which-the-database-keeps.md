---
status: accepted
---

# `under:` reads a stored resolution of the declared parents, which the database keeps

`under:<name>` (ADR-0018) needs to know which items answer to a name as their parent's, and which are parents themselves. Walking every live item through declarations bound as a query parameter cost hundreds of milliseconds at 100,000 items against the launcher's 100 ms budget (spec §14), and no index can serve a walk whose paths are a parameter. Decided 2026-10-12 (the deputy on #627, accepted by Björn the same day): **the walk's answer is stored, and the database keeps it.**

- Migration `0028` adds `sync.parent_declaration`, holding each adapter template's `payload_paths`, and `sync.item_parent`, one `(adapter_kind, name, kind)` row per name an item answers to, indexed on `lower(name)`. Statement-level triggers on `sync.item` fill it by running the `declared_parent_names!` and `declared_names_as_parent!` expansions, when an item is written and when a declaration changes. The app writes `sync.parent_declaration` from the template list it builds its searcher with, in `Searcher::declaring`, which writes only when something differs.
- Readers ask the index and join `knobas.source_config` on `s.id = i.source_id and s.kind = ip.adapter_kind`. The adapter is matched at read time, never at write time, because a trigger on `knobas.source_config` would put the sync's lock on `delete_source`'s path.
- The invariant: **the stored rows depend on the item and on `sync.parent_declaration`, and on nothing else.** A source configured after its items were written, removed, or given another adapter kind answers correctly on the next query, with nothing recomputed.
- Migration `0029` (#642) adds `arm` (`child` or `parent`) and `source_id`, the item's copied, with `item_parent_child_idx` on `(source_id, adapter_kind) where arm = 'child'`, so `under:` coverage is measured on the corpus: a source answers only if something it synced names a parent.
- The same pattern, by the same trigger functions restated, stores: the commits a build ran on in `sync.item_revision` (migration `0030`, #732, read by the detail's builds of a commit); the project each item declares in `sync.item_project` (migration `0032`, #759, #761, with `at` kept in step by `sync.item_updated`, read by `knobas_core::project::list`; the same migration adds `item_parent_name_prefix_idx` and `sync.item_parent.at` for prefix suggestions); and the Tickets screen's fields `status_name`, `assignee`, `project_key` and `project_name` in `sync.item_field` (migration `0063`, #2184), keyed by `(entity_id, field, paths)`. An `item_field` row is matched by its paths through `knobas_core::stored_join!` or `stored_string!`, and the payload is walked where none match, so a read binding declarations the database never stored answers as before. Each stored table answers in milliseconds where the walk took hundreds.
- The stored rows are derived and never in a backup: the `sync` schema is outside every archive (§16.12), and a re-sync fills them.
- `Searcher::new` with no templates answers `under:` from whatever the database has stored; the app always builds its searcher through `Searcher::declaring`.

## The lock

A write to `sync.item` takes `pg_advisory_xact_lock_shared(627, 0)` before it resolves. A statement on `sync.parent_declaration` that changed a row takes the same key exclusively before it recomputes; one that changed nothing returns first. Both hold the key until commit. Whichever side comes second waits for the first and reads what it wrote; item writes share the key, so concurrent syncs never wait on each other.

Invariant: the exclusive key is taken only by statements on `sync.parent_declaration`, never by a trigger on a table a sync run or a user command writes, so a run and a declaration change wait in one direction only and never deadlock.

Residual: a declaration change waits for every in-flight run to commit, and PostgreSQL queues later item writes behind it. It happens only when the stored declarations differ from the binary's (first bring-up after an upgrade, or a second binary on the same database), and bring-up writes the declarations before its first sync.

## Rejected

- Keep the per-row walk and tune it. Fencing brought it to about 180 ms, still over budget, and the fences hold only while the planner keeps its plan.
- Cache the known names in the searcher. A newly synced parent reads as unknown until refresh.
- A generated column. It may read only its own row and no subqueries; the declarations live outside the row.
- The sync writer fills a `text[]` column through Rust resolvers. A changed declaration would wait for a full re-sync, rows written by seeds or a restore would be empty, and the filter and the path line would be decided in two languages.
- Resolve the adapter at write time with a trigger on `knobas.source_config` taking the exclusive key. It deadlocks `delete_source` against a run of the same source (`mm-630-deadlock.sh`). Per-source keys deadlock one source at a time; `for key share` on config rows makes the delete wait for the run; taking the lock in Rust puts Rust on the write path; accepting the deadlock aborts a run or the user's delete.

## Consequences

- One SQL implementation decides both the filter and the path line. A migration cannot call a Rust macro, so each migration carries the expansion as text, and `knobas-search`'s `tests/it/item_parent.rs` fails if a macro's expansion drifts from the stored function until a new migration restates it. `resolve_parents` stays the Rust twin, held to `declared_parents!` by `crates/knobas-core/tests/it/payload_paths.rs`. `knobas-app`'s `tests/it/builds_of_a_commit.rs`, `tests/it/project_census.rs` and `tests/it/item_field.rs` hold the other stored tables to the walk and their resolvers' text to the macro expansions.
- No Rust on the write path: `knobas-sync`'s writer is unchanged, no adapter's `CURSOR_VERSION` moves, and an upgrade that changes a declaration recomputes the items of every kind it reaches when the app first writes the new declarations.
- This is the schema's first trigger and its first stored value that depends on something outside its own row.
