- **`crates/knobas-source/src/**` (a `Source` method, a module and the ticket-system minimum),
  `crates/knobas-wasm-host/wit/adapter.wit` (`knobas:adapter@1.2.0`), one new command
  `translate_ticket_view` and its IPC schema — issue #1208 (2026-10-19): `translate_query`, the
  ticket system's optional read, moves *Make an editable copy*'s JQL reading out of the shell and
  into the Jira adapter.** No migration, no `commands/` or `ipc/` module, no `WriteOp`, no
  `KindPaths` field, no battery clause, no change to `crates/knobas-app/src/error.rs`. Parent spec
  #1192 (ADR-0029: "`translate_query` is an optional read, and JQL is Jira's answer to it").
  Ratified by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is
  in `docs/archive/2026-10-unattended-rulings.md`.

  **The read.** `Source::translate_query(&self, view: &serde_json::Value) ->
  Result<Option<query::Translation>, SourceError>`, defaulting to `Ok(None)` (*this adapter
  translates no query*), as `pipeline_settings` and the run reads default. `view` is the saved
  view's payload as the adapter emitted it, so the source's query language never leaves the
  adapter; the read is pure and asks the source nothing. `knobas_source::query` holds the answer,
  plain serde data: `Translation { conditions, kept: u32, left_out }`, `Condition { key, negated,
  values }`, `LeftOut { clause, reason }`, and `Key`, the search grammar's keys in snake case
  (`project`, `status`, `issuetype`, `priority`, `assignee`, `under`, `label`, `sprint`, `has`,
  `updated`, `assigned`). The ticket-system minimum's `optional_reads` gains
  `role::TRANSLATE_QUERY` (`"translate_query"`); Jira's claim names it.

  **Jira's answer.** `knobas-source-jira`'s `translate` is the shell's `jql.ts` moved over: the
  same parts, conditions, kept count, and left-out clauses with their reasons, byte for byte. It
  reads the payload's `jql`, `type` (a Scrum board adds `sprint in openSprints()`) and
  `sub_query`. `jql.test.ts`'s cases are its unit tests, each expected search text unchanged. The
  shell holds no JQL parser.

  **The command.** `commands::search::translate_ticket_view(id) -> Option<Translation>`: the
  saved view `id`'s `jira_ref` and that item's live payload, handed to its source's adapter, which
  is built as `view`'s live read builds it. `not_found` for no view or an item not live, `invalid`
  for a view somebody made, the source's failure as `sources::to_ipc` maps it. Mirrored in
  `app/src/lib/ipc/search.ts` as `translateTicketView` and `QueryTranslation`.

  **What the shell does with it.** A saved view offers *Make an editable copy*, *Save as a view*
  and View settings' *A copy keeps …* only when its source's ticket-system claim names
  `translate_query` (`sourceRoles.answers`); otherwise the view still shows, with none of them.
  The copy's conditions are written as search text by the shell's own `conditionsText`.

  **`knobas:adapter@1.2.0`.** The minor after #1205's 1.1.0 (PR #1250, `downloads`), under the
  1198 entry's rule: a new `knobas:adapter` interface, `query`, with one function,
  `translate-query: func(instance: instance, saved-view: json) -> result<option<json>,
  source-error>`, exported by the guest world `adapter` beside `source` and `downloads`. The
  host's statically bound world `hosted` is unchanged from 1.1.0; it finds `query` as it finds
  `downloads`, through one helper, `optional_export`, which looks the interface up at the host's
  version (wasmtime's semver-compatible export lookup finds any 1.x's) and refuses an interface
  that lacks its function. A component without `query`, any 1.0.0 or 1.1.0 component, links as
  before and answers the trait's default, `None`; the call runs within the adapter's deadline
  (#1204). A free function taking the `instance` record, not a method on the `adapter` resource,
  because a resource method is bound with its resource, and binding it would make every earlier
  component fail to link. `INTERFACE_VERSION` is 1.2.0, so a component built against 1.2.x
  loads and one against 1.3.0 is refused. The fixture translates a payload's `survey_query` as
  one `label` condition; its `v1_0` build exports no `query`.

  Pinned by the Jira adapter's `translate` tests, `knobas-source`'s `query` round trip,
  `knobas-wasm-host`'s `optional_export` unit test and
  `a_saved_view_s_query_crosses_to_the_component_and_its_translation_back`,
  `jira_views_ipc.rs`'s
  `make_an_editable_copy_asks_the_saved_views_source_to_translate_its_query`, and the Tickets
  page's vitest (*a saved view from a source without translate_query offers no editable copy*).
