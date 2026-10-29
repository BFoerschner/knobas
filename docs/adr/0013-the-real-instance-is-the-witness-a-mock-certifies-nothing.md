---
status: accepted
---

# The real instance is the witness; a mock certifies nothing

M0 to M2 were tested mock-first against `knobas-mockd`, and features that passed against it kept failing against the real system, because the mock encoded the implementer's assumptions and a test that checks an assumption against itself cannot fail. Decided 2026-09-02 (Björn, M3 grilling): **every adapter and every write path is tested against the real system, and no mock is a witness for any acceptance or exit criterion.**

- Each system gets a seed script from the Tidewater fixtures, a `just <system>-live` recipe (stand up, seed, suite, tear down) and a live suite. The PR body shows its run, and the merge-manager re-runs it before squashing. The working model says which PRs owe one: a PR touching an adapter crate, a `WriteOp`, the descriptor or the sync engine.
- `just check` stays fast. The live suites run beside it, never inside it.
- `knobas-mockd` is deprecated and frozen: no Confluence half, port 8211 unreserved, nothing new. Its tests stay only until the live suites assert the same things, then go with it. `knobas-source-mock`, the trait-level fake that lets UI and engine tests run with no server, is a different thing and stays.
- The Atlassian pair runs on 10-user, 3-hour timebomb keys, so the live shape is disposable: the seed is fast, idempotent against a partly seeded instance, and followed by `down -v`. `testenv/fetch-timebomb-keys.sh` fetches the keys and `seed-atlassian.sh` calls it when a key is unset. Jira project creation over REST fixes a template key, so the seeded workflow is Jira's default; seeded worklogs and comments belong to the admin account.
- The one fake exception (Björn, 2026-09-07). A live instance can be made to answer slowly, drop the connection, refuse a credential or move a target. It cannot be made to do what it will never do, such as a socket.io session that stays alive and never acknowledges (#486). For that class alone a fake in the crate's test module is the witness for that one criterion, its doc comment cites this line, and the adapter's certificate stays its live recipe. Awkward to reproduce is not cannot produce: if any seed, flag or clock drives the real instance into the fault, it is witnessed live.

## Rejected

- Keep mock-first and add a Confluence half to mockd. Atlassian publishes no machine-readable Confluence DC spec, so that mock would have been shaped by reading docs.
- Retire mockd in one ticket now. Deleting its tests before the live suites cover the same assertions removes regression cover.
- Run the live suites inside `just check`. A Jira boot in front of a CSS fix is a gate nobody runs.
