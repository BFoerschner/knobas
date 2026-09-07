-- 0023_the_checkout_and_its_override.sql -- the per-repo checkout path a
-- person sets by hand, when the clones-root scan is not the answer
-- (issue #499, v1.5).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0024 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0022` is M4.1's alert table (#444); this number is v1.5's
-- checkout override and nothing else. Ratified in advance by spec #491
-- ("Knobas-owned data in one migration: a settings row for the clones root,
-- and a small table of per-repo overrides keyed by repo entity id") and
-- recorded as an exception in `docs/contract.md` §10.8.
--
-- ## What a checkout is
--
-- `CONTEXT.md`, **Checkout**: "a clone of a repo on this machine, found under
-- the **clones root** -- a directory setting knobas scans two levels deep,
-- matching each clone's remote to a repo entity by host and owner/repo -- or
-- set by hand per repo as an override. Knobas-owned data about the local disk,
-- never a field of the mirrored repo, and never written to: no clone, no
-- checkout, no fetch (ADR-0016)."
--
-- Read that last clause twice, because it is what this table is *not*. There
-- is no path column on `sync.item` and there never will be: a mirrored repo's
-- every field was written by a remote system, and ADR-0016 forbids a program
-- knobas spawns from taking an argument out of one. A checkout is knobas' own
-- observation about this disk, so it lives in the `knobas` schema beside the
-- other knobas-owned rows and is keyed on the entity the observation is about.
--
-- ## Why the scan needs no table and the override does
--
-- The scan is a **read of the filesystem, performed on every open**, and
-- nothing about it is worth storing: a directory that moved between two opens
-- would leave a cached row pointing at nothing, and the read is a `read_dir`
-- of at most two levels. What cannot be re-derived is the answer a person
-- *gave* -- the repository whose clone sits outside the root, or the linked
-- worktree the scan deliberately does not follow -- and that is the whole of
-- this table.
--
-- ## The clones root is a setting and takes no DDL
--
-- `knobas.setting` (`0002`, comment 6) is the key/value store "for app-level
-- state that has no other home", and the clones root is one string. It is
-- written under the key `checkout.clones_root`, named here because this
-- migration is spec #491's "one migration ... a settings row for the clones
-- root, and a small table of per-repo overrides" and a reader looking for the
-- other half must not conclude it was forgotten. `knobas_app::checkout`
-- carries the key as a constant and a test pins the two spellings together;
-- adding a table for one row would be the migration-per-flag `0002` exists to
-- prevent.
--
-- ## Keyed on the repo entity, and cascading
--
-- `entity_id` references `knobas.entity` the way `monitor_alert` and
-- `monitor_sample` do: a repo purged from the mirror (its source deleted with
-- `purge_items`) takes its override with it, because the override is an
-- answer about *that* repository and means nothing without it. A **tombstoned**
-- repo keeps its entity row and therefore keeps its override -- the clone is
-- still on the disk after the server withdrew the repository, and forgetting
-- where it is would help nobody.
--
-- No `kind = 'repo'` constraint. The column cannot express one -- `knobas.entity`
-- carries the kind and a CHECK cannot reach across a row -- and the resolution
-- path refuses a non-repo kind by name before it ever writes here, which is
-- where the honest error message is. What the schema does enforce is the one
-- thing it can: **one override per repository**, as the primary key, so setting
-- a path twice replaces it rather than leaving two answers to one question.
--
-- ## The path is text and is not validated here
--
-- Not a `check (path like '/%')`: a Windows checkout is `C:\src\payout-service`
-- and a constraint written on macOS would refuse it, on a platform this row is
-- meant to work on. What is refused is a blank one, which is a person clearing
-- the field rather than setting it -- and clearing is a DELETE, so a blank row
-- could only ever be a bug.
create table knobas.checkout_override (
  entity_id  text primary key references knobas.entity(id) on delete cascade,
  path       text not null,
  updated_at timestamptz not null default now(),
  constraint checkout_override_path_not_blank_chk check (btrim(path) <> '')
);

comment on table knobas.checkout_override is
  'The checkout path a person set by hand for one repo entity, when the clones-root scan is not the answer (issue #499). Knobas-owned data about this disk; never written to a source, and never a field of the mirrored repo (ADR-0016).';
