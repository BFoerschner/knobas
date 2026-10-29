- **`crates/knobas-source/src/**` and the IPC schema — issue #453 (2026-09-07): the create, what
  it targets, and the name that attaches it.**

  The last of the Uptime Kuma write half's three ops, and the one whose consequences reach past the
  adapter: a create needs somewhere to put what it made, and something has to join it to the asset
  it was made for. The argument behind every line here is **§9's *Amendments from the Uptime Kuma
  adapter, create***; this is the list of frozen surfaces it touches. **Ratified by the orchestrator
  under #453's own third criterion, which asks for this entry by name. Björn keeps the gate for
  frozen contracts and this entry is flagged for his review** — and in particular the SPI's new
  free function, the `AssetEdit` variant, and the sync engine now writing links.

  **`crates/knobas-source/src/**` — three additions, all additive:**

  1. `WriteOp::CreateMonitor { entity, name, url }`, identifier `create_monitor` (ADR-0006's
     per-milestone growth). `entity` is the **container**, as every other create in that enum
     spells it, and for a monitoring source the container is the source itself. **No
     `monitor_type`**: a monitor of a URL is an HTTP check, and a field with one legal value is one
     every adapter would have to vet. Every device that forces a decision on a new variant was fed:
     `WriteOp::identifier`, `knobas_sync::write_queue::target_entity`,
     `knobas_core::write_queue::PROJECTED_OPS` and `project` (liveness alone, and the clause has no
     teeth — the roster is a container knobas never mirrors, so both projections are
     `{"live": false}` and the write always sends, which is `create_ticket`'s reading),
     `knobas_sync::write_queue::UNCLAIMED_OPS` (**in**, `create_page`'s shape: a withdrawn create
     leaves a check running that no knobas row claims, and a monitor's *name* is a label rather than
     an address — Kuma holds any number of monitors called the same thing),
     `contract::known_write_ops`' probes, `every_write_op_has_a_stated_projection`,
     `every_write_op_says_whether_a_withdrawal_can_leave_one`, and the four sibling adapters'
     exhaustive `write` matches, which refuse it by name.
  2. `knobas_source::monitor_target(source_id)` — the entity id a create against that source
     addresses, `<source id>:monitors`, spelled **once and in the SPI** (its key half is a private
     constant: nothing outside that module has a use for half an address). The alternative was every producer of the op composing it, which is a per-adapter table
     in the app of exactly the kind `config_schema` and `accepts_account` exist to forbid. The key
     is not decimal and a Kuma monitor id is, so a create's target cannot collide with a monitor
     (`the_create_target_is_not_a_monitor_id`). **Not called a *roster***: that word is the
     Monitors tab's list of rows in `CONTEXT.md`, and this is an address.
  3. `contract::battery`'s probe list grows a `CreateMonitor` aimed at a container **no source
     has** — a create is the one probe whose success would leave something behind.

  **The IPC schema — three shapes, no new command:**

  * `AssetDetail.monitor_targets: MonitorTarget[]`, `#[serde(default)]`. The sources a *Create
    monitor for this asset* would go to. **Empty is the ordinary answer and it is #453's second
    criterion**: a Kuma with only its API key declares no write ops, so it is not in the list and
    the pane draws no control. Filled by `commands::assets::get_asset`, which gains an injected
    `AppHandle` — `monitor_roster`'s arrangement since #452, supplied by Tauri, nothing on the wire.
  * `MonitorTarget { source_id, display_name, entity }` — a new DTO. `entity` is composed by the
    backend from `monitor_target`, so the form submits an entity it never spelled.
  * `AssetEdit` gains `Monitors { added: string[] }` — the **one variant in that union that
    appends**. It reaches `0020`'s column, `ADD_MONITORS`' `array_cat` and the `edited` /
    `field: "monitors"` line the import already writes, from the other of the two surfaces that
    name a monitor. `WriteOpPayload` in `sources.ts` gains the matching `CreateMonitor` branch.

  **`crates/knobas-sync/**` is not frozen** (this section says so in as many words) and is where
  the new module lands: `knobas_sync::attach::resolve` draws the `monitored-by` links an asset's
  recorded names have earned, inside the run's transaction, after the alert reconcile. Recorded
  here rather than left out because it is the half of spec #427's *Import* sentence that had never
  been built — *"resolved by the next import **or the M4.1 sync**"* — and because it means **the
  sync engine now writes `knobas.link` rows**, which it did not before. What bounds it: never
  removes a link, never touches a pair with an active row (confirmed *or* proposed — `0011`'s index
  is over the unordered pair whatever its confirmation, so #478's proposal is not something to
  insert over), never attaches a tombstoned monitor, and writes no activity line (the name's own
  line is written where the name is added).

  **What is not touched.** **No migration** — `knobas.asset.monitors` is `0020`'s column and
  `knobas.link` is `0001`'s. **No new command, no argument change, no event change**; the
  `commands/` + `ipc/` module layout is unchanged and **neither append-only barrel grows a line**.
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched. The
  keychain envelope is unchanged at version 2.

  Pinned by: `knobas_source`'s `the_create_target_is_not_a_monitor_id`, `write_op_round_trips` and
  `every_op_has_its_own_snake_case_identifier` (now fourteen); `knobas-sync`'s
  `every_write_op_has_a_stated_projection` and
  `every_write_op_says_whether_a_withdrawal_can_leave_one`; `sources_mirror.rs`'s
  `every_write_op_variant_is_declared_in_the_mirror`; `commands::assets`'
  `the_asset_detail_matches_its_typescript_mirror`,
  `only_an_instance_that_offers_the_op_is_a_create_target`,
  `only_a_kind_whose_ops_could_include_it_is_worth_a_keychain_read` and
  `the_pane_offers_the_create_the_spi_names_at_the_target_the_spi_spells`; the Kuma crate's
  `the_document_is_the_seeds_own` (which parses `testenv/kuma-monitor.mjs`),
  `a_create_knobas_cannot_deliver_is_refused_here`,
  `the_minted_id_is_read_out_of_kumas_own_answer`,
  `a_creates_target_is_this_sources_own_or_it_is_refused_here` and
  `a_create_without_an_account_is_refused_by_the_same_sentence`; ten tests in
  `knobas-sync`'s `tests/attach.rs`, all driven through `run_once` rather than through the
  resolver — the last of them,
  `a_poll_ignores_monitor_rows_its_descriptor_does_not_declare`, is why `run_locked`'s descriptor
  gate is a **safety check** here and not `!sweep_kinds.is_empty()`'s saved round trip: the mirror
  can hold `kind = 'monitor'` rows under an id whose current descriptor has no such kind, since
  `sync.item.source_id` has no foreign key to `knobas.source_config`, `delete_source`'s
  `purge_items` is the reader's choice, `0012` leaves unconfigured rows visible, and `add_source`
  plans for an id being taken again (#127);
  five in `tests/assets_ipc.rs` for the append, the duplicate, the blank and the absent
  target; six in `create-monitor.test.ts` and seven in `AssetsView.monitor.test.svelte.ts` for the
  prefill and the two writes' order; and — for the halves no test can reach without the real
  server — `just kuma-live`'s
  `a_monitor_created_through_the_write_op_is_published_under_the_id_it_answered` and
  `a_monitor_created_through_the_write_queue_is_mirrored_and_attached_to_its_asset`.

