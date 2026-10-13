- **`crates/knobas-source/src/**`, `knobas_core::payload` and the Gitea adapter's stored payload
  — issue #718 (2026-10-13): `KindPaths.head_repo` and `KindPaths.default_branch`, the Code
  tile's two misgroupings.** Spec #701, a follow-up of #712's review (PR #717), under the spec's
  delegation of 2026-10-13; #718 asks for this entry for a declared path or a payload change, and
  both happen.

  **The declarations.** `KindPaths` gains `head_repo: Vec<PayloadPath>` and `default_branch:
  Vec<PayloadPath>` (`#[serde(default)]` both, so an older peer's descriptor decodes as declaring
  none; `field::HEAD_REPO`, `field::DEFAULT_BRANCH`, in `field::ALL`; weighed as text by the
  contract battery like the other scalar paths). Gitea declares `head_repo` on `pr` at
  `head.repo.full_name` (a fork's name for a fork's pull request) and `default_branch` on `branch`
  at `repository.default_branch`. Mirrored in `app/src/lib/ipc/sources.ts`.

  **The payload.** Gitea's branch record does not say whether it is the default, so the
  `repository` object `map` already writes into a branch's (and a commit's, and a branch
  tombstone's) payload gains `default_branch`, the repository record's own, where that names one;
  the same key Gitea's `base.repo` carries. `GiteaCursor` goes to **version 3** (a version-2
  cursor stops parsing and the first run after the upgrade is a backfill, #610's plan), and
  `RepoCursor` gains `default_branch`: when a repository's default changes, every branch of it is
  re-emitted though no head moved, and no commit is walked for it. A commit beyond the budget
  keeps the version-2 object; nothing reads a default off a commit.

  **The rule** (`LIVE_ATTENTION`, amending #712's entry above). A `branch` whose title is the
  declared `default_branch` is dropped from the attention set with the settled rows: it is in no
  group and in no count (`total`, `tally`, `groups`), whoever's it is and whether or not its
  source has an identity, so in an ungrouped room too. A miss (no declared default) draws the
  branch as any other (ADR-0007). A branch is proposed by an open `pr` of the same source whose
  declared `head` is its title **and whose declared `head_repo` is its path line**, no longer the
  pull request's own (base) path line: a fork pull request from `main` leaves the base repo's
  `main` unproposed. A `pr` whose `head_repo` misses (a deleted fork) proposes nothing. A pull
  request that asks me for review and is mine is drawn once, under `review_requested`: the first
  group a row fits is its group (unchanged, now pinned).

  **What is not touched.** No `EntityOrder` member, no `EntityFilter` or `EntityPage` field, no
  command, no migration, no module, no frontend code beyond the type.

  Pinned by: `tests/it/entity.rs`'s `a_repos_default_branch_is_drawn_in_no_group`,
  `an_ungrouped_room_draws_no_default_branch_either`,
  `a_fork_pull_request_does_not_propose_the_base_repos_branch` and
  `my_pull_request_asking_me_is_drawn_once_as_review_requested` (with #712's three, whose
  fixtures now declare `head.repo`); `sources_mirror.rs`'s `KindPaths` keys; `knobas-source-gitea`'s
  `map::tests::a_branch_carries_its_repository_and_head_message` and
  `a_repository_naming_no_default_branch_writes_none`, `tests/it/sync.rs`'s
  `a_changed_default_branch_re_emits_every_branch_and_walks_no_commit` and
  `a_cursor_from_before_the_repository_payload_re_emits_branches_and_budgeted_commits` (now from
  version 2), and the contract battery over the fake, whose pull requests now carry `head.repo`.
  Witnessed against the real server by `just gitea-live`'s
  `the_shapes_the_fake_only_assumes_are_certified_here`: the seeded *SEPA retry* pull request's
  declared `head_repo` is its base repo's path line and, with its `head`, names exactly one
  mirrored branch; every live branch declares its repo's default, and in each repo exactly one
  branch is titled so.

