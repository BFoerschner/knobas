- **IPC command semantics, issue #1628 (2026-10-22): a write waiting `unauthorized` is not sent
  again by the tick. `flush_writes` sends it, and `set_source_secret` and `update_source` release
  it.** Spec #1618, ruling A2, under Björn's 2026-10-22 waiver for this spec's frozen-surface
  steps. No signature changes, no migration, no event, no new module in `commands/` or `ipc/`.
  The generated `sources.ts` changes only in the doc comments on `QueuedWrite.wait_reason` and
  `WaitReason`.

  **Why.** The tick called `flush_all` every 5 s and re-sent every pending head write. A write
  refused with `401` was sent again with the same credential every tick: 17,280 a day, and an
  instance with a login-failure lockout (Jira DC's CAPTCHA, an LDAP policy) locks the account the
  reader is about to re-enter. `docs/development.md` already gave syncs the rule "a 401 does not
  retry at all". Writes now follow it.

  **The tick** (`knobas_sync::write_queue::flush_all`, and `flush_source`, which a submit,
  *Send mine anyway* and an amend run): a head write is sent when it has never been tried, or a
  release has cleared its reason; never when it waits `unauthorized`; and, when it waits
  `unreachable`, once `config::backoff_after(attempts)` (P7's 1, 2, 5, 15, 60 minutes) has passed
  since `attempted_at`. A source whose health is `unauthorized` or `missing_secret` is skipped
  whole, the test `trigger_all` makes.

  **`flush_writes`** (*Flush now*) sends every head write of the source, or of every source when
  `source_id` is `null`, once, whatever it waits on, and skips no source for its health. Later
  passes of the same flush, which let an entity's next write follow one that landed, use the
  tick's test, so a press sends a write at most once. It still never releases a held write.

  **`set_source_secret`** releases the source's `unauthorized` writes (`wait_reason` back to
  `null`, `detail` kept) once the keychain holds the new credential: the secret, the Kuma account
  beside it, or both, whatever the connection test answers. **`update_source`** releases them
  when the patch carries `base_url` or `config`. An adapter that would not build waits under the
  same reason. A release writes no activity line. A refusal that comes back after a release, sent
  with the credential it replaced, is not recorded.

  Witnessed by `crates/knobas-sync/tests/it/write_queue.rs` (the not-resent, release, stale-refusal,
  ladder and *Flush now* tests), `write_queue_flush_all.rs`
  (`the_tick_skips_a_source_whose_credential_needs_a_human`) and
  `crates/knobas-app/tests/it/unauthorized_write.rs` (the IPC seams).
