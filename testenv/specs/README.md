# Vendored API contract documents

**The primary target for Jira and Confluence is the self-hosted (Data Center) product** — that is what Björn's real instances run. Cloud is a possible later flavor. No real work instances are available during development, so correctness is defined as conformance to the documents below plus, for whole-product fidelity, the **real self-hosted containers** in `../docker-compose.yml` (Atlassian publishes official images — `atlassian/jira-software`, `atlassian/confluence` — usable with free developer/timebomb licenses; they run behind the opt-in `--profile real-atlassian`, mockd stays the daily driver).

| File | Contract for | Format | Status |
|---|---|---|---|
| `jira-dc-rest.wadl` | **Jira Data Center REST v2** (the adapter's primary dialect: `/rest/api/2/search` + classic `startAt` pagination, issue/comment/worklog/transitions — all verified present) | WADL (official; "Jira 9.17.0", the latest Atlassian publishes) | vendored |
| — | **Confluence Data Center REST v1** (`/rest/api/content`, CQL search) | **Atlassian publishes no machine-readable DC spec at all** (no OpenAPI, no reachable WADL) | contract = official HTML REST docs + validation against the real `atlassian/confluence` container |
| `teamcity.json` | TeamCity REST | Swagger 2.0, served only by a running server | extract from the pinned `jetbrains/teamcity-server` container (see `fetch.sh`) |
| `jira-cloud-v3.json` | Jira **Cloud** v3 (421 paths, `/search/jql`) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v1.json` | Confluence **Cloud** v1 (CQL lives only here) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v2.json` | Confluence **Cloud** v2 content CRUD | OpenAPI 3.0.3 | vendored — cloud flavor, later |

Rules:
- `SHA256SUMS` pins the exact documents. mockd mocks the **DC shapes** (Jira v2, Confluence v1) and validates its responses — and, via request-validation middleware, the adapters' requests — against the strongest available contract per API.
- **Version alignment:** the Jira WADL is also published per-version (`docs.atlassian.com/software/jira/docs/api/REST/<version>/jira-rest-plugin.wadl`); once Björn's instance versions are known, re-pin the WADL and the container tags to exactly those versions.
- Refresh deliberately via `fetch.sh`, review the diff, then update `SHA256SUMS` — never let a contract change slip in silently.
- Flowrun is an internal system with no public spec: its stub in `knobas-mockd` *defines* the assumed contract, to be validated against the real instance when one becomes available.
- The Jira Cloud `/search` removal (HTTP 410, Oct 2025) is **Cloud-only** — DC keeps `/rest/api/2/search`. Adapters must not confuse the two dialects; the source config carries a `flavor: datacenter | cloud` field.
