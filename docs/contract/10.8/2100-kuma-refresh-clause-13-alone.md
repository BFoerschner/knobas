- **`crates/knobas-source/src/**`: `contract::refreshes_as_it_syncs`, battery clause 13 alone —
  issue #2100 (2026-10-25), ADR-0031.** Parent spec #2085. No change to the `Source` trait, its
  DTOs or the battery's clauses; no migration, no IPC change, no WIT change.

  One new public function beside `contract::battery` and `contract::fills_its_roles`:

  ```rust
  pub async fn refreshes_as_it_syncs(source: &dyn Source, items: &[SyncItem])
  ```

  It runs the battery's own clause 13 (`check_refresh`, unchanged) over `items`, a batch the
  caller's sync of `source` emitted, against `source.descriptor()`. For a live suite whose server
  moves under clause 2 (Uptime Kuma rewrites a response time on every heartbeat, so `live_kuma`
  runs the full battery against a recording only): the live suite hands it a poll's batch that
  carries a paused monitor's tombstone, so clause 13 sees *gone* on the real server. A caller of
  the check runs the same assertions the battery runs; no clause can drift between the two.
  Witnessed by `live_kuma`'s
  `the_refresh_reads_a_monitor_as_the_run_does_and_a_paused_or_deleted_one_as_gone`, through the
  account source and the key-only source.

  The rest of #2100 (the Kuma adapter's `Source::refresh`, `Capability::Refresh` on its template,
  the socket.io `getMonitor` read) is in `crates/knobas-source-kuma/**` and is recorded in
  `docs/contract.md`, "Amendments from the Uptime Kuma refresh". Ruled by the merge-manager
  (Opus): §10.8 lists "any change to `crates/knobas-source/src/**` (… the contract battery)", and
  #1199's `fills_its_roles` took an entry for the same shape of addition.
