- **The IPC command schema and the Rust handler barrel — issue #824 (2026-10-15): one new read
  command, `entity_checkout_diff`, the uncommitted diff at a checkout (ADR-0026).** Spec #808,
  requirement R-8.9 and ruling D5; milestone TF6. ADR-0026's consequences leave "one IPC command or
  one more field on `CheckoutView`" to the build, with its §10.8 entry; this is that entry.

  **A command, not a field.** `entity_checkout` is read on every detail open and every window
  focus, and a diff reads file contents (3.7 s worst case on a 17,000-file checkout, measured), so
  the diff is read when the *Local changes* tab opens and not before. In the **existing**
  `commands/entity.rs`, beside `entity_checkout`:

  ```rust
  #[tauri::command] pub async fn entity_checkout_diff(.., entity_id: String)
      -> Result<knobas_app::checkout::uncommitted::DiffRead, IpcError>;
  ```

  `DiffRead` is internally tagged on `status` with `TreeRead`'s four arms: `read` with the
  `UncommittedDiff` fields beside the tag (`unstaged: CappedDiff`, `staged: CappedDiff`,
  `untracked: Vec<String>`, `untracked_truncated: bool`; `CappedDiff { files:
  Vec<knobas_source::diff::DiffFile>, truncated: bool }`), `no_git`, `not_a_repo`, and
  `failed { reason }`. None of the four is an `IpcError`. The errors are the checkout's own:
  `invalid` for an address that is not a repo or a branch, `not_found` for one the mirror does not
  hold or with no checkout on this machine (as `open_checkout` refuses), `internal` for a query.
  Mirrored in `app/src/lib/ipc/entity.ts` as `CappedDiff`, `UncommittedDiff`, `DiffRead` and
  `entityCheckoutDiff`, and in the fake IPC.

  **What runs.** `knobas_app::checkout::uncommitted::read`, three commands under
  `working_tree`'s environment, guards and shim check (now shared as `locate` and `git_at`):
  `git -C <path> --no-optional-locks -c core.fsmonitor=false -c diff.autoRefreshIndex=false diff
  --no-ext-diff --no-color --no-textconv --src-prefix=a/ --dst-prefix=b/`, the same with
  `--cached`, and `git -C <path> --no-optional-locks -c core.fsmonitor=false status
  --porcelain=v2 -z --untracked-files=normal` for the untracked paths. Two flags beyond
  ADR-0026's list, each needed for a clause of it: `diff.autoRefreshIndex=false`, because
  `git diff` rewrites the index's stat data after the diff and `--no-optional-locks` does not stop
  it (git 2.50.1, witnessed); and the pinned prefixes, so a checkout's `diff.noprefix` or
  `diff.mnemonicPrefix` does not move a path through the shared parser. One 10-second timeout over
  all three. Each diff half is read up to 24,000 lines or 16 MiB and git is killed past that;
  then parsed by `knobas_source::diff::parse_view` under its per-file and per-view caps. At most
  1,000 untracked paths.

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**` (the parser is used as it is), no new module in either barrel or
  in `commands/`; `uncommitted` is a child module of `checkout`. The handler list grows by one line
  at its foot. Nothing is stored.

  Pinned by `crates/knobas-app/tests/it/uncommitted_diff.rs` (thirteen tests over repositories
  `git` makes in a temp dir: modified, staged, staged and changed again, untracked, a staged
  rename, binary, over the per-file cap, two prefix configurations, a configured diff driver never
  run, the index and refs unchanged after a read, not a repository, git missing), the module's own
  tests (the line and byte read caps, the untracked cap, the timeout over a real slow git, and
  `the_read_serialises_as_the_mirror_declares`), and `checkout_ipc.rs`'s
  `the_local_changes_are_read_at_the_resolved_checkout`.
