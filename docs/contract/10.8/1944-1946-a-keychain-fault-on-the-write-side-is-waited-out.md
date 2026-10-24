- **IPC command semantics, issues #1944 and #1946 (2026-10-24): a write pressed while the
  keychain is locked is queued, a credential save releases a write waiting on the keychain, and
  a keychain error says `keychain:` once.** Ruled by the implementer on Opus
  (`docs/decisions/2026-10-unattended-rulings.md`, #1944/#1946). No signature change, no new
  variant, no migration, no event, no new module in `commands/` or `ipc/`. The generated
  `sources.ts` changes only in the doc comment on `WaitReason`.
  `knobas_sync::write_queue::release_unauthorized` and its `knobas_core` statement are renamed
  `release_credential_waits`. They are crate functions, not commands.

  **`submit_write`** (and every path through `sources::write_queue::submittable`: the awaited
  submit, the worklog, standup, action-item and docs writes, start-work's steps) checks an op
  against the kind's template plus what the instance offers. When the keychain will not open
  (`SecretError::Backend` or `Unavailable`), the instance half is no longer read as empty. It is
  read as every write the kind's claimed roles allow (`knobas_source::role::Minimum::writes`),
  so a Kuma pause or resume pressed while the keychain is locked is queued. It used to answer
  `invalid` ("does not offer"). The flush then waits the keychain out on the sync's ladder, as
  #1741 made it. If the instance turns out not to offer the op, the adapter refuses it at the
  flush (battery clause 5). An op no role of the kind allows is still `invalid` at the call.
  `crud::instance_write_ops`, which draws the Monitors tab's buttons, still answers an empty
  list for a locked keychain.

  **`set_source_secret`** releases the source's writes waiting on its credential: those waiting
  `unauthorized` (#1628), and those waiting `unreachable` whose `detail` starts
  `KEYCHAIN_DETAIL_PREFIX` (#1741). Their `wait_reason` goes back to `null`, `detail` is kept,
  and the next flush sends them instead of waiting out the rung. **`update_source`** releases
  the same set when the patch carries `base_url` or `config`. A keychain fault that a flush met
  before a save it overlapped is not recorded, as a 401 from the replaced credential is not
  (#1628). A write waiting on a server that did not answer is not released.

  **Error text.** `SecretError::Backend` prints the store's own words, as `Unavailable` and
  `NotFound` do. The wrapper says they are the keychain's: `SourcesError::Secret`
  (`keychain: {0}`, what the sources commands that read or write the keychain answer), the sync's
  `RunFailure::message` (the queue's `detail`, unchanged), and the importer's `IpcError`
  (`keychain: …`, as before for `Backend` and now for `Unavailable` too). A locked keychain
  in a sources command used to read `keychain: keychain: User interaction is not allowed.`

  Witnessed by `crates/knobas-app/tests/it/keychain_write.rs` (the save and the quit),
  `crates/knobas-sync/tests/it/write_queue.rs`
  (`a_credential_save_releases_a_write_waiting_on_the_keychain`,
  `a_keychain_fault_met_across_a_credential_save_does_not_put_the_write_back_to_wait`, and the
  two #1741 exact-detail tests), and the `knobas-app` unit tests
  `a_pause_pressed_while_the_keychain_is_locked_is_queued_not_refused`,
  `a_keychain_error_names_the_keychain_once` and `a_locked_keychain_names_the_keychain_once`.
