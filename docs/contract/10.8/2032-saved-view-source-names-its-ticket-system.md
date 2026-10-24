- **IPC schema, issue #2032 (2026-10-24): `SavedViewSource` gains `system: String`, the ticket
  system a saved view comes from.** No new command, event, migration, type or module in
  `commands/` or `ipc/`; the generated mirror (`app/src/lib/ipc/generated/search.ts`) is
  regenerated for the field. Ruled by the implementer on Opus (`## #2032` in
  `docs/decisions/2026-10-unattended-rulings.md`); parent spec #1192, story 7.

  1. **`SavedViewSource.system`**, answered by `ticket_views` on every row whose
     `saved_view_source` is not `null`: the name the view's group is made from, by
     `knobas_sync::saved_views::declared` over the descriptor of the source's adapter
     (`knobas.source_config.kind`) in the registry `ticket_views::list` already takes, which is
     the first saved-view kind's declared `system_name`, else the descriptor's `name`. Jira's is
     `Jira` whatever the source is called. Where the adapter is not compiled in, or declares no
     saved-view kind, it is the source's display name (its id when that is blank).
     `knobas_sync::saved_views::SavedViewKinds` gains `pub fn source_name(&self) -> &str` for it.
  2. **No other change.** The group (`<system> boards` / `<system> filters`), the projection and
     every other field read as before.

  Answered by the fake IPC (`fake/ticket-views.ts`): `Jira` on each Jira view, and `Kestrel` on
  the one saved view of the fixture-only second ticket system (`fake/sources.ts`). Called by the
  Tickets screen (`TicketsPage.svelte`, `SavedViewSourceLine.svelte`, `ViewEditor.svelte`), which
  names the view's source with it where prototype-6 says Jira. Witnessed by
  `tests/it/jira_views_ipc.rs::a_saved_view_names_its_ticket_system_not_its_source`.
