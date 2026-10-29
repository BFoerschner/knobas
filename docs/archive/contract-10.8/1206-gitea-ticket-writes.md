- **The registry's write table: Gitea declares `transition` and `create_ticket` — issue #1206
  (2026-10-19).** No `WriteOp` variant, no change to `crates/knobas-source/src/**`, no role
  minimum, no migration, no command, no DTO, no `KindPaths` field. Parent spec #1192
  (ADR-0029: the ticket system's optional writes). Recorded here because ADR-0006 makes every
  growth of the write table a ratified exception, and `sources_registry.rs`'s
  `the_registry_declares_exactly_the_ratified_write_set` is the table.

  **What Gitea now is.** A `ticket-system` source beside its `code-host` claim: a `ticket` kind
  (`KIND_TICKET`, label *Issue*, monogram `IS`, `full_sync_exhaustive: false`), one per issue,
  keyed `owner/repo/issues/<n>` (`keys::issue_key`), read from `GET
  /repos/{o}/{r}/issues?type=issues&state=all` with `since=` the per-repository watermark. Its
  declared paths: status `state`, assignee `assignee.login`, labels `labels[].name`, project
  `repository.full_name` (name `repository.name`), comment count `comments`, and three the
  adapter writes into a `knobas` object beside the record: `knobas.priority` (the value of a
  scoped label `Priority/<value>`, else `null`), `knobas.issue_type` and `knobas.parent`
  (always `null`), with `knobas.key` the name a ticket answers to as a parent.

  **The reads.** `reachable_transitions` answers an issue's other state, read live from `GET
  /repos/{o}/{r}/issues/{n}`, and refuses any id that is not an issue's. A source view of an
  issue is `None`: its detail is the ticket's.

  **The writes.** `transition` is `PATCH /repos/{o}/{r}/issues/{n} {"state": "open" | "closed"}`;
  any other status is refused before a request. `create_ticket` is `POST /repos/{o}/{r}/issues
  {"title", "body"}` on a repository id, sent once (#1122), its receipt the new number; the
  ticket type is not sent, Gitea has none. `comment` now takes an issue as well as a pull request:
  both discussions are on `/issues/{n}/comments`. `log_work` stays Jira's alone. Certified
  against the testenv Gitea and Forgejo by `live_gitea.rs`'s
  `a_ticket_is_filed_moved_and_commented_on_through_its_writes` (ADR-0013).
