# TeamCity build mutators in the mockd admin API

mockd's `/__mock/*` control plane does not expose the TeamCity build mutators
(`finish_build`, `queue_build`, `cancel_build`, `fail_build_to_start`,
`describe_build_type`). Compose mode deliberately does not drive TeamCity
build state.

## Why this is out of scope

The two test modes divide the work. In-process tests hold an `Arc<MockState>`
and drive build lifecycles directly, which is where the running-to-finished,
canceled and failed-to-start shapes (the issue-#105 class) are exercised.
Compose-mode end-to-end runs exist to assert contract fidelity against a real
container boundary, and `GET /__mock/violations` is their whole assertion
vocabulary. Nothing under `testenv/` drives TC build state, and adding the
routes would create a standing implication that compose-mode e2e ought to,
doubling coverage that in-process tests already own.

The admin router's header in `crates/knobas-mockd/src/admin.rs` records actual
coverage (the Jira-side mutators have twins, the TC-side ones do not) and
points at the closed issue.

If `testenv/` ever grows a TeamCity end-to-end run that needs to move a build
through its lifecycle from outside the process, that is the reconsideration
trigger: delete this file and add `/__mock/teamcity/*` routes then. The
mutators are all present on `MockState` (state.rs), so the twins are
mechanical when wanted.

## Prior requests

- #219: "mockd admin API: TeamCity build mutators have no /__mock/* twins, despite admin.rs's claim"
