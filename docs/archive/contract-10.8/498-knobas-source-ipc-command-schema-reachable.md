- **`crates/knobas-source/src/**` and the IPC command schema — issue #498 (2026-09-08): the
  reachable-transition read, one trait method and one IPC command.**

  The argument behind every line here is **§9's *Amendments from the reachable-transition read***
  above; this is the list of frozen surfaces it touches. **Ratified by the orchestrator under
  #498's first acceptance criterion, which asks for "the §10.8 entry [that] records the trait
  growth and the IPC read" by name. Björn keeps the gate for frozen contracts and this entry is
  flagged for his review — ratified in his absence by the deputy's ruling of 2026-09-08 on #498
  (`docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the trait
  method, on battery clause 8 — the strictest of the three, since #146 had reserved a battery
  clause change to Björn and this ticket's own first criterion asks for it by name — and on the
  IPC read, and found both this entry and §9's section sufficient as written.**

  **`crates/knobas-source/src/**` — two additions, both additive:**

  1. `Source::reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError>`,
     **with no default implementation** — the first growth of this trait since the M0 freeze, and
     the first *read* it has gained. `entity` is an `EntityRef` in string form, the address every
     `WriteOp` already carries; the answer is the statuses the entity's workflow offers from where
     it stands, in the source's own spelling, and nothing stores it. No default is the decision and
     §9 argues it: a default is either an adapter silently declining to answer or a new adapter
     shipping an empty select, and the compiler asking every implementation is ADR-0006's forcing
     function applied to a read. Every implementation in the tree answered it — the six adapters,
     `knobas_sync::progress::Observed` (delegating, reporting nothing), the battery's own
     `TestSource`, and the test fakes.
  2. `contract::battery` **clause 8**: an adapter whose descriptor does not declare the
     `"transition"` write op must refuse the read with `SourceError::Protocol`, naming its own
     `adapter_kind`. The declared side is skipped, not probed, for clause 5's reason. This is a
     new clause on the shared battery, so it is a new obligation on **every** adapter and on any
     adapter written later — which is the point of putting it there rather than in six suites.

  Nothing else in that crate moves: `SourceDescriptor`, `SyncItem`, `WriteOp` (`Transition`
  included, field for field), `WriteReceipt`, `SourceError`, `Sink` and `instance` are untouched,
  and no `WriteOp` variant was added — this is a read, and #491's out-of-scope list rules out a
  `ReadOp` enum, a default implementation and any stored copy.

  **The IPC schema — one new command, no DTO:**

  * `commands::entity::reachable_transitions(entity_id: String) -> Vec<String>`. A **new command**,
    which is the frozen surface this touches: the `commands/` + `ipc/` module layout is unchanged
    (it is a read on the entity module, the treatment `mini_board` got), and it takes no new DTO in
    either direction — an entity id in, an array of strings out — so `app/src/lib/ipc/entity.ts`
    gains one exported function (`reachableTransitions`) and no interface. There is no
    `source_id` argument: the namespace *is* the source (§4.1), `submit_write`'s reasoning
    exactly.
  * **One barrel grows a line, not two.** `crates/knobas-app/src/lib.rs`'s handler list gains
    `commands::entity::reachable_transitions`, which `wiring.rs`'s
    `every_command_is_in_the_handler_list` requires. `app/src/lib/ipc/index.ts` lists **modules and
    events**, not functions, and `./entity` is already re-exported — so it is unchanged, and that
    is the file being append-only rather than an omission.
  * **No event.** The answer is about now and is good for as long as the detail is open, so there
    is nothing to invalidate and nothing to announce.

  **What is not touched.** **No migration** — nothing here is stored, in Postgres or anywhere else;
  §9 says at length why there is nowhere it could be. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched, and the keychain envelope is unchanged
  at version 2. `sources::crud` gained a function that reuses `instance_from` and
  `Credential::keeping`, so the adapter this read builds is the one a scheduled run builds; that
  module is not on the frozen list.

  Pinned by: the three battery cases (`accepts_an_adapter_that_answers_the_read`,
  `rejects_a_source_with_no_workflow_that_answers_the_read`,
  `rejects_a_refusal_that_does_not_name_the_adapter`) plus every adapter's own
  `passes_the_contract_battery`; `wiring.rs`'s `every_command_is_in_the_handler_list`;
  `knobas-app`'s `tests/status_move.rs`'s
  `the_read_offers_the_workflow_and_a_source_without_one_refuses_by_name`; and — the witness ADR-0013
  asks for — `tests/atlassian_live.rs`'s
  `the_reachable_transitions_read_answers_the_seeded_workflow_from_every_state` against the real
  seeded Jira. §9 carries the full list and the measurements.

