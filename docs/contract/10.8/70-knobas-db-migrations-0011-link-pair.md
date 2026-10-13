- **`crates/knobas-db/migrations/0011_link_pair_unordered.sql`, issue #70 (2026-08-30):** the
  link uniqueness rule becomes **unordered**. Granted under the **2026-08-29 delegation**
  (recorded in the #43 entry above): migrations merge under Fable merge-managers; milestone
  exits and the contract battery's clauses were not delegated and remain Björn's, and this PR
  touches neither. The *approach* was ruled separately — Björn, 2026-08-30, choosing the
  migration over command-layer canonicalisation.

  **The migration.** `0011`, allocated to this stream and to nothing else; `0001`–`0010` are
  never edited, and **`0012` is the next free number.** It replaces `link_active_idx`
  (`(from_id, to_id, relation) where deleted_at is null`, from `0001`) with
  `link_pair_active_idx` on `(least(from_id, to_id), greatest(from_id, to_id), relation)`,
  same partial predicate. No column, table or constraint changes.

  **What was wrong.** The rule was directed while `knobas_core::link::entries_of` reads
  `from_id = $1 or to_id = $1` — undirected. So `A→B` and then `B→A` under one relation both
  succeeded, and both panels drew two rows for one relationship, which is #40's story 14 read
  backwards. Every other statement in the codebase already treated the pair as unordered:
  `suggest`'s `driver_tail!` suppression compiles one `not exists` over `knobas.link` in both
  directions with no filter, because a detector that re-proposed `B→A` after the user removed
  `A→B` would silently resurrect a dismissal. The uniqueness rule was the one place that
  disagreed, and the new index is what makes them agree.

  **Unordered for uniqueness, ordered for storage.** Only the index expression normalises the
  pair; `from_id` and `to_id` keep what was written, so `blocks` still reads correctly from
  both ends and story 7's inverse labels are untouched. Canonicalising the *stored* pair was
  the cheap alternative and is exactly what it would have cost. Strictly stronger than what it
  replaces — every pair the old index refused this one refuses too — and the third column is
  still `relation`, so the same pair stays linkable under different relations (story 15).

  **It is re-entrant on a database the defect already damaged**, which is the half that costs
  something if it is wrong: `create unique index` fails on a table that violates it,
  `migrate::run` is on the boot path, and a migration that fails to apply is an app that no
  longer opens. Colliding groups are resolved first, keeping one row each, and which one is
  kept is not arbitrary: a **confirmed link outranks a proposal** whatever their ages (the #41
  symptom below), then the older row wins, `id` breaking the tie. Losers are tombstoned rather
  than deleted, so the detector's undirected suppression will not propose them back. Pinned by
  two tests in `knobas-db`'s schema battery — the rule itself, and the migration applied
  through the runner to a wound-back database holding exactly the rows it forbids.

  **Two behaviours outside the frozen list change with it, and are recorded here because the
  migration is what forces them.**

  `knobas_core::suggest::accept_edge` becomes `resolve_edge`, returning `Edge::Promoted` /
  `Superseded` / `Open` and taking a connection rather than a pool. Its second symptom is
  PR #132's, reported on #70 by that PR's merge-manager: the promotion path was
  direction-exact while the index was directed, so the two cancelled out and hand-drawing the
  **reversed** triple of a live proposal simply succeeded, leaving a stale proposal in the
  tray beside a confirmed link for one pair. Same direction is still *Accept*. Reversed is now
  the user contradicting the proposal's direction, and **the user wins**: the proposal is
  withdrawn — the same tombstone `dismiss` writes, so it is the same fact to detection — and
  the link is written with the ends the user gave it. Confirming it instead would store the
  opposite claim, which story 7's inverse labels would then render faithfully back at them.
  `create_link_inner` runs both in one transaction (`link::create_with` and
  `activity::record_with` are the executor-taking forms this needs), because the withdrawal is
  only half a mutation, and it answers the tray *before* the insert rather than after it fails
  — a unique violation aborts the transaction it happens in.

  `knobas_core::note`'s `reconcile_refs` loses its `on conflict` arbiter. An inference spec
  naming the three columns matches no index now and the statement would fail outright; naming
  the expression instead would put the normalisation in two places, and the copy there is the
  one that would go stale. Bare `on conflict do nothing` is safe: the endpoints come out of
  `knobas.entity` in the `select` itself, so no foreign key can be the fault, and a foreign-key
  violation is not a conflict `do nothing` covers in any case.

  It also gains a step, and this one is not cosmetic. Two notes naming each other now share
  one row, so `A` dropping its `[[B]]` would withdraw the only row there was while `B`'s body
  still said `[[A]]` — the exact body/links disagreement that module's header promises cannot
  happen, and a state `0011` produces on upgrade for any database already holding mutual refs.
  A ref link the *other* note still names is therefore **handed over** (its ends swapped)
  rather than withdrawn, so the row belongs to whoever still justifies it. Display-neutral:
  `refs_of` reads the body and backlinks read the pair undirected, so no panel changes.
  It reads `knobas.confirmed_link` and not the base table (#161) — every ref link is confirmed
  by the column default, and a machine proposal between two notes is the tray's to answer,
  never a side effect of saving a body.

  **No IPC change.** `create_link` still returns `LinkRow`, no command, DTO field or event name
  is added or changes meaning, and neither barrel is touched. `LinkMutation` gains
  `superseded: Option<ActivityRow>` and is not a wire type — the displaced proposal is
  announced as its own `activity:new` line, because a proposal leaving the tray is a mutation.

  Ratified by the orchestrator as issue #70 itself, whose acceptance criteria are the four
  properties the tests above pin.

