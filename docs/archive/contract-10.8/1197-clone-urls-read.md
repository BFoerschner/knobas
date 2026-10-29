- **`crates/knobas-source/src/**` (a descriptor field and the code-host minimum), the IPC schema
  of `list_adapters` and the `sources.ts` mirror — issue #1197 (2026-10-18): the code-host read
  `clone_urls`, and *Clone* for any code-host source.** No migration, no new command, no
  `commands/` or `ipc/` module, no `WriteOp`, no `KindPaths` field, no change to
  `crates/knobas-wasm-host/wit/adapter.wit` (`knobas:adapter@1.0.0` is untouched), no change to
  `crates/knobas-app/src/error.rs`. Parent spec #1192 (ADR-0023, ADR-0029). Ratified by the
  implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md`.

  **The read.** The code-host minimum (`knobas_source::role::MINIMUMS`) gains the required read
  `clone_urls` (`role::CLONE_URLS`). It is answered by a declaration, not by a `Source` method,
  as the diff and tree reads are answered by the view's blocks: `SourceDescriptor` gains
  `clone_from: Vec<clone_urls::CloneFrom>`, `#[serde(default)]` on the `roles` precedent, so a
  descriptor from a peer that predates it decodes as one that names no address. `CloneFrom` is
  `BaseUrl` (`"base_url"`) or `Config(key)` (`{"config": "<key>"}`): which address a person typed
  into the instance a repository is cloned from, in order of preference. Mirrored in
  `app/src/lib/ipc/sources.ts` as `clone_from: CloneFrom[]` and held by `sources_mirror.rs`.
  Gitea declares `["base_url"]`.

  **Why a declaration.** ADR-0023 builds the clone URL from an address a person typed plus the
  checked full name, never from the mirror. A declaration keeps that structural: the adapter
  names which typed address, and knobas resolves it (`clone_urls::addresses`) and appends
  `/<owner>/<repo>.git` with `knobas_core::checkout::clone::clone_url`, whose scheme, host and
  query checks are unchanged. *Clone* reads the adapter's template descriptor, so it builds no
  adapter and asks the keychain nothing, as before. An external adapter answers it inside its
  descriptor JSON, which the 1198 entry already lets grow by a `#[serde(default)]` field, so the
  read reaches a WASM component with no new WIT function and no new interface version. A config
  value is typed only while no adapter can write it: the scheduler fills config solely under its
  `DISCOVERABLE` keys (Jira's field ids), so a key `clone_from` names must never join that list.

  **The battery, clause 10.** `role::check` holds three things beside the minimum: a code-host
  claim that reads `clone_urls` names at least one address (`NoCloneAddress`); a `clone_from`
  that names any address comes with a code-host claim that reads `clone_urls`
  (`UnclaimedCloneAddress`); and a `Config(key)` names a string property of `config_schema`
  (`UndeclaredCloneKey`), so the address is one the Add-source form asked for.

  **Clone.** `knobas_app::checkout::clone::start` takes the adapter registry and drops its
  `kind != "gitea"` refusal. A repository whose source's adapter is not installed, or whose
  adapter names no address the instance fills, is refused `Invalid`; the first address
  `clone_url` admits wins, and when none does, the first refusal is the answer. `clone_repo`
  reads the registry from the sources state, so it answers `NotReady` until the sync engine is
  up. Its IPC arguments and answers do not change.
