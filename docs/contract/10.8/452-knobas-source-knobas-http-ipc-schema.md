- **`crates/knobas-source/src/**`, `crates/knobas-http/**` and the IPC schema — issue #452
  (2026-09-07): two write ops, a second credential, and the surfaces that carry it.**

  The Uptime Kuma **write** half, which the #442 entry above promised would owe a real entry. The
  argument behind every line here is **§9's *Amendments from the Uptime Kuma adapter, write half***;
  this is the list of frozen surfaces it touches. **Ratified by the orchestrator under #452's own
  third criterion, which asks for these entries by name. Björn keeps the gate for frozen contracts
  and this entry is flagged for his review.**

  **`crates/knobas-source/src/**` — four additions, all of them additive:**

  1. `WriteOp::PauseMonitor { entity }` and `WriteOp::ResumeMonitor { entity }`, identifiers
     `pause_monitor` and `resume_monitor` (ADR-0006's per-milestone growth). A pair rather than one
     op with a flag: an action bar renders one button per identifier and a queue row records which
     act is owed in its `op` column. Every device that forces a decision on a new variant was fed:
     `WriteOp::identifier`, `knobas_sync::write_queue::target_entity`,
     `knobas_core::write_queue::PROJECTED_OPS` and `project` (both on **liveness alone** — see §9),
     `contract::known_write_ops`' probes, `every_write_op_has_a_stated_projection`,
     `every_write_op_says_whether_a_withdrawal_can_leave_one` (both `false`: a paused check is the
     thing the reader asked for, and a withdrawn pause leaves the monitor as it was), and the four
     sibling adapters' exhaustive `write` matches, which refuse both by name.
  2. `SourceInstance::account: Option<instance::Account>` — the optional second credential an
     adapter is handed, `None` for every source but a Kuma with an account. `Debug` redacts the
     password and keeps the username, because a refused login has to say *as whom*.
  3. `instance::Account { username, password }` — one struct rather than two `Option<String>`
     fields, because half an account is not a weaker account.
  4. `SourceDescriptor::accepts_account: bool`, `#[serde(default)]` so a descriptor from a peer
     built before this still decodes as the one-credential adapter it was. `true` for `kuma` and
     `false` for every other adapter.

  **`crates/knobas-http/**` — one addition:** `Request::text(String)`, sending the body verbatim
  with `Content-Type: text/plain;charset=UTF-8`. It exists because a socket.io emit is a *packet*
  and not a document (`42["pauseMonitor",8]`), which `Request::json` cannot carry in any spelling;
  the alternative was a second `reqwest` client inside the adapter, past the rate limiter and the
  retry budget, which is what that crate exists to prevent. `HttpClient::send`, its retry loop, its
  limiter, `SEND_BUDGET`, `MAX_ATTEMPTS` and the classification are **untouched**.

  **The IPC schema — four shapes, no new command:**

  * `SecretInput` becomes `{ value?: string | null, account?: { username, password } | null }`.
    `value` was a required `string`; it is now optional, and **absent means keep what is stored**
    for both halves. One rule read in both directions: a reader adding an account to a Kuma cannot
    retype an API key Kuma showed them once (story 69's "without re-entering the key"), and a
    reader replacing an expired key must not silently lose the account beside it. `add_source` is
    the one caller for which *keep* has no meaning and it refuses a missing `value` by name.
  * `AccountInput { username, password }` — a new DTO riding inside `SecretInput`, its own type
    rather than the SPI's `Account` re-used, so the day a stored account grows a field the form and
    the keychain can move one at a time.
  * `SourceDescriptor.accepts_account` on the wire, mirrored in `app/src/lib/ipc/sources.ts`.
  * `MonitorRow.actions: string[]` — the `WriteOp` identifiers the Monitors tab may offer on that
    row, already filtered by what the source declares *and* by what the row's state has a use for.
    The inbox's `actions` precedent exactly. `WriteOpPayload` in `sources.ts` gains the matching
    `PauseMonitor` and `ResumeMonitor` branches.

  **What is not touched.** **No migration** — nothing here is stored in Postgres; the credential is
  in the keychain and `actions` is derived per read. **No new command, no argument change, no
  event change**; the `commands/` + `ipc/` module layout is unchanged and **neither append-only
  barrel grows a line**. `crates/knobas-app/src/{error,profile}.rs` are untouched. `monitor_roster`
  gains an injected `AppHandle`, which Tauri supplies and which changes nothing on the wire — and
  it answers an empty `actions` rather than a refusal when the sync engine has not started, so the
  tab still draws during bring-up.

  **The keychain envelope goes to version 2** (interfaces §3's convention, which §10.8's list does
  not name but #452's criterion does): the same one item per source, now able to carry an optional
  `account` beside the secret, with a v1 item still reading as the one-credential source it always
  described. §9's section argues the shape and why it is one item rather than two.

  Pinned by: `knobas_secrets`' `an_account_rides_beside_the_secret_in_the_same_item`,
  `a_version_one_item_still_reads_as_a_source_with_one_credential` and
  `an_unknown_envelope_version_is_refused_rather_than_misread` (now reading its probe off
  `READABLE_VERSIONS`), plus `the_keyring_store_honours_the_store_contract` against the **real**
  macOS keychain; `knobas_source`'s `write_op_round_trips` and
  `every_op_has_its_own_snake_case_identifier`; `sources_mirror.rs`'s
  `every_write_op_variant_is_declared_in_the_mirror` and `the_descriptor_shape_matches_its_typescript_mirror`;
  `commands::assets`' `the_monitor_roster_matches_its_typescript_mirror`; the Kuma crate's
  `an_account_is_what_turns_the_write_ops_on` (the battery over **both** instances) and
  `an_account_is_what_gives_an_instance_its_write_ops`; `assets::offer_actions`'
  `a_row_is_offered_the_action_its_state_has_a_use_for`; `SourceRow.test.svelte.ts`'s
  *a healthy source whose adapter takes an account is offered one*; and, for the halves no test can
  reach without the real server, `just kuma-live`'s
  `a_monitor_paused_through_the_write_op_leaves_the_roster_and_comes_back`,
  `only_a_source_with_an_account_offers_the_write_ops`,
  `a_refused_account_and_a_refused_write_are_different_faults` and — one seam higher, which is what
  the criterion's *through the queue* asks for — `kuma_write_live.rs`'s
  `a_pause_queued_through_the_write_queue_reaches_kuma_and_the_mirror` and
  `a_pause_on_a_source_with_only_its_api_key_is_refused_before_it_is_queued`.

