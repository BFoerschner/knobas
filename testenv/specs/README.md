# Vendored API contract documents

**The primary target for Jira and Confluence is the self-hosted (Data Center) product** — that is what Björn's real instances run. Cloud is a possible later flavor. No real work instances are available during development, so correctness is defined as conformance to the documents below plus, for whole-product fidelity, the **real self-hosted containers** in `../docker-compose.yml` (Atlassian publishes official images — `atlassian/jira-software`, `atlassian/confluence` — usable with free developer/timebomb licenses; they run behind the opt-in `--profile real-atlassian`, mockd stays the daily driver).

| File | Contract for | Format | Status |
|---|---|---|---|
| `jira-dc-rest.wadl` | **Jira Data Center REST v2** (the adapter's primary dialect: `/rest/api/2/search` + classic `startAt` pagination, issue/comment/worklog/transitions — all verified present) | WADL (official; "Jira 9.17.0", the latest Atlassian publishes) | vendored |
| — | **Confluence Data Center REST v1** (`/rest/api/content`, CQL search) | **Atlassian publishes no machine-readable DC spec at all** (no OpenAPI, no reachable WADL) | contract = official HTML REST docs + validation against the real `atlassian/confluence` container |
| `teamcity.json` | TeamCity REST (`/app/rest/server`, `/app/rest/builds`, `/app/rest/buildTypes`) | **OpenAPI 3.0.0** — 268 paths, 238 `components.schemas` with **lowercase** names (`server`, `user`, `build`) | vendored — 2026.1, 2026-08-27. **Mixed authority, see below** |
| `jira-cloud-v3.json` | Jira **Cloud** v3 (421 paths, `/search/jql`) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v1.json` | Confluence **Cloud** v1 (CQL lives only here) | OpenAPI 3.0.1 | vendored — cloud flavor, later |
| `confluence-cloud-v2.json` | Confluence **Cloud** v2 content CRUD | OpenAPI 3.0.3 | vendored — cloud flavor, later |

## TeamCity: vendored 2026-08-27, with **mixed authority**

**Provenance, as supplied by Björn:** downloaded from JetBrains' guest server instance (TeamCity 2026.1), then hand-augmented by Björn with information from the official HTML documentation.

Both halves of that sentence matter, and they do **not** carry the same weight:

| Part of the document | Origin | Trust |
|---|---|---|
| The bulk of the paths and schemas | emitted by a real TeamCity 2026.1 server | **authoritative** — this is what the server says about itself |
| Descriptions, examples and constraints added on top | one human reading the official HTML docs and transcribing | **a human's error bar** — prose read, understood, and re-encoded by hand |

**Why this is written down rather than just appreciated.** When the fidelity gate fails against some clause in this document, the debugging prior depends on which half the clause came from. A server-derived constraint that mockd violates is almost certainly a mockd bug. A hand-added constraint that mockd violates may equally well be a mis-transcription — the HTML docs are prose, prose is ambiguous, and encoding ambiguous prose as a hard JSON Schema constraint is exactly where a well-meaning transcription goes wrong. **Do not bend a fixture to satisfy a hand-added constraint without first checking the constraint**, because a mockd fixture edited to match a wrong constraint is now doubly wrong: it no longer matches the real TeamCity, and the gate is green about it.

That this document is **OpenAPI 3.0.0** and not the Swagger 2.0 originally anticipated is recorded per `fetch.sh`'s own postscript: schemas live under `components.schemas`, not `definitions`, and their names are **lowercase** (`server`, `user`, `buildTypes`, `builds`, `build`). `crates/knobas-mockd/tests/teamcity_contract.rs` follows that, and validates with format assertion deliberately off — TeamCity overloads `format` to carry its own locator grammars (`BuildLocator`, `"String value"`), which are not JSON Schema formats. The gate pins that the document uses none of the OpenAPI-3.0-only constructs (`nullable`, boolean `exclusiveMinimum`) where OAS 3.0 and modern JSON Schema disagree, so a future re-fetch that introduces one fails with a message naming the dialect rather than an unexplained type error.

**Re-fetching would destroy the hand-added half.** See the warning in `fetch.sh` — `--teamcity` writes `teamcity.json` in place. A re-fetch must be taken to a scratch path and *diffed* against the vendored copy, never written over it.

### Historical: the blocker this replaces (2026-08-25 → discharged 2026-08-27)

For two days TeamCity had golden-fixture validation only, because `teamcity.json` could not be obtained on the development machine: JetBrains publishes no static swagger document at a public URL (four candidates re-verified, all 404/403), the only other source was a running server behind a one-time human first-start wizard, and the host had 13 GiB free on a 98 %-full volume against an image needing roughly 10 GB. Ruling P11(b) blessed golden-only validation as the fallback, and `teamcity_contract.rs` was written **self-arming** so the schema half would start asserting the moment a document appeared, with no code change. It did, and it does. The route that actually worked — JetBrains' **guest instance**, which needs no local server at all — is now recorded in `fetch.sh`; the old "no source anywhere but your own server" framing was too pessimistic.

Rules:
- `SHA256SUMS` pins the exact documents. mockd mocks the **DC shapes** (Jira v2, Confluence v1) and validates its responses — and, via request-validation middleware, the adapters' requests — against the strongest available contract per API.
- **Version alignment:** the Jira WADL is also published per-version (`docs.atlassian.com/software/jira/docs/api/REST/<version>/jira-rest-plugin.wadl`); once Björn's instance versions are known, re-pin the WADL and the container tags to exactly those versions.
- Refresh deliberately via `fetch.sh`, review the diff, then update `SHA256SUMS` — never let a contract change slip in silently.
- Flowrun is an internal system with no public spec: its stub in `knobas-mockd` *defines* the assumed contract, to be validated against the real instance when one becomes available.
- The Jira Cloud `/search` removal (HTTP 410, Oct 2025) is **Cloud-only** — DC keeps `/rest/api/2/search`. Adapters must not confuse the two dialects; the source config carries a `flavor: datacenter | cloud` field.
