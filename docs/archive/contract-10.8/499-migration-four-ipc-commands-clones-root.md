- **A migration and four IPC commands — issue #499 (2026-09-08): the clones root, the scan
  and the per-repo checkout override.**

  The **second** of v1.5's two migrations; #496's expression index above is the first. Ratified
  in advance by spec #491 ("Knobas-owned data in one migration: a settings row for the clones
  root, and a small table of per-repo overrides keyed by repo entity id") and by #499's own
  fourth criterion, which asks for this entry by name. **Björn keeps the gate for frozen
  contracts and this entry is flagged for his review.**

  **`crates/knobas-db/migrations/**` — one migration, `0024_the_checkout_and_its_override.sql`.**
  `knobas.checkout_override (entity_id text primary key references knobas.entity(id) on delete
  cascade, path text not null, updated_at timestamptz not null default now())`, with a
  `btrim(path) <> ''` CHECK. Nothing before `0024` is edited. It was **written as `0023` and
  renumbered** when #496's index landed on `main` first; nothing had applied it, which is the only
  circumstance in which a migration may be renumbered at all and the reason the number is
  single-writer. Two things it deliberately does **not** do, both argued in its own header: it
  adds **no path column to `sync.item`** — every field on a mirrored repo was written by a remote
  system, and ADR-0016 forbids a spawned program taking an
  argument from one — and it adds **no table for the clones root**, which is one
  `knobas.setting` row under `checkout.clones_root` (`0002`, comment 6, is what that store exists
  for). The migration's header names that key so a reader of spec #491's "one migration ... *and*
  a settings row" does not conclude the second half was forgotten;
  `knobas_app::checkout::CLONES_ROOT_KEY` is the constant and
  `the_setting_key_is_the_one_the_migration_names` pins the two spellings together.

  **The IPC schema — four commands and one DTO, all additive:**

  * `clones_root() -> Option<String>` and `set_clones_root(path: Option<String>)` — the setting,
    read and written. A blank path clears it rather than storing an empty root the scan would walk
    from the process's working directory.
  * `entity_checkout(entity_id) -> CheckoutView` — the override, else the scan, else *no
    checkout*. Read on every open and **stored nowhere**: a clone that moved between two opens
    would leave a cached path pointing at nothing. Its two statements read `sync.item` and not
    `sync.live_item`, which makes `checkout::repo_of` the **fourth** reader to reach past the
    view: the panel is mounted inside `get_entity`'s detail, which is exempt from both halves for
    the same reason, and the clone is still on the disk after the source withdrew the repository
    or somebody turned it off. `CONTEXT.md`'s **Live item** entry names it beside the other
    three, per that entry's own rule.
  * `set_checkout_override(entity_id, path: Option<String>) -> CheckoutView` — set or clear, keyed
    on the **repository** even when a branch was the address, answering the fresh view so clearing
    an override *shows* the scan taking over.
  * `CheckoutView { repo_entity_id, repo_url, path, found_by, clones_root, clone_command }` with
    `FoundBy` = `override` | `scan` | `nothing`, mirrored in `app/src/lib/ipc/entity.ts`.
    `clones_root` rides on the view so *no root set* and *nothing found under one* stay apart on
    the panel; `clone_command` is `git clone <the repo's URL>` and is **text to copy, never run**.

  **What is not touched.** The `commands/` + `ipc/` module layout is **unchanged**: all four live
  on the `entity` pair, because a repo and a branch are entities and `get_entity` is already there
  — the placement `notify` and #290's notification setting took, for the same reason. **Neither
  append-only barrel grows a module line** (`app/src/lib/ipc/index.ts` re-exports `./entity`
  already); `crates/knobas-app/src/lib.rs`'s handler list grows the four commands, which is what
  that list is append-only *for*. No event. Nothing in `crates/knobas-source/src/**` — a checkout
  is not a source's business and no adapter learns anything — nothing in `crates/knobas-http/**`,
  and `crates/knobas-app/src/{error,profile}.rs` are untouched. No `WriteOp`: nothing here writes
  to a source, and nothing here writes to a working tree either (ADR-0016 — no clone, no checkout,
  no fetch). The keychain is not involved; a path is not a secret. The share export's part list is
  unchanged and `knobas.checkout_override` is in **no** part, `knobas.setting`'s reason exactly: a
  path on this machine means nothing on a colleague's.

  Pinned by: `knobas_core::checkout`'s `ssh_and_https_forms_normalise_to_the_same_key` (eleven
  spellings, one key), `a_url_that_names_no_repository_misses` (fourteen negatives, Windows paths
  among them), `a_nested_group_keeps_the_last_two_segments`,
  `only_the_repository_loses_a_git_suffix`, `the_clone_command_is_the_repos_url_and_nothing_else`,
  `the_origin_url_is_read_out_of_a_git_config` and `a_config_with_no_origin_misses`;
  `knobas-core/tests/checkout_scan.rs`'s seven walks of a real temporary tree —
  `the_scan_reaches_two_levels_and_no_further`,
  `the_shallower_of_two_clones_wins_and_the_answer_is_stable`,
  `a_worktree_is_not_found_by_the_scan` and `a_submodule_inside_a_clone_is_not_a_second_checkout`
  among them; `knobas_app::checkout`'s `the_setting_key_is_the_one_the_migration_names`,
  `found_by_serialises_as_the_mirror_declares`,
  `the_view_serialises_the_keys_the_mirror_declares` and
  `the_kinds_with_a_checkout_are_the_ones_the_panel_is_mounted_for` (which reads `Detail.svelte`,
  because the panel's gate is the second spelling of `CHECKOUT_KINDS` and `entity_checkout`
  refuses every other kind by name); `commands::entity`'s
  `the_mirror_invokes_the_commands_by_their_registered_names` and `tests/wiring.rs`'s
  `every_command_is_in_the_handler_list`, which is what makes the four reachable from the window;
  fourteen tests in `crates/knobas-app/tests/checkout_ipc.rs` over scratch databases — the
  setting's round trip, the scan, the override winning and giving the scan back, a branch
  answering its repository's, the three refusals,
  `a_branch_resolves_to_the_longest_repo_id_it_starts_with` and
  `an_underscore_in_a_repo_name_is_not_a_wildcard` for the two ways the branch-to-repo lookup can
  claim the wrong repository, and `purging_the_repo_takes_its_override_with_it` for the cascade,
  and `a_withdrawn_repo_and_a_turned_off_source_still_answer_their_checkout` for the two halves
  of the view this read is exempt from; and, on the rendered side, eight in `CheckoutPanel.test.svelte.ts` and five in
  `CheckoutsSection.test.svelte.ts`.

