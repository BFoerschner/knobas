- **Interfaces §4.1's Jira rate-limit default — issue #2167 (2026-10-26): 20 req/s with a burst of
  40 (was 5 and 10).** Parent spec #2163. No migration, no IPC, no change under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`; Gitea's and TeamCity's defaults are
  unchanged. §4.1's *Rate-limit defaults* line now says so.

  **The default.** `JiraConfig::default()` and the descriptor's `config_schema` defaults for
  `rate_per_sec` and `rate_burst` are 20 and 40. Both stay per-source settings, so a source on a
  server that asks for gentler clients lowers them in its form; a source that saved the old values
  keeps them. Pinned by `config::tests::an_empty_object_is_the_datacenter_default` and
  `descriptor::tests::the_form_offers_twenty_a_second_with_a_burst_of_forty`. Per the issue
  tracker's note on limiter refills (#1737, `docs/agents/issue-tracker.md`), a count of requests in a time window at this default counts
  the burst within one refill interval (50 ms).

  **Why, and what rides on it.** Data Center inlines at most 20 worklogs per issue in `/search`,
  and the run reads the rest through `GET /issue/{key}/worklog`. A run now completes a page's
  truncated or absent worklog containers concurrently, at most eight in flight
  (`sync::WORKLOG_READS_IN_FLIGHT`), after the page is read and before any of its items reach the
  sink; items reach the sink in walk order, and the first failure in walk order fails the run
  with nothing after it pushed. At 5 req/s those reads were spaced 200 ms apart: 200 such issues
  took a 1500-issue run from 8.1 s to 45.2 s (#2163). Witnessed by `sync::tests`'
  `a_pages_truncated_worklogs_are_completed_eight_at_a_time_in_walk_order` and
  `a_failing_worklog_completion_fails_the_run_and_stops_the_page` over the fake `JiraApi`, and
  against the real product by `live_jira_seeded.rs`'s
  `an_issue_with_more_than_twenty_worklogs_comes_back_with_all_of_them`.
