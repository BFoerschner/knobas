- **The IPC schema — issue #704 (2026-10-13): `CheckoutView` gains `tree`, where the working tree
  stands (ADR-0021).** Spec #701, workflow 3, under its delegation of 2026-10-13, which names
  ADR-0021 and "any new IPC for local status" among the surfaces it signs off; this is that entry.

  **No new command.** `entity_checkout` (and the `set_checkout_override` command, whose answer is
  `checkout::view` too) carries the state as one more field, `tree: Option<TreeRead>`, `None`
  exactly when `path` is.
  `TreeRead` is internally tagged on `status`: `read` with the `WorkingTreeState` fields beside the
  tag (`branch: Option<String>` with `None` for a detached head, `head: Option<String>` with `None`
  before a first commit, `upstream: Option<String>`, `ahead`, `behind`, `changed`, `untracked`,
  `conflicted`, all `u32`), `no_git`, `not_a_repo`, and `failed { reason }`. None of the four is
  an `IpcError`: each is a sentence the panel says. Mirrored in `app/src/lib/ipc/entity.ts` as
  `TreeRead` and `WorkingTreeState`.

  **What runs.** `knobas_app::checkout::working_tree::read` runs `git -C <checkout path>
  --no-optional-locks -c core.fsmonitor=false status --porcelain=v2 --branch
  --untracked-files=normal` with `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`,
  `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR`, `GIT_OBJECT_DIRECTORY` and
  `GIT_ALTERNATE_OBJECT_DIRECTORIES` removed, stdin closed and a 2-second timeout that kills it.
  `git` is looked up on `PATH`; there is no setting. `open_checkout` resolves the path without
  this read (`checkout::resolve`), so an editor does not wait on `git status`. The only argument
  from outside the program is the checkout path ADR-0016 already allowed; the branch name the
  mirror holds is compared in the shell (`app/src/lib/detail/working-tree.ts`'s `branchNote`) and
  never reaches git. `crates/knobas-app/Cargo.toml` turns on tokio's `process` feature for it.

  **What is not touched.** No migration, no event, no `WriteOp`, no new module in either barrel or
  in `commands/`; `working_tree` is a child module of `checkout`. Nothing is stored: the state is
  read when the panel opens and when the window regains focus.

  Pinned by: `crates/knobas-app/tests/it/working_tree.rs`'s ten tests against repositories `git`
  makes in a temp dir (clean, modified and staged and untracked, a rename, a conflict, detached,
  ahead and behind a local upstream, no commit yet, not a repository and a missing path, git
  missing from `PATH`, and `the_read_writes_neither_the_index_nor_a_ref`), and its eleventh,
  `a_git_that_does_not_answer_is_abandoned_at_the_timeout` (the real `git`, slowed by a clean
  filter in the test repository's own configuration; no stand-in); `checkout_ipc.rs`'s
  `the_view_says_where_the_working_tree_stands`; `knobas_app::checkout`'s
  `the_view_serialises_the_keys_the_mirror_declares` and `working_tree`'s
  `the_read_serialises_as_the_mirror_declares`; on the rendered side `working-tree.test.ts`,
  `BranchCheckoutNote.test.svelte.ts`, six new tests in
  `CheckoutPanel.test.svelte.ts` and `Detail.checkout.test.svelte.ts`.

