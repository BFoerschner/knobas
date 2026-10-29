- **`knobas_core::payload` and `crates/knobas-source/src/**` — issue #1020 (2026-10-17):
  `KindPaths.owner` and `KindPaths.monitor_type`, Uptime Kuma's `state` chip and type, Gitea's
  repository owner.** The set editor's monitor row draws its type and its state's dot, and a
  repository row its teams, else its owner (`link_sets::MEMBER_ROWS`). No migration, no grammar
  key, no `WriteOp`, no new command, no `commands/` or `ipc/` module.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  `owner: Vec<PayloadPath>` and `monitor_type: Vec<PayloadPath>`, both `#[serde(default)]`, in
  `field::ALL` as `field::OWNER` and `field::MONITOR_TYPE`. The contract battery weighs both as
  `Wanted::Text`. Mirrored in `app/src/lib/ipc/sources.ts` and held by `sources_mirror.rs`.

  **Uptime Kuma** declares a monitor's `state` chip over the top-level `state` its `map` already
  writes: `up` ok, `down` err, `pending` warn, `maintenance` muted; a null `state` draws no chip.
  Every reader of `declared_state!` now draws a monitor's dot. It declares `monitor_type` at the
  top-level `type` (`http`, `ping`). `status_name` is unchanged.

  **Gitea** declares a repository's `owner` at `owner.login`, Gitea's own record.

  **The member row's line** (`link_sets::line_of`): an asset's type and environment, else a
  mirrored item's chain of groups, a monitor's type, a repository's teams (`declared_list!`,
  joined `, `), its owner, else its project and parent chain. The `LinkSetMemberRow` wire shape is
  unchanged. Witnessed by `link_set_patterns_ipc.rs`'s
  `a_monitor_row_draws_its_type_and_dot_and_a_repository_its_team_or_owner`; the fake IPC follows
  (`fake/link-sets.ts`, `fake-features.test.ts`).
