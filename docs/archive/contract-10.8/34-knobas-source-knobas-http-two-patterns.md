- **`crates/knobas-source/src/**`, `crates/knobas-http/**` and two patterns in
  `crates/knobas-app/src/error.rs`, issue #34 (2026-08-28):** ADR-0004
  (`docs/adr/0004-structured-source-error-status.md`, accepted 2026-08-27) is the decision this
  entry records, and it specifies the package: the status on `SourceError`, 401 and 403 made
  distinct, a body→message hook, and all three adapters migrated in the same change.

  **The SPI.** `SourceError::Unauthorized` becomes `Unauthorized { status: Option<u16> }` and
  `Protocol(String)` becomes `Protocol { status: Option<u16>, message: String }`;
  `SourceError::status()` is the one way to ask, and `SourceError::unauthorized()` /
  `SourceError::protocol(..)` construct the faults an adapter raises without asking anyone --
  a missing keychain entry, a base URL that is not one -- which carry no status. That absence is
  load-bearing, not incidental: a sink failure or a DNS blip reading as a 404 would be swallowed
  as a skipped repository. **The four fault classes are unchanged and so is every mapping
  downstream of them**: 401 *and* 403 are still `Unauthorized`, which is what puts *Re-enter
  password* on screen and what a Jira DC CAPTCHA lockout needs, so `IpcErrorCode`, `AuthState`
  and `SyncOutcome` map exactly as before. Björn ruled on that reading of ADR-0004's "401 and 403
  become distinct" on 2026-08-28: distinguishable **by status**, not two fault classes. What
  changes is that an adapter can now tell which refusal it got, which is what ADR-0004 asked for;
  Gitea's `credential_still_good` probe, whose only reason to exist was that it could not, is
  deleted. Both status fields are `#[serde(default)]`, so a peer that sends none still decodes.
  The serde form of the two variants changes (`"Unauthorized"` -> `{"Unauthorized":{"status":401}}`)
  and that reaches nothing: `SourceError` has no TypeScript mirror and does not cross the bridge --
  §2.2 spells `ConnectionReport::error` as `Option<SourceError>`, and the shipped code deliberately
  carries a `String` plus an `IpcErrorCode` instead, saying so in place (`sources/mod.rs`). No IPC
  command, DTO field or event name changes. `contract.rs` takes the same pattern-shape edit and
  nothing more -- no battery clause added, removed or reworded.

  **`crates/knobas-http`.** `classify::status_error` gains a third parameter, the caller's
  `BodyMessage` (`fn(&str) -> Option<String>`), and `HttpConfig` gains
  `body_message: Option<BodyMessage>`, `None` by default. It is consulted inside
  `HttpClient::send`, where the response body still exists and where nothing else can reach it,
  and only for the message-carrying fault -- `Unauthorized` carries no message. What a hook
  returns is excerpted like the raw body, so it cannot widen `BODY_EXCERPT`. Additive on the
  config and covered by its `Default`; a full struct literal must name `body_message`, which all
  three adapters now do. Every other guarantee of the crate is untouched -- one door onto the
  wire, one fault mapping, the same attempts, budget, `Retry-After` and rate limit, and the
  re-export list is unchanged but for `BodyMessage`. `tests/transport.rs` gains one end-to-end
  test -- the only witness that `HttpConfig` -> `HttpClient` -> `send` is wired, since
  `classify`'s unit tests call `status_error` directly and never build a client -- plus the same
  pattern-shape edits.

  **`crates/knobas-app/src/error.rs`.** Pattern shapes only, no arm added or removed:
  `SourceError::Unauthorized` -> `SourceError::Unauthorized { .. }` and
  `SourceError::Protocol(_)` -> `SourceError::Protocol { .. }` in `from_source_error`, forced by
  the variants becoming struct variants. The mapping itself does not move. Outside the frozen
  list and noted for completeness: `sources/crud.rs`'s `auth_state_of`, `sources/mod.rs`'s
  `to_ipc` and `sources/registry.rs` take the same pattern-shape edit and nothing more.

  Ratified by the orchestrator as ADR-0004 and issue #34, which specify the package and its
  acceptance criteria. **No migration of its own** -- `0004` is `0004_backfill_trigger.sql`,
  claimed by #32. No other change inside the frozen paths.

