- **Four new commands (`list_installed_adapters`, `check_adapter_updates`, `update_adapter`,
  `uninstall_adapter`), three new DTOs (`AdapterStatus`, `AdapterSource`, `Removal`) and one
  tagged union (`UpdateCheck`), and `install_adapter`'s argument widened — issues #1209 and
  #1210 (2026-10-19): install from a release, the update check and update, uninstall, and the
  restore's reinstall.** Parent spec #1192 (ADR-0028). No migration (the `0051` row already holds
  the source URL, the tag and the hash), no event, no new module in `commands/` or `ipc/`, no
  change to `crates/knobas-source/src/**`, `crates/knobas-http/**`, `crates/knobas-wasm-host/**`,
  `error.rs` or `profile.rs`. `restore_backup`'s shape is unchanged. Ratified by the implementer on
  Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md`.

  **`install_adapter(manifest: string)`** keeps its name and its answer. `manifest` may now be a
  GitHub repository URL (`https://github.com/<o>/<r>`, its latest release's
  `knobas-adapter.json`), any manifest URL, or a local path as before
  (`sources::remote::Origin::parse`). A manifest URL of the shape
  `…/<o>/<r>/releases/download/<tag>/<file>` records `<tag>`; a GitHub release records its
  `tag_name`; any other URL and a local file record no tag, and the hash alone pins them.
  `source_url` is what the reader pasted, normalised. A release with no `knobas-adapter.json`, or
  whose manifest names an asset it does not carry, is `invalid`; a release nobody answers for is
  `unreachable`.

  **`list_installed_adapters() -> AdapterStatus[]`**, by kind. **`AdapterStatus`**:
  `adapter_kind`, `name` (the descriptor's, or the kind for an adapter refused at load),
  `adapter_version`, `interface_version`, `source_url`, `tag`, `artifact_sha256`, `hosts`,
  `sources` (each `AdapterSource { id, display_name }` whose `source_config.kind` is the adapter's),
  `refused` (why load or restore refused it, else `null`), `update` (the last `UpdateCheck`, `null`
  before one ran). **`UpdateCheck`** is tagged on `state`: `current`;
  `available { tag, adapter_version }`; `failed { reason }` (the check could not read the latest
  release); `refused { tag, reason }` (the reader updated and the new release's checks refused it).

  **`check_adapter_updates() -> AdapterStatus[]`** reads each install's latest release (a tagged
  install is behind when the latest tag differs, an untagged one when the manifest's hash does),
  records each answer in process memory, and answers the list. It changes no install. The app runs
  it once at start, in the background (`sources::start`).

  **`update_adapter(kind: string) -> AdapterStatus`** fetches the latest release, runs every static
  check, refuses a release of another adapter kind, then stores the artifact, moves the row (version,
  interface, tag, hash, hosts, limits, `installed_at`) and replaces the registry's adapter; the next
  run builds the new one. The old artifact is removed once no row pins its hash. A refusal is
  `invalid` with the check's reason, recorded as `refused` beside the check, and changes nothing
  installed.

  **`uninstall_adapter(kind: string, removals: Removal[]) -> null`**. **`Removal`**:
  `source_id`, `purge_items`. Every source of the adapter needs an answer, else `invalid` naming the
  ones without and nothing removed. Each source is removed through `crud::delete` (the
  `delete_source` path: purge or keep its items) and the scheduler told as `delete_source` tells it;
  then the row, the registry entry and the unpinned artifact go.

  **Backup and restore.** The backup carries `knobas.external_adapter` as it already did (the
  schema dump); the `.wasm` is in the profile's `adapters/` store and in no archive. The share
  export carries no external-adapter record: the table is in no `ShareParts` part, and
  `share::tests` names it among the tables no part may carry. After a restore,
  `backup::restore` reinstalls each row (`installed::reinstall_restored`): an artifact already
  stored under the row's hash loads; any other is fetched from `source_url` at the recorded tag and
  refused unless the manifest's hash and the bytes' hash are the row's. A refusal is recorded on the
  registry, shown as `refused` by `list_installed_adapters`, and the rest of the restore stands.

  Pinned by `knobas-app`'s `tests/it/external_adapter_release.rs` (a local forge serving GitHub's
  and Gitea's release APIs; one `#[ignore]`d live test against a real public repository named by
  `KNOBAS_ADAPTER_LIVE_REPO`), `tests/it/sources_mirror.rs`'s `adapter_status_matches_the_mirror`,
  `sources::remote`'s unit tests and `TfSettings.test.svelte.ts`.
