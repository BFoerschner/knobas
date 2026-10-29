- **The IPC schema, `mini_board` and the search grammar — issues #813 and #815 (2026-10-16):
  two fields on `MiniBoardCard`, and `has:`.** The Tickets view's lanes (R-3.5) need a card's
  labels, and signals (R-3.8, R-3.14) are a read the cards draw and `has:` filters by. No new
  command, no migration, no declaration field, no `WriteOp`, no `commands/` or `ipc/` module.

  **The card.** `MiniBoardCard` gains two fields, read only for the cards a column keeps:
  - `labels: Vec<String>`, the declared `labels` list path (#811's field) in the record's order,
    empty on a miss. The *Label* lanes read the first;
  - `signals: Vec<knobas_core::signals::Signal>`, each `{ kind, text, tone }`: `kind` one of
    `failing-build`, `blocker`, `open-pr`, `review-for-me` (`SignalKind`), `text` the words drawn
    (*build failed*, *blocked by OPS-77*, *PR #142 open*, *review asked of you*), `tone` the
    shell's `Tone`. In the order a card draws them; empty for none.
  Mirrored in `app/src/lib/ipc/entity.ts` (`Signal`, `SignalKind`), held by `entity_mirror.rs`'s
  `the_mini_board_shapes_match_their_typescript_mirror`. The fake IPC answers both over its
  fixture.

  **The reader** (`knobas_core::signals`, new module; `ticket_signals!` and `linked_ends!`,
  exported macros). A set-based table expression over a relation of ticket ids, reading
  `knobas.confirmed_link` and the declared `state` and `reviewers` paths only (ADR-0007). The
  ticket's reach is ADR-0008's walk with the ticket as the seed: its confirmed links, and one hop
  further, never through a context.
  - `failing-build` (`err`): of the builds in reach, the newest of each configuration (its
    `sync.item_parent`, else itself) has a declared state in the `err` tone;
  - `blocker` (`err`), one per blocker: the ticket is the `to` end of a confirmed `blocks` link;
    named by the blocker's key when it is a ticket, else its title;
  - `open-pr` (`info`), one per pull request: a `pr` the ticket links directly whose declared
    state is in the `info` tone; its number is the `#N` its key ends in, *PR open* without one;
  - `review-for-me` (`warn`): one of those declares a requested reviewer equal to a configured
    username, ignoring case.
  The board reads it in a second statement over the kept ids
  (`knobas_core::mini_board::planned_signals`, planned by `read_plans.rs` with confirmed links).

  **The grammar** (`knobas_search::query`). `FieldKey::Has`, spelled `has:`, the ninth ticket key
  spec #808 lists (#811's entry has the other eight). Values: the four kinds, ignoring case; any
  other is an unknown token. `-has:` negates. `FieldFilter` gains `me: Vec<String>`, the
  identity's usernames lowercased, filled by `merge` for `has:` only. The corpus predicate is
  `i.entity_id in (<the reader over every end of a confirmed link> where kind = any(values))`,
  with its binds in place (not through `fd`) so the walk is one hashed subplan a statement.
  `filter_suggestions` offers the four values. Held to the cards by
  `knobas-app/tests/it/signals_ipc.rs`: each signal seeded, the card, `has:` through
  `mini_board` and through `search` agree, and the negation is the complement.
