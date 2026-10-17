- **The IPC command schema (`entity_thread`'s answer), `knobas_core::payload` and
  `crates/knobas-source/src/**` — issue #929 (2026-10-17): the thread's changes carry status
  moves made in Jira and the linked pull requests' openings and merges (R-5.2).** Spec #808,
  *Include changes*. No migration, no grammar key, no `WriteOp`, no new command, no `commands/` or
  `ipc/` module. Amends #819's `ThreadChange`, whose entry stands as written.

  **The wire shape.** `ThreadChange` becomes `{ at: DateTime<Utc>, kind: ChangeKind, status:
  Option<String>, from: Option<String>, by: Option<String>, mine: bool, number: Option<String>,
  repo: Option<String> }`, with `ChangeKind` one of `"status"`, `"opened"`, `"merged"`
  (`knobas_app::conversation::ChangeKind`, lowercase on the wire). `status` was a `String`; it is
  `null` on a pull-request line. `Thread` and `ThreadComment` are unchanged. `changes` stay
  oldest first and never count as comments (R-5.1). Mirrored in `app/src/lib/ipc/entity.ts`
  (`ChangeKind`, `ThreadChange`) and the fake IPC (`fake/entity.ts`); held by
  `conversation::tests::the_thread_and_its_mirror_agree`.

  **Status moves made in Jira.** The Jira adapter's `/search` sends `expand=changelog` (a
  parameter the pinned WADL declares on `api/2/search`; `renderedFields` is still never asked),
  so the stored payload carries `changelog.histories`. `conversation::HISTORIES` is the second
  named payload read (ADR-0007) beside `COMMENTS`: `changelog.histories` when it is a JSON array,
  else none. Each history item whose `field` is `status` is a `status` line: `at` the entry's
  `created`, `status` its `toString`, `from` its `fromString`, `by` the author's `displayName`
  else account, `mine` the account in the identity. An item with no time or no `toString` is no
  line; **failure direction: a drifted shape is no line**, pinned by
  `a_changelog_of_another_shape_reads_as_no_moves`. Witnessed by `conversation_ipc.rs`'s
  `a_status_change_made_in_jira_is_a_change_with_who_from_and_to` and against the real product
  by `live_jira_seeded.rs`'s `each_issue_carries_its_status_history`.

  **Moves knobas sent** (#819's `MOVES`) stay lines, `mine`, with no `from` or `by`, except one
  the changelog already records: a changelog move to the same status no earlier than two minutes
  before the write was queued (`conversation::SKEW`, the server's clock against this machine's).
  A record with no changelog draws them all, as before.

  **Pull-request events.** `KindPaths` gains `opened: Vec<PayloadPath>` and `merged_at:
  Vec<PayloadPath>`, both `#[serde(default)]`, in `field::ALL` as `field::OPENED` and
  `field::MERGED_AT`; the contract battery weighs both as `Wanted::Instant`. Mirrored in
  `app/src/lib/ipc/sources.ts` and held by `sources_mirror.rs`. Gitea declares a pull request's
  `opened` at `created_at` and `merged_at` at `merged_at` (Gitea writes it on every pull request,
  `null` until merged; the test fake now does too). `conversation::PULLS` reads every live `pr`
  confirm-linked to the ticket in either direction, through `declared_string!` for both times
  (`knobas_core::payload::parse_instant`; a miss is no line), the number as the `#N` its key
  ends in (the signals' rule) and `repo` as the shortest name `declared_parent_names!` gives its
  parent. Witnessed by `a_linked_pull_requests_opening_and_merge_are_lines`.
