- **`crates/knobas-source/src/**` (doc only) — issue #2054 (2026-10-25): Gitea answers
  `Source::refused_writes` from the empty-bodied probes its views send.** Spec #1192, story 10.
  No change to the `Source` trait's signature, its DTOs or the contract battery; no IPC command,
  event, DTO field or barrel line; no migration; no generated TypeScript type moves.

  The doc on `Source::refused_writes` and on `knobas_source::grants` said an adapter answers from
  a **read** and never sends a write to find out. They now say it never finds out with a write
  that could change anything: where a system has no read of what its credential may do, an
  adapter may send a write the system refuses before it changes anything. Gitea is that case
  (#2030): a token cannot read its own scopes, and Gitea checks a route's scope category before
  it binds the body, so `POST /repos/{o}/{r}/branches` and `POST /repos/{o}/{r}/issues` with `{}`
  are refused for the scope (403) or for the body (422) and make nothing (contract §4.2's Gitea
  write row). `GiteaSource::refused_writes` sends those two probes, once per repository and
  scope category its asks fall in, through the probe `crates/knobas-source-gitea/src/grants.rs`
  shares with the views. TeamCity still reads, since no TeamCity write is safe as a probe.

  This amends the sentence in #2040's entry that says an adapter never sends a write to find
  out; that entry carries the amendment line.

  Witnessed by `grants::*` (`knobas-source-gitea`'s `it`, over wiremock) and
  `live_gitea.rs`'s `a_read_only_token_is_refused_the_writes_drawn_outside_a_view_saying_why`
  against the real server, on Gitea and on Forgejo, which also counts the repository's branches,
  issues and pull requests around the probes.
