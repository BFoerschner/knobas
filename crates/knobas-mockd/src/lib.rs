//! `knobas-mockd` — faithful, stateful HTTP mocks of the source APIs knobas
//! cannot self-host, backed by the Tidewater Freight fixture.
//!
//! Two ways in, one behaviour:
//!
//! * **In-process** (`spawn_mock_jira()`, `spawn_mock_teamcity()`,
//!   `spawn_all()`): binds `127.0.0.1:0`, returns a guard that shuts the server
//!   down on `Drop`. This is what adapter integration tests use — fast,
//!   deterministic, no Docker, green in CI.
//! * **As a container** (`src/bin/mockd.rs`): the same routers on the fixed
//!   ports of the interfaces doc §5, for the `testenv/` compose environment.
//!
//! Every request is checked against a path/verb/query allowlist **generated at
//! build time from `testenv/specs/jira-dc-rest.wadl`**. A request the contract
//! does not define gets a real error shape *and* a [`Violation`], so an adapter
//! that invents an endpoint fails its own test suite instead of passing against
//! a lenient mock.
//!
//! ## Documented deviations from the real APIs
//!
//! Ruled acceptable by the orchestrator (interfaces doc §8, P11). Do not
//! "fix" these — each one is here because the alternative buys nothing:
//!
//! 1. **TeamCity without `Accept: application/json` gets 406 + `X-Mockd-Hint`**,
//!    where real TeamCity would serve XML. Writing an XML serializer to reward a
//!    bug is waste; the adapter fails either way, and this way it fails legibly.
//!    `Accept: */*` counts as "not JSON" — reqwest sends that by default, so
//!    this is precisely the guard that forces the header to be explicit.
//! 2. **Jira request validation is a WADL-derived path/verb/query allowlist**,
//!    not schema validation of request bodies: Atlassian publishes no
//!    machine-readable DC request schema (`testenv/specs/README.md`).
//! 3. **Unknown query parameters are refused with 400.** Real Jira ignores
//!    them. mockd is stricter on purpose: a parameter that does nothing is a
//!    bug the adapter author must see.
//! 4. **Every Jira endpoint requires an `Authorization` header**, including
//!    `/rest/api/2/serverInfo`, which a real anonymous-browsing instance would
//!    serve without one. Same reason: credentials must be on every request.

pub mod validate;

pub use validate::{Violation, ViolationKind, ViolationLog};
