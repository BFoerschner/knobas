- **The IPC schema — issue #1571 (2026-10-21): `InboxCategory` gains `failed_sign_in`, and
  `knobas.inbox_state` gains the key `failed_sign_in:<source_config.id>`.** Follow-up of #1439. A
  source whose sign-in is rejected (`unauthorized`) or which has no stored secret
  (`missing_secret`) is an inbox item, and Settings › Notifications has a seventh switch that
  routes it. No new command, no argument changed, no event added, no DTO field, no `WriteOp`,
  nothing in `crates/knobas-source/src/**`, no `commands/` or `ipc/` module, nothing added to either
  append-only barrel. The column the rule reads is migration `0055`, recorded in its own entry
  (`1571-migration-0055-auth-failing-since.md`). Ratified by the implementer on Opus under
  Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1571`.

  **The wire change, in full.** One union grows and no DTO does:

  ```ts
  export type InboxCategory =
    | "review_request" | "mention" | "failed_build" | "new_assignment"
    | "credential_expiry" | "failed_sign_in" | "alert";   // ← the seventh
  ```

  mirroring `knobas_core::inbox::Category::FailedSignIn`, generated into
  `app/src/lib/ipc/generated/entity.ts` (§2.6 as amended by #1325). It sits after
  `credential_expiry` in `Category::ALL`. That order is the order of the switches and of the stored
  `inbox.notification_kinds` list (`canonical`), and a list stored before this change reads back
  unchanged. `InboxItem` keeps its ten fields. For this category they carry what a credential
  expiry carries: `entity_id` and `kind` null, `source_id` the source, `title` its display name,
  `web_url` null. `reason` is `the <name> sign-in was rejected` or
  `no credential is stored for <name>`. `occurred_at` is `auth_failing_since`, the start of the
  episode. `actions` is always empty (`candidate_ops`), and the row opens the source's page in
  Settings, where *Re-enter* is.

  **The `inbox_state` key.** `failed_sign_in:<source_config.id>`, opaque text like every other key.
  No column or constraint holds a category, so nothing in the table moves. It is the same key for
  every failure of the source. That makes *done* and *snooze* answers about the episode, because
  `done_at >= occurred_at` hides the item until a later episode starts.

  **The notification setting.** `inbox.notification_kinds` accepts the new word through
  `set_notification_kinds`' existing parse against `Category::ALL`. Off by default, like every kind
  (story 71). The switch reads *A source cannot sign in*: prototype-6's words, which #1439 took off
  the `credential_expiry` switch because no rejected sign-in fired that category. That switch keeps
  #1439's *A source's credential is about to expire*.

  **One subscription, no new event.** The shell's inbox store re-reads on the existing
  `source:health` event as well as `activity:new` and `sync:state`. *Test connection* and
  *Re-enter* settle a verdict with no run behind them, so `sync:state` alone would not show the
  item arriving or leaving.

  Pinned by `crates/knobas-core/tests/it/inbox.rs`' four new tests (the two states and the four
  negatives; what the row says; once per episode with *done*; leaving when the sign-in works);
  `crates/knobas-app/tests/it/inbox_ipc.rs`'
  `a_failed_sign_in_is_an_item_once_per_episode_through_the_health_writer` (through
  `knobas_sync::config::set_health` to `inbox_items`); the existing
  `every_category_has_a_rule`, `every_inbox_category_has_a_toggle_in_the_interface` and
  `every_category_this_build_has_is_stored_and_read_back`, which now walk seven; and on the frontend
  `tf/inbox/inbox.test.ts`, `inbox/notify.test.svelte.ts`, `inbox/inbox.test.svelte.ts`,
  `tf/settings/TfSettingsSections.test.svelte.ts` and `shell/dev/fake-features.test.ts`.
