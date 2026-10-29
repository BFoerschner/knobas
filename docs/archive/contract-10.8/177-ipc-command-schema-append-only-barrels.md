- **The IPC command schema and both append-only barrels, issue #177 (2026-08-30):** the M2.5 mini
  board's **one granted read** — the milestone's only frozen-surface touch, ratified in advance by
  the spec (#175) Björn approved: "One new additive IPC read command feeds the board … This is the
  milestone's only frozen-surface touch: one §10.8 entry, both append-only barrels appended, the
  commands/IPC layout, migrations baseline, and Source SPI untouched."

  **The exact signature**, in the **existing** `entity` command module (ADR-0009 names the feature
  the *mini board*; "board" never stands alone):

  ```rust
  #[tauri::command] pub async fn mini_board(.., ctx_id: Option<String>, sources: Vec<String>) -> Result<knobas_core::mini_board::MiniBoard, IpcError>;
  ```

  **It takes the room's scope, not just a context, and that is the whole of why it takes two
  arguments.** `app/src/lib/shell/contexts.ts` has two populations of room: a **stored** room is a
  `knobas.context` row and narrows by `context`; a **derived** room — *All work*, plus one per
  configured source — has no context row at all and narrows by `sources`. Exactly one of the two is
  ever narrowing, and `Room.svelte` already hands every tile both (`sources={context.filter.sources}
  ctx={context.filter.context}`). A command keyed on a context id alone would answer an empty board
  in the room every session starts in, which would make #178's "the empty state says there is no
  ticket in this room" a falsehood exactly where the user lands — so this is spec #175's story 10,
  "the mini board filtered by the room's context like every other tile", read as the other tiles
  implement it. Both narrowings are bound as **nullable parameters**, the discipline
  `commands::entity`'s room statements already record: `Some` of no members is a stored room that is
  honestly empty, `None` is a derived room that never asked.

  **The mirror**, appended to the existing `app/src/lib/ipc/entity.ts`: one function `miniBoard`
  (taking `Pick<EntityFilter, "sources" | "context">` — the room's own filter object, so a caller
  cannot narrow by one dimension and forget the other) and four interfaces — `MiniBoard { columns, sources }`,
  `MiniBoardColumn { status, cards }`, `MiniBoardCard { entity_id, source_id, key, title, priority }`
  and `SourceStatuses { source_id, statuses }`. All four are pinned by
  `the_mini_board_shapes_match_their_typescript_mirror` in `crates/knobas-app/tests/entity_mirror.rs`,
  with both nullable fields exercised as `None` per that file's rule.

  **Which barrels were appended**: one line in `crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, in the existing `commands::entity::` group, immediately after `context_members`; and the
  function plus four interfaces at the foot of `app/src/lib/ipc/entity.ts`, which
  `app/src/lib/ipc/index.ts` already re-exports wholesale (`export * from "./entity"`) — so the
  TypeScript barrel grows by that export and its own text is untouched. Neither barrel is rewritten.

  **What did not change.** No migration (`0012` is still the next free number, and the board reads
  `sync.live_item` and `knobas.confirmed_link` as they stand — contract §4.1's four-field
  normalization is untouched and there is no normalized status model). No new module on either side
  of the bridge: the read is a section of `commands/entity.rs`, the store is
  `crates/knobas-core/src/mini_board.rs`, and `knobas-core` is not in the frozen list. Nothing under
  `crates/knobas-source/src/**` — the Source SPI, its DTOs and the contract battery are untouched, so
  ADR-0006 stands. No existing command, DTO field or event name changes; no new event
  (`contexts:changed` and `activity:new` are what already say a room moved, and a read command has
  nothing of its own to announce). `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched — the command's only failure is a query
  failure, which the existing `From<CoreError> for IpcError` already maps.

  **Two consumers, one grant, and why the DTO carries `sources`.** `columns` is the tile's board
  (#178); `sources` is the ticket detail's status select (#179), which needs the statuses observed
  for a ticket's **source corpus** rather than the room's columns — a room with nothing finished
  still has to be able to offer *Done*, and a second command for that would be a second grant. It is
  scoped to the sources that actually put a card on this board, and it never carries the terminal
  group: "no status" is somewhere a ticket can be, not somewhere it can be moved to.

  **Four shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's "a later tidy-up that flattens it is a bug, not a simplification":

  - `MiniBoardColumn.status` is `Option<String>` and the terminal group is its `None`. The words on
    screen ("No status") are the shell's. A sentinel string would be a status the source never said,
    indistinguishable from a source that really spells one that way, and it would land in the
    select's offer.
  - **There is no `_inner` behind the command.** Its body is `lifecycle.pool()?` and one call into
    `knobas_core::mini_board::read`, so it takes `context_members`' shape rather than
    `create_context_inner`'s, which wraps a body that does real work. The read's battery is
    `crates/knobas-core/tests/mini_board.rs`, beside `contexts.rs`: the store owns the behaviour, so
    the store's tests own the proof, and `knobas-app` keeps only the DTO mirror test.
  - **A column carries no count.** The header's count is `cards.len()`; a count beside the list it
    counts is a second copy of one fact, and only one of the two can be right.
  - **The grouping and the column order are the command's, not the client's.** Both consumers get
    the same board, and the order — To Do, In Progress, In Review, Done first where the corpus shows
    them (matched case-insensitively, displayed in the source's own spelling), every other observed
    status after them alphabetically, the terminal group last — is pinned by
    `crates/knobas-core/tests/mini_board.rs` and `knobas_core::mini_board`'s own unit tests.

  **The two payload reads are ADR-0007's, with their failure directions pinned.** Status and
  priority are not among §4.1's four normalized fields, so both come out of the payload: one
  `macro_rules!` each (`status_read!`, `priority_read!` in `knobas_core::mini_board`), so a second
  source's spelling is one more `coalesce` in one place. Both are read at a *type-checked* path — a
  path landing on an object or an array misses rather than being stringified into a column headed
  `{"id":3}`. Direction one: a ticket with no readable status lands in the visible terminal group,
  never dropped and never guessed into a column
  (`a_ticket_with_no_recognizable_status_lands_in_the_terminal_group`). Direction two: a card with no
  readable priority omits it (`a_ticket_with_no_recognizable_priority_carries_none`). Both arms of
  both `coalesce`s are witnessed — Jira's `fields.*.name` and the mock source's flat spelling, the
  latter being what the demo profile's board is drawn from. Both expire
  into the descriptor-declared path ADR-0007 books for M3.

  **The start-work transitions widening is recorded here**, as #179's brief says it must be: the
  status select reuses `WriteOp::Transition` — the existing variant, no SPI growth — so the two
  transitions the start-work flow was ratified with become any status a source's corpus shows. It is
  optimistic by design: knobas has no read of reachable transitions (~~M3's descriptor growth, per
  ADR-0007~~), so the adapter resolves the target at write time and refuses by name, and the refusal
  surfaces through the existing pending/held-write UI. **#179 therefore adds no frozen-surface change
  of its own**, and neither does #178. *The parenthetical is struck 2026-09-04: #277's entry below is
  that growth and it brought no such read. Spec #272 books the read for a later milestone instead —
  "No transitions read: the status select stays optimistic and refuses by name as today; a per-ticket
  transitions read is booked for a later milestone with its own entry" — and `docs/roadmap.md`'s v1.5
  fast follows now carries the booking. What the sentence records, an optimistic select that resolves
  at write time, is unchanged.*

  Ratified by the orchestrator as issue #177 itself, whose acceptance criteria specify the command,
  its tests and this entry.

