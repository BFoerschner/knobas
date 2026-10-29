- **`crates/knobas-http/**` — issue #722 (2026-10-13): an optional 403 reader, so a Gitea 403
  that refuses the write is a refusal and not a dead credential.** The deputy's ruling on #722
  (option (a), recorded in `docs/archive/2026-10-unattended-rulings.md`), signed off under
  Björn's 2026-10-13 delegation. The argument is measured (Gitea 1.27.3, 2026-10-13): a bad
  token is a **401** (*invalid username, password or token*); Gitea answers **403** for writes no
  token can fix -- *Git Repository is a mirror.*, *branch protected*, *can not delete default or
  pull request target branch* -- and for one that a token can, a token without the scope
  (*token does not have at least one of required scope(s), …*). That token still answers `GET
  /user`, so an authenticated read cannot tell the two apart; only the 403's body can, and
  `HttpClient::send` consumed it.

  **`crates/knobas-http` — additive:** `classify::ForbiddenReading` (`fn(&str) ->
  Option<String>`, re-exported) and `HttpConfig.forbidden: Option<ForbiddenReading>` (default
  `None`). `classify::response_error(status, body, body_message, forbidden)` is what `send` now
  calls: on a 403 whose body the reader answers `Some(message)` for, the fault is
  `SourceError::Protocol { status: Some(403), message: "HTTP 403 Forbidden: <message>" }`
  (excerpted like every message); otherwise, and for every other status, it is exactly
  `status_error`, which is unchanged. The fault mapping's rule (401 and 403 →
  `Unauthorized`) therefore still holds for every adapter that passes `None`: Jira (whose 403 is
  the CAPTCHA lockout), Confluence, TeamCity and Kuma each pass `None` (one line each), and the
  hcloud estate read takes the default. No `SourceError` change; the write queue's `retryable` already records a
  `Protocol` 403 as a refusal, never retried (ADR-0004).

  **The adapter (Gitea):** `client::forbidden_refusal` reads Gitea's `{"message": …}` envelope
  and answers it for **every** 403 except the scope sentence, which stays `Unauthorized { status:
  Some(403) }` and waits for a new token; a body that is not the envelope (a proxy's page) is not
  read, and stays `Unauthorized`. It applies to reads too: a full sync over a repository answering
  a non-scope 403 now fails as that refusal rather than as *unauthorized*; it still fails, and
  ruling B4's skip on an incremental run reads the status, which is unchanged. #707's pre-reads in
  `write::delete_branch` stay for their better sentences; a protection set between those reads and
  the delete now arrives as Gitea's *branch protected* refusal instead of one tick's wait as
  unauthorized (the #707 entry's last sentence on that race is superseded). A mirror's **commit**
  view now reads the repository record too, and offers *New branch…* disabled *A mirror: its
  branches come from upstream*, as its branch and repository views do.

  **The test environment:** `testenv/docker-compose.yml` sets
  `GITEA__migrations__ALLOW_LOCALNETWORKS`, so the seed (section 4b) mirrors `tidewater/
  ops-runbooks` from the same Gitea as `knobas/ops-runbooks-mirror` (`mirror_interval` 0) with no
  internet; the change recreates the container on the next `up` (on this branch, the first `just
  gitea-live`, inside the Gitea lock). `live-lib.sh`'s `_live_gitea_up` checks the host's `:3000`
  after `up` and heals a dead OrbStack port forward with `stop` + `up` **only while holding the
  Gitea lock**, failing the session if that does not bring it back; without the lock (`just
  live-up gitea`, a TeamCity run's Gitea) it fails naming `just gitea-live` and stops nothing
  (#699; `testenv/live-test.sh`).

  Pinned by: `knobas-http`'s `classify::tests::a_403_the_caller_reads_as_a_refusal_is_a_protocol_fault`;
  `knobas-source-gitea`'s `tests/it/write.rs`
  (`a_403_refusing_the_write_and_a_403_refusing_the_token_are_different_faults`,
  `a_refusal_and_a_blip_arrive_as_different_faults`) and `tests/it/view.rs`
  (`a_mirrors_commit_offers_a_new_branch_disabled`). Witnessed against the real server by `just
  gitea-live`'s `a_403_refusing_the_write_is_a_refusal_and_the_token_stays_good`: a branch on the
  seeded mirror is refused through the adapter with *Git Repository is a mirror.* and
  `test_connection` still passes; Gitea's protected- and default-branch sentences read off the
  wire; a read-only token's write is `Unauthorized` with the 403 while its `GET /user` passes.

