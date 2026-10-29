- **The IPC command schema — issue #879 (2026-10-17): a Jira view counts the tickets on it in
  projects the source does not sync.** Spec #808, *Jira views*; requirements R-3.22 (header),
  R-3.23 (unsynced tickets), R-3.24 (quick filters, left out); milestone TF5. No migration, no
  new command, no event, no `WriteOp`, no grammar key, no declaration field, no new module in
  either barrel or in `commands/`, no change to the Jira adapter's payload.

  **The IPC.** `ticket_views`' `JiraSource.unsynced: Vec<Unsynced { project: String, count: u64 }>`
  (`crates/knobas-app/src/ticket_views.rs`), which #878 added for *Boards and filters* as every
  member that is not a live ticket, is narrowed to R-3.23's meaning, one field for both readers
  (the Tickets warn line and *Boards and filters*' "and 9 in WEB, not synced"; the shape is
  unchanged): the view's `sync.jira_view_member` keys whose ticket the mirror does not hold live,
  grouped by the project in the key (an id of another shape is a miss), leaving out every project the source's config lists under `projects`
  (a sync still to come), most tickets first, then by project. Only a source that names its
  projects leaves any out: an empty `projects` syncs everything the credential sees and a
  `jql_filter` source has none (the two are exclusive), so a `projects` that is missing, empty or
  not an array counts nothing (ADR-0007's failure direction: a warn line lost, never a *Sync WEB
  too* that narrows an all-projects source to one or that the config refuses beside a JQL).
  Mirrored in `app/src/lib/ipc/search.ts` (`JiraSource.unsynced`, `UnsyncedTickets`) and in the fake IPC (`fake/ticket-views.ts`:
  prototype-6's *Platform on-call* holds 9 in WEB).

  **Quick filters are left out.** The ticket asked to check `rest/agile/1.0/board/{id}/quickfilter`
  against testenv's Data Center and leave quick filters out if it is missing: Jira Software DC
  10.3.24 answers it `404 HTTP 404 Not Found` on both seeded boards, whose configuration answers
  200. Pinned by `crates/knobas-source-jira/tests/live_jira_seeded.rs`'
  `the_seeded_data_center_has_no_quick_filter_endpoint`, which fails the day a Jira image answers
  it.

  Pinned by `crates/knobas-app/tests/it/jira_views_ipc.rs`'
  `a_jira_view_counts_its_tickets_in_projects_the_source_does_not_sync` (two unsynced projects,
  most first; a missing key of a configured project left out; nothing for a config with no,
  or empty, `projects` or with a `jql_filter`). It replaces #878's
  `a_jira_view_names_the_tickets_on_it_that_knobas_does_not_sync`, whose seeded source names no
  projects and so now has none unsynced.
