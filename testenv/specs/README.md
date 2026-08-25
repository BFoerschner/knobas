# Vendored API contract documents

**The primary target for Jira and Confluence is the self-hosted (Data Center) product** — that is what Björn's real instances run. Cloud is a possible later flavor. No real work instances are available during development, so correctness is defined as conformance to the documents below plus, for whole-product fidelity, the **real self-hosted containers** in `../docker-compose.yml` (Atlassian publishes official images — `atlassian/jira-software`, `atlassian/confluence` — usable with free developer/timebomb licenses; they run behind the opt-in `--profile real-atlassian`, mockd stays the daily driver).

| File | Contract for | Format | Status |
|---|---|---|---|
| `jira-dc-rest.wadl` | **Jira Data Center REST v2** (the adapter's primary dialect: `/rest/api/2/search` + classic `startAt` pagination, issue/comment/worklog/transitions — all verified present) | WADL (official; "Jira 9.17.0", the latest Atlassian publishes) | vendored |
| — | **Confluence Data Center REST v1** (`/rest/api/content`, CQL search) | **Atlassian publishes no machine-readable DC spec at all** (no OpenAPI, no reachable WADL) | contract = official HTML REST docs + validation against the real `atlassian/confluence` container |
| `teamcity.json` | TeamCity REST | Swagger 2.0, served only by a running server | **BLOCKED, not vendored** — see below. TeamCity keeps golden-fixture validation only |
| `jira-cloud-v3.json` | Jira **Cloud** v3 (421 paths, `/search/jql`) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v1.json` | Confluence **Cloud** v1 (CQL lives only here) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v2.json` | Confluence **Cloud** v2 content CRUD | OpenAPI 3.0.3 | vendored — cloud flavor, later |

**TeamCity blocker (2026-08-25).** `teamcity.json` could not be vendored on the development machine, and no spec was invented in its place:

1. JetBrains publishes **no static swagger document**. Re-verified 2026-08-25 against four plausible public URLs (`jetbrains.com/help/teamcity/rest/{teamcity-rest-openapi,swagger}.json`, the `teamcity-rest-client` repo, `plugins.jetbrains.com`) — all 404 or 403. `/app/rest/swagger.json` on a running server is the only source.
2. That server is behind a one-time **first-start wizard** (database choice, licence agreement, administrator account) whose form endpoints are not REST API and change between versions. It is a human action, so `fetch.sh --teamcity` stops there and says so rather than guessing.
3. The host had **13 GiB free on a 98 %-full volume**; the image plus its data directory needs roughly 10 GB.

Consequence, per the plan's own fallback: TeamCity is validated against **golden fixtures only** — which is exactly what ruling P11(b) already blesses, and strictly less loss than a hand-written spec that lies. `crates/knobas-mockd/tests/teamcity_contract.rs` carries the swagger half already written and **self-arming**: it detects `teamcity.json`, and the moment the file is vendored the schema assertions start running with no code change. A companion test fails if this row ever stops recording the blocker, so the skip cannot be quietly forgotten.

To discharge it: `cd testenv/specs && ./fetch.sh --teamcity` on a machine with the disk headroom and a human at the browser, then `shasum -a 256 *.json *.wadl > SHA256SUMS`, review, and flip this row to `vendored — <version>, <date>`.

Rules:
- `SHA256SUMS` pins the exact documents. mockd mocks the **DC shapes** (Jira v2, Confluence v1) and validates its responses — and, via request-validation middleware, the adapters' requests — against the strongest available contract per API.
- **Version alignment:** the Jira WADL is also published per-version (`docs.atlassian.com/software/jira/docs/api/REST/<version>/jira-rest-plugin.wadl`); once Björn's instance versions are known, re-pin the WADL and the container tags to exactly those versions.
- Refresh deliberately via `fetch.sh`, review the diff, then update `SHA256SUMS` — never let a contract change slip in silently.
- Flowrun is an internal system with no public spec: its stub in `knobas-mockd` *defines* the assumed contract, to be validated against the real instance when one becomes available.
- The Jira Cloud `/search` removal (HTTP 410, Oct 2025) is **Cloud-only** — DC keeps `/rest/api/2/search`. Adapters must not confuse the two dialects; the source config carries a `flavor: datacenter | cloud` field.
