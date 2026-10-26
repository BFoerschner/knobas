- **What the Jira adapter's `/search` sends — issue #2165 (2026-10-26): no `expand`, so no
  `changelog`, for a sync run's walk and for a refresh alike.** Parent spec #2163. Retires the
  adapter half of #929's entry (`929-thread-changes-jira-changelog-and-pull-requests.md`, *Status
  moves made in Jira*), which stands as the record of what was; the conversation half is #2164's.
  No migration, no IPC, no change under `crates/knobas-source/src/**` or `crates/knobas-http/**`.

  **The query.** `knobas-source-jira`'s `api::search_query` sends `jql`, `startAt`, `maxResults`
  and `fields`, and nothing else; the field list is unchanged, so a ticket's current status still
  arrives as `fields.status`. A refresh reads one issue through the same query
  (`crate::refresh`), so it stops asking too. Pinned by `api::tests::the_search_sends_no_expand`.

  **Why.** Data Center answers `expand=changelog` with each issue's whole history, a description
  edit's old and new text whole, and has no paged or status-only changelog endpoint
  (`/issue/{key}/changelog` answers 404). Measured in #2163 on the seeded Jira: one 10-issue page
  with 1000 edits per issue was 54 MB in 2.5–3.1 s, and 237 KB in under 50 ms without it; past the
  transport's 32 MiB response cap the run failed. Its one reader was the conversation's *Include
  changes*, which #2163 removes.

  **What a stored record holds.** A Jira ticket item's `payload` written from now on has no
  `changelog` key; records already stored keep theirs until #2166's migration strips them. The
  conversation reads a record with no changelog as no Jira moves, so nothing waits on that
  migration. The record shrinks, so its fingerprint changes and the first incremental run after
  the upgrade re-delivers the tickets in its overlap window once; no cursor version bump.

  Witnessed against the real product by `live_jira_seeded.rs`'s
  `a_long_history_syncs_without_its_changelog` (600 description edits of 30,000 characters on a
  sub-task it files and deletes: the full sync is not refused, the stored record and a refresh's
  answer carry no `changelog`), which replaces `a_long_history_comes_back_whole_from_search`, and
  `each_issue_carries_its_status_and_no_history`, which replaces
  `each_issue_carries_its_status_history`.
