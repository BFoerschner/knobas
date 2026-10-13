- **`crates/knobas-source/src/**`, issue #286 (2026-09-03):** the Confluence write set — M3.2's
  `WriteOp` growth, the adapter that declares it, and the version hold it is written to be caught
  by. One entry for the package, the arrangement the #278 and #280 entries above record for a
  sub-milestone.

  **The SPI, and it is two variants rather than three.** `Comment` is re-used with the **page** as
  its container: a reply on a page is the same act as a reply on a ticket, and a `CommentOnPage`
  variant would make the enum adapter-aware, which is the coupling ADR-0006 rejects by name. So
  the growth is:

  ```rust
  WriteOp::CreatePage { parent: String, space: String, title: String, body: String },
  WriteOp::UpdatePage { entity: String, base_version: i64, body: String },
  ```

  identifiers `"create_page"` and `"update_page"`, each with its probe in
  `contract::known_write_ops` and its arm in `WriteOp::identifier` — the two no-wildcard matches
  ADR-0006 relies on. `Source::write`'s signature is unchanged; `WriteReceipt` is unchanged, and
  `create_page` is the second op in knobas to answer one (`WriteReceipt::id`), for the reason the
  type documents: a page knobas just made is not in the mirror until the next sync, so the id the
  server answered with is the only handle that exists in between.

  **`CreatePage` spells its target `parent`, and it is the only variant that does not spell it
  `entity`.** This op has two containers and they are not interchangeable — a page is created
  *inside a space* and *under a parent page*, and Confluence's create wants both. Naming the field
  for what it is says which of the two the queue orders and holds against;
  `knobas_sync::write_queue::target_entity` gets an arm of its own for it rather than a wildcard.
  `parent` is a **mirrored** page (`confluence:98400`), unlike a create's container elsewhere in
  the enum; `space` is Confluence's own space key and is not an `EntityRef`, because ADR-0010 makes
  a space a *project* and knobas mirrors no project as an entity.

  **`body` is storage format on both, and `Comment`'s is not.** What an `UpdatePage` carries is
  mostly a page's *untouched* markup — macros, tables, layouts — which no adapter may re-render, so
  the dialect crosses the SPI as it will be stored. `Comment.body` is shared with Jira and Gitea,
  whose comment fields take plain text, so the Confluence adapter renders it
  (`storage::from_text`: `&` `<` `>` escaped, `&` first; blank line a paragraph, newline a break).
  The frontend has the same rule in `page-sections.ts`'s `toStorage` for the *page* body, and the
  two are documented as a pair.

  **Declared by the Confluence adapter alone**, `write_ops: ["comment", "update_page",
  "create_page"]` with `Capability::Write`; every other adapter refuses both new ops by name, which
  is battery clause 5 and which `knobas-app/tests/sources_registry.rs`'s whole-table assertion
  pins across the registry.

  **The hold is the existing mechanism, reading a field that happens to say "version".**
  `update_page` is listed in `knobas_core::write_queue::PROJECTED_OPS` and takes the **whole-record**
  shape it shares with `transition` and `approve`, for the sharpest version of their reason: this
  op replaces a page's body, so anything at all that happened to the page since the reader started
  typing is something their re-assembled body would delete. That shape already carries the mirrored
  `payload` verbatim, and a Confluence page keeps `version.number` in it — so the ordinary
  "snapshot at queue time against snapshot at flush time" comparison **is** the version comparison,
  and the two sides of a held page edit differ in the version number itself. No payload read was
  added to `knobas-core`, and ADR-0007's ban on one in the *write* direction stands untouched.
  `create_page` takes the liveness-only shape: a new page goes beside whatever else sits under its
  parent, so only the parent leaving the mirror holds it.

  **`base_version` is Confluence's half of the same question**, and the backstop rather than the
  mechanism: the adapter sends `base_version + 1` and the server aborts with a 409, which catches
  the window between the last sync and the flush that the mirror cannot see. Both directions
  matter and neither is sufficient alone (ADR-0012 — delivery is still at-least-once, and a
  re-sent `update_page` whose first attempt landed is refused by that same version check rather
  than applied twice).

  **The section rule is a pure function and lives on the frontend**, `app/src/lib/detail/
  page-sections.ts`: a section is a heading of level one to three and everything until the next
  heading of the same or higher level, refused when it holds any `ac:` element or a table — or
  when it sits inside one, which a cell tag in its own slice is the signal for. It
  re-uses #285's parser for a tag's extent and for a body's words, and scans for **offsets** itself
  — a `StorageNode` carries no index, and re-assembling the whole body means copying everything
  outside the edited section byte for byte.

  **What changed on the IPC schema, and it is one thing.** No command, no event, no DTO field: a
  page write is `submit_write` with a payload, which is the command the *Comment* and status
  controls already use. What did grow is the **argument** shape of that one command —
  `knobas_source::WriteOp` gained two variants, so `app/src/lib/ipc/sources.ts`'s `WriteOpPayload`
  union gained the two matching members. That union is the mirror of the SPI enum rather than a
  schema of its own, and its own doc comment says it grows with `WriteOp` per ADR-0006; the
  implementer flags it here rather than deciding it, since §10.8 freezes "the IPC command and
  event schema" and a reader could reasonably count `submit_write`'s argument as part of it.
  `crates/knobas-app/tests/sources_mirror.rs` is what holds the two halves together and it was
  extended with both variants.

  **What did not change.** No migration. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched. `QueuedWrite` keeps its shape and its
  mirror. No settings key. No barrel was appended — no command was added.
  **`knobas-mockd` is untouched** and deliberately: ADR-0013 freezes it and gives Confluence no
  mock half at all, so the witness is `crates/knobas-app/tests/confluence_live.rs` and
  `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` against the seeded container,
  run by `just atlassian-live` (now four suites).

  **The interim payload reads stay interim, and are named here so they are not lost.**
  `storage-format.ts`'s `storageBodyOf` (#285) is joined by `pageVersionOf` and by a comment's
  author and instant. All are ADR-0007 interim reads in the *read* direction: one named statement
  each, gated on `adapter_kind` rather than guessed from the payload's shape, missing to `null` —
  and for `pageVersionOf` the miss is load-bearing, since a record that does not say what version
  it is gets **no edit offered** rather than an edit sent against a guess. #277's `KindPaths` has
  no slot shaped like "a body in this markup dialect" or "the record's own revision number", and
  adding one is a `crates/knobas-source/src/**` change with its own §10.8 conversation. These reads
  expire into it when there is one; this ticket did not open that conversation.

  A fourth interim read joins them on the *shell* side: `write-queue.svelte.ts`'s `readSnapshot`
  reads `payload.version.number` out of a held snapshot so the panel can print which version each
  side of the comparison is. Gated on the **op** (`update_page`) rather than on the adapter kind,
  because a snapshot carries no adapter kind and one adapter declares that op — the same rule under
  a different key, with the same miss to `null`.

  Ratified by the orchestrator as spec #272 and issue #286, whose acceptance criteria specify the
  two ops, the identifiers and battery probes, the pure section rule, the whole-body re-assembly,
  the version hold, the live tests and this entry.

