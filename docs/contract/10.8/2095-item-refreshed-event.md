- **The event schema: `item:refreshed` — issue #2095 (2026-10-25), ADR-0031.** Parent spec #2085.
  No command, no migration.

  **One event**, `knobas_app::events::ITEM_REFRESHED = "item:refreshed"`, broadcast, mirrored in
  `app/src/lib/ipc/index.ts`'s `EVENTS.itemRefreshed`. Payload
  `knobas_sync::scheduler::RefreshEnded { entity_id: String, write_id: Option<i64>, ended:
  RefreshEnd }` with `RefreshEnd` one of `refreshed`, `gone`, `failed` (snake_case), both in the
  generated `app/src/lib/ipc/generated/sources.ts` (`ipc_mirror`) and re-exported from
  `app/src/lib/ipc/sources.ts`. Sent once per refresh that ended, after its item and the links on
  arrival it settles are written, so a view re-reading on it sees both; `write_id` is the queued
  write the refresh followed, `null` for one the reader asks for (#2107). A refresh that did not
  happen sends nothing: not offered by the adapter, or refused before anything was read (a
  turned-off or unconfigured source, an id that is no entity's).

  It reaches the bridge through one new `SyncEvents` method with a default,
  `refresh_ended(&self, &RefreshEnded)`, told by `knobas_app::sources::refresh::refresh`, the one
  way the app refreshes; `TauriEvents` emits it and every wrapper (`FollowLandings`,
  `ArrivalLinks`, `SuggestOnSync`) passes it on. Coarse state, one per refresh, inside the
  "events are not for throughput" rule. No listener in the shell yet: the echo lifecycle
  (#2102–#2105) and the detail's *Refresh* (#2107) are its readers.

  Witnessed by `sources::events::tests::a_refresh_that_ended_reaches_the_shell_with_the_entity_and_the_write`
  (the real `TauriEvents`, listened for by name), `the_event_names_match_their_typescript_mirror`,
  and the `tests/it/refresh.rs` assertions on what the scheduler's events were told.
