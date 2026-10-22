- **The IPC schema and the event schema — issue #823 (2026-10-15): one command, `clone_repo`, and
  one event, `clone:progress` (ADR-0023).** Spec #808, milestone TF6, under Björn's 2026-10-15
  rule for that milestone (the implementer decides forks and records them); ADR-0023's
  consequences ask for this entry by name.

  **One command, `clone_repo(entityId, stop: Option<bool>) -> CloneAnswer`** (`commands/entity.rs`,
  appended at the foot of the handler list). A repo or a branch; a branch clones its repository.
  `stop` absent or false starts a clone and answers at once; `stop: true` stops the running clone
  and answers once its half-made folder is removed. One command and not a second `cancel_clone`,
  because ADR-0023 names one. `CloneAnswer` is internally tagged on `status`:
  `cloning { path }` (started, or already running), `adopted { path }` (the folder is already a
  clone of this repository; nothing ran), `stopped { path, removed }` and `not_cloning` (a stop
  with nothing running). Refusals are `IpcError`s: `invalid` for an address that is not a repo or
  branch, a repository not from a Gitea source, a name outside
  `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` or with a `.` or `..` half, a source address that is not
  https or ssh, no clones root or a relative one, and no git; `not_found` for a repository the
  mirror does not hold or whose source is gone; `conflict` for a folder that exists and is not a
  clone of this repository ("<path> exists and is not a clone of this repository. Rename it, or
  set a path by hand"), or for a folder another repository of the same name is cloning into.
  Mirrored in `app/src/lib/ipc/entity.ts` as `cloneRepo` and `CloneAnswer`.

  **One event, `clone:progress`**, `knobas_app::events::CLONE_PROGRESS`, broadcast, mirrored in
  `app/src/lib/ipc/index.ts`'s `EVENTS.cloneProgress`. Payload `CloneEvent { entity_id, path }`
  with `CloneState` flattened beside them, tagged on `state`: `progress { phase, percent }`
  (`percent` is the whole clone, 0–100), `done`, `failed { reason, cause, removed }` with `cause`
  one of `auth`, `disk_full`, `other`, and `stopped { removed }`. An event and not an
  `ipc::Channel`: every view that shows the repository draws the same button (R-8.25), not only
  the one that started it. Progress is coalesced to at most one per second besides one per change
  of step, `activity:new`'s rate, so the event keeps the "coarse state" rule in
  `knobas_app::events`.

  **What runs.** `git -c protocol.allow=never -c protocol.https.allow=always -c
  protocol.ssh.allow=always -c submodule.recurse=false clone --progress --no-recurse-submodules --
  <url> <clones root>/<repo>`, no shell, with `GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`, and
  `GIT_ALLOW_PROTOCOL`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT`, `GIT_DIR`, `GIT_WORK_TREE`,
  `GIT_INDEX_FILE`, `GIT_COMMON_DIR`, `GIT_OBJECT_DIRECTORY` and
  `GIT_ALTERNATE_OBJECT_DIRECTORIES` removed from its environment. The URL is the Gitea source's
  `knobas.source_config.base_url` plus `/<owner>/<repo>.git`, never the mirrored `web_url`.
  knobas makes the folder itself with `create_dir` before git starts, so a failure or a stop only
  ever removes a folder this run made. The pure half is `knobas_core::checkout::clone`.

  **What is not touched.** No migration, no setting, no `WriteOp`, nothing in
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, and no new module in either barrel or
  in `commands/`: `clone` is a child module of `checkout`, and `commands/entity.rs` already holds
  the checkout commands. The running clones are an in-memory registry
  (`checkout::clone::Clones::global()`); a quit kills git (`kill_on_drop`) and may leave a
  half-made folder. That folder already has its `origin`, so adoption also asks that `HEAD` names
  a commit (`knobas_core::checkout::clone::is_finished`), and the next *Clone* refuses it with
  "<path> is a clone of this repository that did not finish. Remove it and clone again".
  `crates/knobas-app/src/{error,profile}.rs` are unchanged.

  **The test-only door.** `checkout::clone::start_admitting`, behind the `test-util` feature like
  `AppState::over_pool`, admits other protocols: `file` for `tests/it/clone_ipc.rs` and `http` for
  the testenv Gitea in `tests/clone_live.rs`, which has no https. The production list is pinned by
  `knobas_core`'s `the_argument_list_is_the_adrs`; `clone_ipc.rs`'s
  `git_refuses_any_other_protocol_given_the_production_flags` shows git refusing a `file://` given
  those flags, and `the_app_clones_over_https_and_ssh_only` and `clone_live.rs`'s
  `the_app_itself_refuses_the_testenv_address` show the production entry point refusing first.

  Pinned by: `knobas_core::checkout::clone`'s nine unit tests (the name, the URL, the argument
  list, the progress weights, a finished clone, the failure line and cause); `knobas_app::checkout::clone`'s
  `the_clone_shapes_serialise_as_the_mirror_declares`; `lib.rs`'s
  `the_event_names_match_their_typescript_mirror`; eleven in `tests/it/clone_ipc.rs`; and five
  live in `tests/clone_live.rs`, run by `just gitea-live`.

  **Amendment (#1633, 2026-10-22): the quit stops a clone and removes its folder.** No signature,
  type or event changes. The exit handler calls `checkout::clone::stop_all` before it stops the
  scheduler: every running clone is stopped as `stop: true` stops it, its half-made folder is
  removed and `clone:progress` says `stopped { removed }`, waiting up to 6 s. The sentence above
  that a quit "may leave a half-made folder" now holds only for a crash, a signal, or a folder
  still being written after those 6 s; the not-finished refusal stays for those. `clone_repo`
  gains one refusal: `not_ready` ("knobas is quitting, so no clone starts") for a start that
  arrives after `stop_all` began. The registry is now managed state (`State<'_, Arc<Clones>>`,
  `Clones::global()` managed in `setup`), an injected argument the frontend never passes. Pinned
  by `tests/it/clone_ipc.rs`'s `a_quit_during_a_clone_removes_its_folder_and_the_next_clone_starts`.
