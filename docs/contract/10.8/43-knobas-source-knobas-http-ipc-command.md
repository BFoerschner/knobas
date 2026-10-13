- **`crates/knobas-source/src/**`, `crates/knobas-http/**`, and the IPC command schema with
  both append-only barrels, issue #43 (2026-08-29):** M2's write-back set. **ADR-0006
  (`docs/adr/0006-writeop-grows-per-milestone.md`, accepted 2026-08-29) is the decision this
  entry records**, and it says exactly what this is: `WriteOp` grows per milestone, and each
  growth is a §10.8 ratified exception naming the variants and the adapters that declare them.
  This is the first growth under it. Granted by the **orchestrator**.

  **The merge gate, and a widening of the delegation recorded here because this entry is the
  first thing it covers.** Björn's 2026-08-29 delegation ("let Migration and ipc additions be
  merged by fable too") covered migrations and IPC additions; two of this entry's three
  surfaces — the `WriteOp` growth and `knobas-http` — are neither, so the question was put to
  him rather than stretched over them. Björn ruled the same day that the delegation covers
  them ("yes it does"), so the frozen-contract gate delegated to Fable merge-managers now
  covers migrations, IPC additions, `crates/knobas-source/src/**` (including `WriteOp` growth
  under ADR-0006) and `crates/knobas-http/**`. Milestone exits and the contract battery's
  clauses were not delegated and remain his.

  **The SPI.** `WriteOp` gains **seven** variants, one per operation and never per adapter,
  each with its stable snake_case identifier:

  | variant | identifier | declared by |
  | --- | --- | --- |
  | `Transition { entity, status }` | `transition` | Jira |
  | `CreateTicket { entity, title, body, ticket_type }` | `create_ticket` | Jira |
  | `CreateBranch { entity, name, from_ref }` | `create_branch` | Gitea |
  | `CreatePullRequest { entity, title, body, head, base }` | `create_pull_request` | Gitea |
  | `Approve { entity, body }` | `approve` | Gitea |
  | `TriggerBuild { entity }` | `trigger_build` | TeamCity |
  | `RerunBuild { entity }` | `rerun_build` | TeamCity |

  `Comment` is unchanged and is now declared by **two** adapters, Jira and Gitea, which is the
  point of one variant per operation. The full declared table after this change is: mock
  `comment`; Jira `comment`, `transition`, `create_ticket`; Gitea `create_branch`,
  `create_pull_request`, `comment`, `approve`; TeamCity `trigger_build`, `rerun_build`. It is
  pinned as a whole by `knobas-app`'s `the_registry_declares_exactly_the_write_set_m2_ratified`,
  which replaces the M1-era `no_real_adapter_declares_a_write`: an adapter that quietly started
  declaring an unratified op passes its own crate's tests and fails there.

  Nothing else in `crates/knobas-source/src/**` changes. `WriteOp::identifier` keeps its **missing
  wildcard arm** — ADR-0006's forcing function, and the reason this package could not have shipped
  half-wired. `contract.rs` gains a **probe value per new variant** in `known_write_ops` and
  **no battery clause is added, removed or reworded**; the only other edit there is in that
  module's own test-support adapter, whose `write` now panics for the op it *declares* rather
  than for any op, because the battery legitimately probes the seven it does not.

  **Every variant carries `entity`**, and it is the same field everywhere: the target the adapter
  resolves to a path, the ordering key the queue keeps per-entity order within, and what hold
  detection snapshots. For an op that *creates*, the entity is the **container** — the Jira
  project (`jira:PAY`), the Gitea repository. A container knobas does not mirror is a legal
  target: migration `0005` has no foreign key on `entity_id`, and hold detection reads "not in
  the mirror" as a fact rather than an error.

  **Hold projections, stated per op** (`knobas_core::write_queue::project`, `PROJECTED_OPS`).
  Three shapes, and which one an op gets is a statement about what that op could *overwrite*:

  - **the indexed text** — `comment`, unchanged from #42;
  - **the whole mirrored record** — `transition` and `approve`, the two ops that put a
    *judgement* onto a target whose current state is the reason for the judgement;
  - **liveness alone** — the five additive ops, which add beside what the container holds and so
    overwrite nothing. What still holds them is the target leaving the mirror; a duplicate is the
    source's refusal to give, not a hold.

  **`transition`'s whole-record projection was a decision, and the alternative was rejected
  rather than overlooked.** What one would rather compare is the status alone, and there is no
  adapter-independent way to read it: §4.1 guarantees `title`, `body_text`, `updated_at` and a
  verbatim `payload`, and the status lives only in the last of those — `fields.status.name` for
  Jira, `state` for Gitea. Reading it in `knobas-core` would mean the store, which cannot see
  `WriteOp` at all, learning every source's payload shape; that is the coupling
  `SourceDescriptor` exists to avoid, and #43's spec forbids "an adapter-aware surface" in terms.
  The cost is noise: a comment arriving while the source is down holds a queued transition. The
  cost of the other direction is moving a ticket somebody else already moved, silently — the one
  thing #42 exists to prevent. A false hold shows both versions side by side and is one *Apply
  anyway* away; a missed hold shows nothing. **If the noise becomes real**, the shape to reach
  for is a per-op projection the *descriptor* declares (self-describing, no trait change, no
  adapter table downstream) — which is a new frozen-surface field and therefore its own ADR-0006
  conversation, not a follow-up tidy-up.

  **`crates/knobas-http`.** One additive method: `Request::json(&T)`, which serializes `T` as the
  request body with `Content-Type: application/json`. Every op in the ratified set is a `POST`
  carrying a JSON document, and `Request` had `query` and `header` and nothing else — so an
  adapter needing a body would have had to build a second `reqwest` client beside the rate
  limiter and the retry budget, which is the one thing that crate exists to prevent. **No existing
  signature changes**, `HttpConfig` is untouched, and every other guarantee holds: one door onto
  the wire, one fault mapping, the same attempts, budget, `Retry-After` and rate limit. The body
  is **buffered rather than streamed**, deliberately: `send` retries by `try_clone`, which answers
  `None` for a streaming body, so a write gets the same three attempts every read gets.
  `tests/transport.rs` gains two tests and nothing else in the crate changes.

  **The IPC schema and both append-only barrels.** One new command, in the **existing** `entity`
  module and its existing mirror — the `commands/` + `ipc/` layout is frozen and no module is
  created on either side:

  ```rust
  #[tauri::command] pub async fn submit_write(.., payload: serde_json::Value)
      -> Result<knobas_core::write_queue::QueuedWrite, IpcError>;
  ```

  with `submitWrite` in `app/src/lib/ipc/entity.ts`. One line appended to
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing `commands::entity::`
  group. No existing command, DTO field or event name changes, and **no new event** — for #42's
  reason, unchanged: every queue transition already writes an activity line, so `activity:new` is
  the signal.

  Three shape decisions a later reader might undo without realising what they were for:

  - **`payload` is untyped on the wire**, exactly as `amend_write`'s is and for the same reason:
    `WriteOp` grows per milestone, so typing the argument would drag the SPI's enum onto the IPC
    surface and make every growth an IPC change.
  - **There is no `source_id` argument.** §4.1 makes the instance id and the `EntityRef` namespace
    the same string, so the target already names the source; a second argument could only agree or
    contradict, and a contradiction would aim a write at a source the target does not belong to.
  - **It returns the write *as queued*, before the attempt.** The queue decides send, pend or
    hold; a command that reported "sent" would be reporting a hope. `pending_writes` is the
    outcome read.

  One DTO widens on the TypeScript side only: `WriteOpPayload` becomes an eight-member union,
  which `sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror` requires — a
  variant with no branch there is a held write the reader can apply and discard but not edit.
  `editableBody`/`withBody` still find words only in a `Comment`, so *Edit and send* is offered
  for that op alone; #42 designed that degradation (`null` means "no edit box", not "no actions")
  and widening it is #44/#45's when they need it.

  **One seam is deliberately not here, and #44 hits it first: there is no read of a ticket's
  *available* transitions.** The adapter resolves the status against the source's own list at
  write time and refuses by name — which satisfies "read from the source, not assumed" — but a
  UI that wants to *offer only the reachable statuses* has no method to ask through. `Source`
  has no such read and adding one is a frozen-trait change, so it needs its own grant here
  rather than arriving as a drive-by.

  **No migration.** `0005`'s `op` column deliberately has no CHECK, precisely so a `WriteOp`
  growth is not also a schema change. `0006` was taken by Notes v1 (#46) and `0007` by
  Suggestions (#41) while this was in flight; `0008` was the next free number and has since been
  taken by the start-work flow (#44), whose entry below records `0009` as the next.

  Outside the frozen list and noted because it is what the grant is for: each adapter gained a
  `write.rs` that **never names `WriteOp`** — the dispatch lives with its `impl Source for`,
  because `write_choke_point.rs` refuses any production file that names the op enum without
  implementing the trait, and an adapter's dispatch module that named it would read as a second
  write path. `HANDS_TO_THE_QUEUE` is unchanged and still has one entry.
  `knobas_sync::write_queue::target_entity` gains seven arms and keeps its missing wildcard.
  `knobas-mockd` grew four endpoints (`GET`/`POST api/2/issue/{key}/transitions`,
  `POST api/2/issue`, `POST /app/rest/buildQueue`); the Jira three were already in the generated
  WADL allowlist, and `GET .../transitions` joins the fidelity gate, so its body is validated
  against the WADL's own schema and goldened.

