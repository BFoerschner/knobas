# Vendored OpenAPI documents — the mock servers' contract source

Fetched 2026-08-24 from the official vendor sources (see `fetch.sh`). These files are
**the** fidelity reference for `knobas-mockd` and the adapters: no real Jira Cloud /
Confluence / TeamCity instance is available during development, so correctness is
defined as conformance to these documents (plus the real Gitea and Uptime Kuma
containers in `../docker-compose.yml`, which need no mocks).

| File | API | Spec | Paths |
|---|---|---|---|
| `jira-cloud-v3.json` | Jira Cloud platform REST API v3 | OpenAPI 3.0.1 | 421 |
| `confluence-cloud-v1.json` | Confluence Cloud REST API v1 (CQL search lives ONLY here) | OpenAPI 3.0.1 | 89 |
| `confluence-cloud-v2.json` | Confluence Cloud REST API v2 (content CRUD) | OpenAPI 3.0.3 | 151 |
| `teamcity.json` | TeamCity REST — *not yet vendored*; extract from the pinned `jetbrains/teamcity-server` container (see `fetch.sh`) | Swagger 2.0 | — |

Verified present at vendor time: `/rest/api/3/search/jql`, issue get/comment/worklog/transitions
(Jira); `/wiki/rest/api/search` (Confluence v1 CQL); `/pages`, `/pages/{id}` (Confluence v2).

Rules:
- `SHA256SUMS` pins the exact documents; mockd's tests validate its responses (and,
  via request-validation middleware, the adapters' requests) against these files.
- Refresh deliberately via `fetch.sh`, review the diff, then update `SHA256SUMS` —
  never let a spec change slip in silently.
- Flowrun is an internal system with no public spec: its stub in `knobas-mockd`
  *defines* the assumed contract, to be validated against the real instance when
  one becomes available.
