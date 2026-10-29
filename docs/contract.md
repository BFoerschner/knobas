# The frozen contract

What a PR may not change without announcing it as §10.8 says, and the conventions every adapter and every IPC command keeps. Cut to its binding sections on 2026-10-29; the M1 freeze proposal it grew from, with its stream plans, open points, rulings and the amendments round, is archived verbatim in `docs/archive/contract-2026-08-m1-freeze.md`. Section numbers are kept here, as stubs where the body moved, so a citation like "§10.6(c)" or "§9" still resolves. The design doc `docs/specs/2026-08-23-knobas-design.md` is cited inline as spec §n.

---

## 1. Migration `0002` (single-writer: orchestrator)

Archived (`docs/archive/contract-2026-08-m1-freeze.md` §1). The migration itself is `crates/knobas-db/migrations/0002_m1_cockpit.sql`; the rule that no applied migration is ever edited is in §10.8.

## 2. IPC surface for M1

§2.1 to §2.5 listed the M1 commands and are archived. The command surface is now the generated mirror (`app/src/lib/ipc/generated/`) plus the §10.8 ledger; §2.6 is the rule that keeps the mirror honest.

### 2.6 TS mirrors (`app/src/lib/ipc/*.ts`)

~~Mirrors stay hand-written (M0 rationale: `tauri-specta` is still an RC).~~ Amended by #1325 (2026-10-19, `docs/archive/contract-10.8/1325-the-ipc-mirror-is-generated.md`): every barrel's **types** are generated from its Rust types by `ts-rs` into `app/src/lib/ipc/generated/<barrel>.ts` (the last of the seven switched by #1331, the shape-assertion module retired by #1332), and `just check` fails on a stale one; the barrel re-exports them, and its `invoke` functions stay hand-written. Mechanical mapping: `DateTime<Utc>` → `string` (RFC 3339), `Option<T>` → `T | null`, `i64/u64/f32` → `number`, `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields, snake_case enums → string unions, tagged enums → discriminated unions. The function surface:

```ts
// app.ts
export function ping(): Promise<string>;
export function appStatus(): Promise<AppStatus>;
export function frontendReady(): Promise<void>;
export type DbState = { state: "starting"; detail: string | null } | { state: "migrating" }
                    | { state: "ready" } | { state: "failed"; message: string };

// sources.ts
export function listAdapters(): Promise<SourceDescriptor[]>;
export function listSources(): Promise<SourceSummary[]>;
export function addSource(input: NewSource): Promise<SourceSummary>;
export function updateSource(id: string, patch: SourcePatch): Promise<SourceSummary>;
export function deleteSource(id: string, purgeItems: boolean): Promise<void>;
export function setSourceSecret(id: string, secret: SecretInput): Promise<CredentialHealth>;
export function testSource(draft: SourceDraft): Promise<ConnectionReport>;
export function credentialHealth(): Promise<CredentialHealth[]>;
// SUPERSEDED (§10.3): an optional channel does not decode. Two functions:
export function syncNow(sourceId: string): Promise<number>;
export function syncNowWithProgress(sourceId: string, progress: Channel<SyncProgress>): Promise<number>;
export function syncAll(): Promise<number[]>;
export function syncStatus(): Promise<SourceSyncStatus[]>;
export function listSyncRuns(sourceId: string | null, limit: number): Promise<SyncRunRow[]>;
export function dbStats(): Promise<DbStats>;
export function reindexFts(): Promise<void>;
export function demoLoad(): Promise<SyncReport>;

// search.ts
export function search(query: SearchQuery): Promise<SearchResponse>;
export function launcherHome(): Promise<LauncherHome>;
export function smartLists(): Promise<SmartListSummary[]>;
export function smartListItems(id: string, limit: number): Promise<SearchResponse>;

// entity.ts
export function getEntity(entityId: string): Promise<EntityDetail>;
export function listEntities(filter: EntityFilter, limit: number, offset: number): Promise<EntityPage>;
export function recentActivity(limit: number, entityId?: string): Promise<ActivityRow[]>;

// index.ts — barrel + `ipcErrorMessage` (M0) + `EVENTS` name constants.
```

---

## 3. Keychain convention (`keyring` 4.x — roadmap §4)

**Nothing secret ever reaches Postgres** (§14). The DB holds `auth_kind`, the base URL, and non-secret config (a username lives in `source_config.config.username`); the keychain holds one item per source.

```
service = "dev.knobas.desktop"            release builds (the Tauri identifier)
        = "dev.knobas.desktop.dev"        cfg!(debug_assertions)
        = "dev.knobas.desktop.test.<run>" tests (MemoryStore in practice — see below)
account = "source:<source_id>"            e.g. "source:jira", "source:jira-eu"
value   = {"v":1,"kind":"pat","secret":"…"}   JSON envelope
```

Rationale: one item per source (not per auth method) means changing PAT → password rewrites in place instead of orphaning an item; the envelope's `kind`/`v` leave room for OAuth (access + refresh + expiry) without a naming change (§3 "OAuth later"); a dev-suffixed service keeps `just dev` off the real credentials, and gotcha 10 (unsigned dev builds re-prompt every run) is then at least survivable.

**Test/CI seam — mandatory.** CI runs on Linux (`.github/workflows/check.yml`), where `keyring` needs a live secret-service over D-Bus. Stream F therefore ships

```rust
// crates/knobas-secrets/src/lib.rs
pub trait SecretStore: Send + Sync {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError>;
    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError>;
    fn delete(&self, source_id: &str) -> Result<(), SecretError>;   // absent ⇒ Ok(())
}
pub struct KeyringStore { service: String }     // the real one
pub struct MemoryStore { .. }                   // every test, and CI
pub struct Secret { pub kind: knobas_source::AuthMethod, pub value: String } // Debug redacts
#[derive(Debug, thiserror::Error)] pub enum SecretError { Backend(String), Unavailable, NotFound }
// `Locked` was renamed `Unavailable` in PR #19 and its meaning narrowed: it covers exactly the
// six OSStatus values `apple-native-keyring-store` maps to `NoStorageAccess`, and NOT
// errSecInteractionNotAllowed / errSecAuthFailed / errSecUserCanceled — a locked keychain or a
// denied prompt on macOS arrives as `Backend`. An "Unlock your keychain" affordance built on
// `Unavailable` alone is dead code there. See the crate docs before building on either variant.
```
`just check` must never touch a real keychain: the store is injected into `AppState`, and `KeyringStore` is exercised only by a `#[ignore]`d macOS-local test.

**Lifecycle (SourceConfig ↔ keychain), single source of truth:**

| Step | Order of operations |
|---|---|
| **Test (draft)** | `test_source` holds the typed secret **in memory only**; nothing is written until Save. A draft for a saved source (`source_id` set, `secret: None`) re-tests the stored one. |
| **Create** | `put` the secret → insert `source_config`. If the insert fails, `delete` the secret (no orphan item). Never the other way round: a config row with no secret is a source that silently 401s. |
| **Re-enter** | `set_source_secret` → `put` (overwrite) → `test_connection` → write `auth_state` + `auth_checked_at` → emit `source:health`, clear `backoff_until`. |
| **Delete** | delete `source_config` (cascade leaves `knobas.entity` rows alone — links and notes point at them) → `delete` the secret, ignoring absence. `purge_items: true` additionally deletes that source's `sync.item` rows and tombstones its entities. |
| **Missing** | `get` returns `None` for a configured source ⇒ `auth_state = 'missing_secret'`, the scheduler skips it (no request, no backoff churn), the sources view offers *Re-enter*. |
| **Export** | §14: source configurations export **without secrets**; a restored config lands as `missing_secret`. |

---

## 4. Adapter conventions (streams A, B, C)

### 4.1 Universal rules

- **Instance id = EntityRef namespace = `source_config.id`.** Lowercase slug `[a-z][a-z0-9-]{0,31}`, chosen at add time (default: the adapter kind), **immutable afterwards** — it is baked into every entity id, link and activity row. Two Jiras are `jira` and `jira-eu`. `display_name` is freely renameable; `SourcePatch` deliberately has no `id`. The sync engine already refuses blank / `:`-bearing / reserved ids (`check_source_id`), and the contract battery refuses them at certification time.
- **Key = the most stable identifier the source exposes**, prefixed by the source's own type word where the instance's id space is not unique. Keys may contain `:` and `#` (`EntityRef` splits on the first `:` only).
- **`write_ops: []`, no `Capability::Write` — M1 is read-only toward every source.** Confirmed against roadmap §2: write-back (Jira status/comment/create, Gitea branch/PR, TeamCity trigger) is M2's identity release; §5's write-back list is not M1. `write()` returns `SourceError::Protocol` for everything, which the battery already enforces both ways (`Capability::Write` ⇔ non-empty `write_ops`).
- **Cursor discipline (battery clause 2):** a run that emitted **nothing** returns the cursor it was handed, byte-identical. One exception: Jira's hourly `board_read` stamp (#1700, the Jira cursor row below). Watermarks advance only when at least one item was pushed. Cursor content is an opaque adapter-defined string; every adapter uses a **versioned JSON envelope** (`{"v":1,…}`) so a later shape change is detectable and an unrecognised version means "full sync".
- **Full sync = reconcile.** After a `cursor: None` run, the engine sweeps rows whose `synced_at` predates the run (see §1) — adapters need do nothing, but must not fake a full sync by resetting a watermark mid-run. *Amended by #2186 (2026-10-26, §10.8): an adapter handed a cursor that walked a kind whole anyway (its scope changed) says so through `Sink::walked_whole`, and the engine sweeps that kind as after a `cursor: None` run.*
- **Fault classification is user-visible** (SPI doc): 401 **and** 403 → `Unauthorized` (a Jira DC 403 after repeated failures is the CAPTCHA lockout, `X-Authentication-Denied-Reason` — the user must act, so *Re-enter* is the right offer); connect/DNS/TLS/timeout → `Unreachable`; anything else → `Protocol`. Identical mapping from `test_connection` and mid-`sync`.
- **HTTP stack** (roadmap §4): `reqwest` 0.13 with rustls + native roots (corporate CAs), `reqwest-middleware` + `reqwest-retry` (3 attempts, exponential, honours `Retry-After`, retries 429/502/503/504 and connect errors only), `governor` for a per-instance rate limit, connect timeout 10 s / request timeout 30 s, `User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`. No `tauri-plugin-http` (it pins reqwest 0.12). The shared builder + status→`SourceError` mapping live in `crates/knobas-http` (proposal **P8**); *fallback: each adapter carries a ~60-line private `http.rs` and the orchestrator deduplicates in the exit sweep.*
- **Normalization**: `title` = the source's one-line summary; `body_text` = title + description + comment texts joined by blank lines (what FTS indexes, mirroring `knobas-source-mock`); `payload` = the raw record verbatim (§3a re-mapping guarantee); `author` = the source's username string (display-name mapping is M2's people work); `updated_at` = the source's own timestamp, never `now()`.
- **Rate-limit defaults** (config-overridable per source): Jira 20 req/s burst 40 (since #2167, §10.8; was 5 and 10), Gitea 10 req/s burst 20, TeamCity 5 req/s burst 10. Page sizes: Jira 100, Gitea 50, TeamCity 100.

### 4.2 Per source

| | **A — Jira DC** | **B — Gitea** | **C — TeamCity** |
|---|---|---|---|
| `adapter_kind` | `jira` | `gitea` | `teamcity` |
| kinds (`KindInfo.id`) | `ticket` (JI) | `repo` (RE), `branch` (BR), `pr` (PR), `commit` (CM) | `build` (BU), `build_config` (BC) |
| key form | issue key: `jira:PAY-231` | `gitea:owner/repo`, `gitea:owner/repo#142`, `gitea:owner/repo@<sha40>`, `gitea:owner/repo@refs/heads/<name>` | `teamcity:build:<buildId>`, `teamcity:buildType:<buildTypeId>` |
| auth | Bearer PAT (DC ≥ 8.14) or Basic user+password | `Authorization: token <pat>` | Bearer token or Basic |
| test_connection | `GET /rest/api/2/myself`, version from `/rest/api/2/serverInfo`, **`GET /rest/api/2/field`** for the Epic Link custom field id (#297 — reported as `ConnectionInfo.discovered["epic_link_field"]`; a 404 — and a 501, which is what mockd answers for a path it declares and does not serve — reads as "no field table" and does not fail the connection, while a 401 there is still a refused credential) | `GET /api/v1/user`, version `/api/v1/version` | `GET /app/rest/server` |
| read endpoints (M1) | `GET /rest/api/2/search` (`jql`, `startAt`, `maxResults`, `fields`, `expand=renderedFields`) — classic `startAt`/`total` pagination, **never** Cloud's `/search/jql` (gotcha 4); `GET /rest/api/2/issue/{key}` incl. `comment`, `worklog` in `fields`/`expand`; **what the user may write** (#2052): `GET /rest/api/2/mypermissions?projectKey=…`, once per project asked about (a ticket's asks in its key's project), read for `Source::refused_writes`: `comment` needs `ADD_COMMENTS`, `transition` `TRANSITION_ISSUES`, `log_work` `WORK_ON_ISSUES`, `create_ticket` `CREATE_ISSUES`. Only `projectKey` is sent (the WADL declares no `permissions` filter; Jira 10.3.24 answers all 75 keys either way). No write is sent to find out | `/api/v1/repos/search`, `/repos/{o}/{r}/branches` (and `/repos/{o}/{r}/git/refs/heads`, read whole, after a branch walk of more than one page, #1716), `/repos/{o}/{r}/pulls?state=all&sort=recentupdate`, `/repos/{o}/{r}/commits?sha=&since=` (`&until=` on a catch-up, #1624; both server-side and inclusive on Gitea, and ignored by Forgejo 17, which declares neither, so the walk holds both bounds itself, #2034), `/repos/{o}/{r}/issues/{index}/comments` (fifth read, M2 ruling B1, config-gated; **not a paged listing** — `since`/`before` only, read whole in one request; see the #131 amendment); `/repos/{o}/{r}/teams` (#966, read whole, only when the repository's record is written) | `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}`, the project tree `GET /app/rest/projects?fields=…` (#849, read whole, refused when it reports a further page); for a build configuration's source view (#664) `GET /app/rest/buildTypes/id:{id}?fields=…` and the configuration's last 20 builds through `/app/rest/builds`; for a build's source view (#665) `GET /app/rest/builds/id:{id}?fields=…`, `GET /app/rest/changes?locator=build:(id:{id})&fields=…`, `GET /app/rest/builds/id:{id}/artifacts?fields=…`, and on its deferred tabs `GET /app/rest/testOccurrences?locator=build:(id:{id}),status:FAILURE,count:100&fields=…` and the log's tail from `GET /downloadBuildLog.html?buildId={id}` (not `/app/rest`: plain text, no `fields=`) — **always** `Accept: application/json` (else XML) and always an explicit `fields=`; **what the user may write** (#2040): `GET /app/rest/users/current/permissions` with no locator, `/app/rest/projects` for the ancestry, and `GET /app/rest/buildTypes/id:{id}?fields=projectId` or `GET /app/rest/builds/id:{id}?fields=buildType(projectId)` for an entity's project, read for `Source::refused_writes` and beside a configuration's view; the same read answers whether a run may carry its overrides (`customize_build_parameters`) and the queue's top (`reorder_build_queue`, #2055). No write is sent to find out: `POST /app/rest/buildQueue` with a build type queues a build |
| write endpoints | `POST /rest/api/2/issue/{key}/comment`, `GET`+`POST /rest/api/2/issue/{key}/transitions`, `POST /rest/api/2/issue` (M2, #43); `POST /rest/api/2/issue/{key}/worklog` — `started` in `yyyy-MM-dd'T'HH:mm:ss.SSSZ` (milliseconds and a numeric offset both mandatory), `timeSpentSeconds`, `comment`, **no `adjustEstimate`** so Jira's own `auto` applies (M3.1, #280) | `POST /repos/{o}/{r}/branches`, `POST /repos/{o}/{r}/pulls`, `POST /repos/{o}/{r}/issues/{index}/comments`, `POST /repos/{o}/{r}/pulls/{index}/reviews` (M2, #43); from a source view that offers writes, `POST /repos/{o}/{r}/branches` and `POST /repos/{o}/{r}/issues` with the body `{}`, sent once each to learn what the token may write, and by `Source::refused_writes` once per repository and scope category asked about (#2054): Gitea refuses them for the scope (403) or for the body (422) and changes nothing (#2030) | `POST /app/rest/buildQueue` for both trigger and re-run (M2, #43) |
| cursor | `{"v":1,"updated_to":"2026-08-24T09:14:00Z"}`; JQL `updated >= "<watermark − 2 min>" ORDER BY updated ASC`. The 2-minute overlap is mandatory: **JQL time resolution is one minute**, so an exact-boundary watermark drops items. Re-delivery is free — upserts are idempotent. **Superseded in part by the #345 amendment below** (2026-09-04): the version is `2` and `seen` recognises a record by fingerprint rather than by its timestamp. The JQL and the overlap are unchanged. *Superseded in part by #1623 (2026-10-22, "Amendments from the Jira walk order" below): the walk orders by `created`, and by `id` inside a tie since #1720.* **Amended by #1700** (2026-10-23): the envelope gains an optional `board_read` (`{"at":…,"scope":…}`, the server time of the last run that read the Jira boards and the *Boards and filters* setting it read under); an idle run within the hour of it, under the same setting, makes no board request and carries the delivered views forward, and the views' keys are paged at 1,000; an optional field with a default, so still version `4`. The stamp is battery clause 2's one exception: an idle run an hour on reads the boards and returns a cursor whose `board_read` moved though it emitted nothing, at most once an hour, so the hour restarts. The engine counts that run as a change and writes one `synced` line for it. **Amended by #2181** (2026-10-26): the envelope gains an optional `scope`, the JQL clause `projects` or `jql_filter` rendered to (empty for none); a run under another scope drops `updated_to` and `seen` and walks the new scope in full, keeping the views. An envelope without the field matches no scope, so each source walks in full once after the upgrade; an optional field, so still version `4`. **Amended by #2186** (2026-10-26): a walk from no watermark says it walked `ticket` whole (`Sink::walked_whole`), so the engine retires the tickets it did not return, a project taken out of the scope among them. | `{"v":1,"repos_listed_at":"…","repos":{"owner/repo":{"pulls_updated_to":"…","commits_since":"…","branches_hash":"…"}}}` — per-repo watermarks; a repo added upstream is picked up by the repo-list re-listing each run. **Superseded in part by #764** (2026-10-14, `docs/archive/contract-10.8/764-gitea-cursor-fingerprints-on-the-watermark.md`): what sits on the pull-request watermark second, and the repository record, are recognised by fingerprint rather than by number or timestamp; still version `3`. **Amended by #1624** (2026-10-22): an incremental pull-request or commit walk that its budget or page cap stops before the watermark leaves an optional `pulls_catching_up` / `commits_catching_up` beside it (the oldest record examined, where the next run continues; the newest, adopted as the watermark once the window below is read), and the watermark does not move until then; optional fields, so still version `3`. **Amended by #2039** (2026-10-25): `commits_catching_up` gains an optional `page`, the page of the cut branch's listing the cut fell on. A server that ignores `until=` (Forgejo 17) lists that branch from its head, and the next run goes to `page` once the branch's first page holds only commits above the window, provided the page still starts above it; otherwise it reads on from page 2. A catch-up without `page` (written before #2039) is read from the branch head as before; an optional field, so still version `3`. The repository listing pages over `sort=id&order=asc`. *Amended by #1979 (2026-10-24, "Amendments from the anchored walks" below): a full sync lists twice and walks both.* **Amended by #2186** (2026-10-26): a position holding a repository that `owners[]` or `repos[]` no longer admits is dropped and the run is a full sync; a full sync says it walked `repo` and `branch` whole (`Sink::walked_whole`), so the engine retires what it did not return, the repositories out of scope among them. Shape unchanged, still version `3`. ETags/`If-None-Match` are an **optimization to verify against the real container**, not a contract. | `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional `state:running,state:queued` poll** each run — a running build mutates without a new id. **Superseded in part by #1448** (2026-10-21, `docs/archive/contract-10.8/1448-run-test-summary.md`): the version is `2`, so a position written before `BUILD_FIELDS` asked for the test counts reads as unknown and the next run re-reads the window. **Amended by #2181** (2026-10-26): a scoped source's envelope gains an optional `scope` (`"projects:<ids>|build_types:<ids>"`), absent for an unscoped one, which keeps its bytes; a cursor under another scope is read as no cursor, so the run reads each configuration's window of the new scope as a first run does; the engine, holding a position, does not sweep. Still version `2`. |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`), `projects[]` or `jql_filter`, `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `epic_link_field` (per-instance id — **filled by *Test connection*** since #297, never an example to copy) | `owners[]`/`repos[]` allowlist, `username`, `clone_urls[]` (#683: other addresses CI clones from, the SPI's `clone_urls::CONFIG_KEY`, read by the app for commit hints; §10.8) | `project_ids[]`, `build_type_ids[]`, `builds_per_config`, `username` (identity — filled by *Test connection*, used for `@me`/My items) |
| contract source | `testenv/specs/jira-dc-rest.wadl` + `knobas-mockd` | the **real** pinned Gitea container (roadmap §3) | TeamCity swagger extracted per `testenv/specs/fetch.sh` + `knobas-mockd` |
| client | hand-rolled reqwest (~5 endpoints) | hand-rolled reqwest; codegen from `/swagger.v1.json` is permitted by roadmap §4 but is stream B's internal call | hand-rolled reqwest |

#### D — Confluence DC (M3.2, issues #284 and #287)

Added after the M1 table above rather than as a fifth column, because the table's columns are M1's three streams and a fourth would misread as one. The universal rules of §4.1 apply unchanged.

| | **D — Confluence DC** |
|---|---|
| `adapter_kind` | `confluence` |
| kinds (`KindInfo.id`) | `page` (PG), `full_sync_exhaustive: true` over the configured space list |
| key form | the **content id**: `confluence:98307`. Not the title and not `space:title` — a page renamed or moved keeps its content id, and every link, note and timer drawn to it survives the rename |
| auth | Bearer PAT (DC ≥ 7.9) or Basic user+password |
| test_connection | `GET /rest/api/user/current`, reporting `username` as the account. **No version**: Confluence DC publishes it only through the administrators-only `/rest/api/settings/systemInfo`, so `ConnectionInfo::server_version` is `None` rather than an *unreachable* for an ordinary account |
| read endpoints | `GET /rest/api/content/search` (`cql`, `limit`, `expand`), the `_links.next` continuation it answers with, `GET /rest/api/content/{id}/child/comment` (`start`, `limit`, `expand`) as the completion path, and `GET /rest/api/content/{id}` (`expand`) — the comment walk's second half, which resolves a new or edited comment to the page it is on (#287, #2073); **what the user may write** (#2053): `GET /rest/api/content/{id}?expand=space,operations` (the page's space, and `update` among this user's operations on it) and `POST /rpc/json-rpc/confluenceservice-v2/getPermissions` with `["<space key>"]` (the user's own space permissions, `view`/`modify`/`comment`/`admin`; a read, and the one Data Center 9.2 offers an ordinary user, since `/rest/api/space/{key}/permissions` is administrators-only), read for `Source::refused_writes`. No write is sent to find out. Atlassian deprecates that JSON-RPC service: where it is off or gone, comments and creates are no verdict and an edit is judged from the page's operations alone |
| write endpoints (#286) | `POST /rest/api/content` for a page (`type: page`, `space.key`, `ancestors[0].id`, `body.storage`) and for a comment (`type: comment`, `container.{id,type}`, `body.storage`); `GET /rest/api/content/{id}` then `PUT` the same for an edit. **The content `PUT` replaces the record**, so the title and the content type are read back and sent again — a request that omitted the title would blank it — and `version.number` is sent as `base_version + 1`, which is how Confluence is asked to abort with a 409 when somebody else got there first |
| `expand=` | `body.storage,ancestors,space,version,history,children.comment.body.storage,children.comment.version,children.comment.history` — the storage format kept verbatim in `payload`, the space as ADR-0010's project, the ancestors as the launcher's path, `version.number`/`version.when` as the cursor's identity and (since #286) as what a section edit is made against, and the discussion **with its dates and its authors** in one request instead of one per page. Two tickets asked for the comment expansions and both reasons are kept: the dates are what let a comment date the page it is on (#287, below), and `version.by`/`history.createdBy` are the byline the detail's comment section renders (#286). They are the same expansions the completion path already asked for |
| paging | **`_links.next`, followed verbatim**, and there is no `total`: a content search reports `size` (this page) and a next link. `limit` is capped at **50** by the server once a body is expanded, so `page_size` is refused above it rather than silently clamped. A continuation link that is not a path rooted at the instance is refused — a walk that stopped early must never be reported as a completed one, because the kind is exhaustive. *Superseded in part by #1976 ("Amendments from the anchored walks" below): the page walk sends `start` itself and asks each result from the last page read; it still ends where no `_links.next` comes back.* |
| cursor | `{"v":1,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"…"}]}`; CQL `type = page [AND space in (…)] AND lastmodified >= "<watermark − 2 min>" order by lastmodified asc`. The 2-minute overlap is mandatory for the same reason as Jira's: **a CQL date literal is `"yyyy-MM-dd HH:mm"` and therefore minute-resolution**, so an exact-boundary watermark drops items. Identity in `seen` is `(content id, version.number)` and not `(id, timestamp)` — Confluence's version counter closes the "edited twice in one second" hole the Jira cursor documented (Jira closed it too in the #345 amendment below, with a record fingerprint; this row is the precedent that one cites). **Superseded in part by #860** (2026-10-16, `docs/archive/contract-10.8/860-862-docs-search-rail-and-writes.md`): the version is `2` with the shape unchanged, so the first sync after the upgrade is a full one and every page's `body_text` gains the Docs search's marks. **Amended by #1138** (`docs/archive/contract-10.8/1138-confluence-positions-sweep.md`): the envelope gains `positions`, and an incremental run adds a positions sweep, version unchanged. *Superseded in part by #1976 (2026-10-24, "Amendments from the anchored walks" below): the walk orders by `created asc, id asc`, the bound stays on `lastmodified`.* **Amended by #2181** (2026-10-26): the envelope gains an optional `scope`, the `space in (…)` clause (empty for none); a cursor under another space list is read as no cursor, so the run syncs the new list in full. An envelope without the field matches no list; still version `3`. **Amended by #2186** (2026-10-26): a walk from no watermark says it walked `page` whole (`Sink::walked_whole`), so the engine retires the pages of a space taken out of the list. |
| zone | CQL literals carry **no zone** and are read in the instance's own. It is learned from a timestamp the server itself rendered (`version.when` on the run-start probe), never from a timezone database and never from an admin endpoint. Unknown falls back to UTC−12, not UTC: guessing the offset *high* moves the query's lower bound forward and skips edits permanently, guessing it low only re-reads them |
| ceiling | the run-start probe is the **same scope**, `order by lastmodified desc`, `limit=1`, `expand=version`. Its `version.when` is the ceiling the watermark may not pass (`CONTEXT.md`, *Watermark*), so a page edited *during* a run cannot carry the position past run start and hide every other edit made while it ran. Witnessed, not clocked — `now()` is guaranteed too high the moment the two clocks disagree. **The never-backwards rule outranks the clamp**: where the previous watermark is already *above* the ceiling — the page that set it was deleted, or moved out of the configured spaces — the position stays where it was rather than being dragged back to a ceiling now older than it. The clamp does not bind on that one run, which costs a re-walk; accepting it would cost a source that re-delivers its recent history on every poll and never settles |
| call order | **`/rest/api/user/current` is the first call of every run**, before the probe and before the walk. A content search is a read a server may allow anonymously, and where it does an unresolvable credential answers 200 with an empty result set — which on an exhaustive kind is the engine's licence to tombstone the mirror. The identity call has no anonymous answer. This is the Confluence spelling of the `/serverInfo`-first ordering issue #276 measured on Jira |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`; Cloud refused by name), `spaces[]` (**empty = every space the account can see**), `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `page_size` (1–50, default 50) |
| write ops (#286) | `comment`, `update_page`, `create_page`, with `Capability::Write`; every other op is refused by name. `comment` is the SPI's existing op re-used with the **page** as its container — a reply on a page is the same act as a reply on a ticket, and an adapter-shaped variant of one is what ADR-0006 rejects. `create_page` and `update_page` are M3.2's ADR-0006 growth and carry the §10.8 entry below. This is also what makes an inbox mention on a Confluence source offer a button (#287): `Category::Mention` asks for `comment`, `knobas_app::inbox::offer` keeps only what the descriptor declares, and declaring it here is all that was needed — `a_mention_offers_comment_from_a_source_that_declares_it` flipped on its own |
| contract source | **the real container, and nothing else.** ADR-0013: Atlassian publishes no machine-readable Confluence DC spec, so `knobas-mockd` has no Confluence half and port 8211 stays unreserved. `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` (the adapter) and `crates/knobas-app/tests/confluence_live.rs` (the write path through the queue, #286) against the seeded instance are the only witnesses, run by `just atlassian-live` |
| comments (#287, #2073) | a **second CQL query per run**, the comment walk: `type = comment [AND space in (…)] [AND lastmodified >= "<watermark − 2 min>"] order by lastmodified asc`, whose results are resolved to the pages they are on and emitted as ordinary `page` items. It exists because a comment is *separate content*: posting or editing one does not move its page's `lastmodified` (Data Center 9.2.21), so the page walk can never reach a comment on a page nobody has edited since. Until #2073 it was `mention = currentUser() AND type in (page, comment)`, and every other comment waited for a page edit or a full sync; the inbox finds a mention in the stored page payload, not by which walk delivered the page, and a mention in a page body moves the page's `lastmodified`. A full sync's comment walk starts at the new watermark less two minutes, since the page walk has just delivered every thread. It **does not move the watermark** — the two walks share one cursor and a comment posted today would put every page edit below the next run's lower bound — but it does contribute its records to `seen`, which is what makes an idle poll idle (battery clause 2), under a cap of their own (2,000 beside the page walk's 500) so a busy discussion cannot evict page entries. A comment **deleted** in Confluence leaves no search hit and waits for a full sync |
| §4.1 `body_text` carries a rendered `@name` | a Confluence mention is markup (`<ac:link><ri:user ri:userkey="…"/></ac:link>`), so stripping tags leaves no trace of who was named and `knobas_core::inbox`'s mention rule — which reads `body_text` for `@name`/`[~name]` — could never fire. `storage::to_text` therefore renders `ri:user`: `ri:username` as whoever it names, `ri:userkey` **only** when it is the key `/rest/api/user/current` reported for this credential. Every other link renders as nothing: ADR-0007's miss direction, chosen so an unresolvable key costs a mention knobas never claims rather than claiming somebody else's. This is the same class as TeamCity composing `"{state} {status}"` into `body_text` (below); `payload` stays verbatim (§3a) |
| §4.1 `updated_at` is the newest of the page **and its discussion** | a comment does not move a page's `version.when`, so a page dated by its own edit alone would be a page commented on this morning wearing last year's date — and every reader that filters on recency drops it, `knobas_core::inbox::WINDOW_DAYS` first among them, which would make the comment walk pointless. Jira answers this natively (a comment moves `fields.updated`); `children.comment.version` is what lets Confluence answer it too. **The cursor is unaffected**: it walks and clamps on the page's own `version.when`, because that is what CQL's `lastmodified` matches for a page. A server that will not expand the comment dates falls back to the page's stamp, which is the miss direction |
| client | hand-rolled reqwest through `knobas-http` (5 reads, 3 writes) |

**Accepted limitation, recorded rather than hidden.** Offset paging over the field being ordered by can *step over* a row: a page at the front of an `asc` order is edited, moves to the end, every row behind it shifts down by one, and the walk's next offset lands one past where it should. The stepped-over page keeps its old timestamp, so no incremental query reaches it either — only the next full sync does. This is the same class the Jira adapter accepts with `startAt` and `ORDER BY updated ASC`, and the ceiling does not close it; `a_page_edited_mid_walk_can_shift_a_row_past_the_offset` pins it so a reader does not build a stronger guarantee on top of it. *The Jira adapter no longer accepts it: see "Amendments from the Jira walk order" below (#1623). Nor does this one since #1976: see "Amendments from the anchored walks" below.*

**Frozen surfaces, #284: one, ratified.** `crates/knobas-source-confluence/**` is a new crate and is not in §10.8's list; the registry row and the app manifest line are the append-only additions §3a's "one crate + one registry line" describes. No migration and no IPC command. The **one** frozen surface this ticket does touch is the IPC schema — `EntityRow.path`, which criterion 5 cannot be met without — and it carries its own §10.8 entry, ratified by the orchestrator and flagged there for Björn.

**Frozen surfaces, #287: none.** The mention walk is one more call on a crate-private adapter trait, one more CQL string and one more `expand` field; the inbox gained Confluence items without `knobas-core` changing. No migration, no `crates/knobas-source/src/**` change, no IPC command, event, module-layout or barrel line, and nothing in `crates/knobas-http/**` — so no §10.8 entry is owed. What it does change beyond this crate is `updated_at` for **every** Confluence page, which the row above states and which the M3.2 exit gate is the place to weigh.

#### E — Uptime Kuma (M4.1, issue #442, read half)

A fifth block for the same reason D is one. **The departures are the substance here** and they are argued in §9, *Amendments from the Uptime Kuma adapter, read half* — this table is the reference row a reader of §4.2 comes looking for, not the argument.

| | **E — Uptime Kuma** |
|---|---|
| `adapter_kind` | `kuma` |
| kinds (`KindInfo.id`) | `monitor` (MO), `full_sync_exhaustive: true`, and it is the only one. No project kind: Kuma's only container is a monitor group and groups are out of scope (spec #427), so there are no project rooms |
| key form | the **monitor id**: `kuma:8`. `monitor_id` is a label on every series a v2 Kuma publishes, and it is the only stable identity `/metrics` carries — a monitor renamed keeps it |
| auth | **API key only** (`AuthMethod::ApiToken`), sent as HTTP Basic with an empty username, which is what Uptime Kuma's `/metrics` accepts. There is no account password to offer and `Pat` is refused by name at `build` |
| test_connection | `GET /metrics` — the same and only request a sync makes. `server_version` from the `app_version` gauge's `version` label; `account` is `None` (an API key belongs to the instance, and `/metrics` never says whose it is); `detail` is the roster size; `discovered` is empty |
| read endpoints | `GET /metrics`, and nothing else. **Uptime Kuma v2 has no other door an API key opens**: the dashboard is socket.io, and a key authenticates exactly this one HTTP endpoint. So the read path is a Prometheus exposition parse, not a REST walk, and the four `monitor_*` gauge families that carry fields are folded by `monitor_id` into one monitor each (`model.rs` names the two that are not read and why). *Since #2100 a refresh reads the same document for one monitor and, on an instance with an account, asks `getMonitor` over socket.io about one the document omits; see "Amendments from the Uptime Kuma refresh".* |
| write endpoints | none. `write_ops` is empty and `Source::write` refuses every op by name. Pause, resume and create are socket.io and are their own ticket (spec #427, story 81) |
| paging | none — `/metrics` is one document with no continuation, so every run reads the whole roster |
| cursor | `{"version":1,"monitors":{"<id>":{"name":"…","digest":"…"}}}` — **a digest of the last corpus, not a position.** `/metrics` carries no timestamp, no paging and no "changed since", so there is nothing to resume from; the cursor exists because battery clause 2 requires an idle incremental run to emit nothing and hand its cursor back. An unchanged Kuma emits nothing; any difference re-emits **every** monitor, which is what keeps "every run is full" and `full_sync_exhaustive` honest. Departs from #442's and spec #427's literal "the cursor is none" — argued in §9 |
| deletion | **the adapter tombstones**, as `SyncItem::deleted`, for every monitor in the cursor that the new document does not publish; the stored names are what a tombstone renders with. The engine's sweep is gated on a cursor-less run (`knobas_sync`, limitation 3) and a scheduled poll always resumes, so the sweep alone would leave spec #427's story 54 unmet in every running installation. Both stay in place and agree. Gitea's cursor is the precedent. **A paused monitor is tombstoned too**: pausing removes every one of its series from `/metrics`, measured on the pinned image, so through this channel *paused* and *deleted* are one observation |
| §4.1 departures | all three — `payload` is synthesized rather than a raw record, `body_text` is `name state type address`, `updated_at` is always `None` — because `/metrics` has no records and no clock in it. Each is argued in §9 |
| `payload_paths` | exactly one, `state` (#277, ADR-0007), written at the top level of every monitor and `null` where Kuma answered a state code this adapter has no word for. `blocked_statuses` is empty by decision: a monitor that is down is broken, not blocked |
| config (`config_schema`) | four transport-tuning properties and **nothing that names the key** — the base URL is `SourceInstance::base_url` and the key is the keychain secret, so neither is a schema property |
| contract source | **the real pinned container, and nothing else** (ADR-0013). `crates/knobas-source-kuma/tests/live_kuma.rs`, run by `just kuma-live`, is the witness; `tests/contract.rs` runs the battery against a *recording* of that server's `/metrics` so `just check` stays docker-free, and if the two disagree the recording is what is wrong |
| client | hand-rolled reqwest through `knobas-http` (1 read, 0 writes). No `chrono`: there is no instant to parse |

**Frozen surfaces, #442: none.** A new crate, plus the registry row and the app manifest line §3a licenses. No migration, no IPC command, event, DTO or layout change, neither barrel, nothing in `crates/knobas-source/src/**` and nothing in `crates/knobas-http/**` — #284's Confluence shape exactly. §10.8 carries a pointer rather than an entry, and says so.

**Adapter construction** (needed by the scheduler and by `test_source`): each adapter crate exposes exactly
```rust
pub fn descriptor_template() -> knobas_source::SourceDescriptor;   // id == adapter_kind
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
pub struct SourceInstance { pub id: String, pub display_name: String, pub base_url: String,
                            pub auth: AuthMethod, pub secret: Option<String>,
                            pub config: serde_json::Value }
```
Where `SourceInstance` lives is proposal **P6** (`knobas-source` vs `knobas-app`). *Fallback: each adapter defines its own `Config` struct plus `from_json`, and stream F's registry adapts — mechanical to unify later.*

---

## 5. `knobas-mockd` contract

Archived (§5 of the archive). mockd is deprecated and frozen (ADR-0013).

## 6. Stream ownership, checkpoints, exit criteria

Archived (§6). Ownership of frozen surfaces is §10.8's list; everything else is the ticket's.

## 7. Open points needing an orchestrator ruling

Archived (§7). Every proposal P1 to P13 was granted or accepted on 2026-08-24; §8 of the archive has the rulings.

## 8. Orchestrator rulings on §7

Archived (§8).

## 9. Amendments and rulings from the plan-authoring round and after

Archived (§9 of the archive, verbatim): the per-adapter amendments from 2026-08-24 to 2026-10-25, each headed *Amendments from the … (date, binding) — issue #N*. They stay binding; an archived §10.8 entry that says "§9 argues it" points there. A new adapter convention lives in the tests and the PR that land it, not in a new §9 section.

## 10. As built

§10.1 to §10.5 and §10.7 (what landed, where it differs, the test-support constructor, per-stream notes, hand verification at the freeze) are archived. One rule from §10.2a still binds: a command that needs the pool takes `State<'_, Lifecycle>` and calls `lifecycle.pool()?`, never `State<'_, AppState>`; `no_command_takes_the_app_state_directly` in `crates/knobas-app/src/commands/mod.rs` scans every command signature for it.

### 10.6 Known, deferred by ruling — not bugs to rediscover

Three things were found in review and **deliberately left alone**, because fixing either here would take a decision that belongs to the stream that owns it. Recorded so the next person to find them knows they are already known, and what the answer is.

**(a) A command that arrives before the database is up returns a raw string, not `IpcErrorCode::NotReady` — stream D's.**
Every command takes `State<'_, AppState>`, and `AppState` is managed only once Postgres is up. A `#[tauri::command]` resolves *every* argument before its body runs, so a call that beats bring-up is rejected by Tauri itself with `"state not managed"` — a bare string, no code, nothing the frontend can branch on. `NotReady` is in `IpcErrorCode` precisely for this and is currently unreachable from it.

It is **latent today**: `run`'s `setup` hook blocks on bring-up and the window is created hidden, so no webview exists to call anything until `AppState` is managed. It goes **live the moment stream D makes bring-up asynchronous** (the M0 carry-over: window first, real loading state driven by `db:state`), which is the same PR that makes the window able to call early. So it is D's, and D must not solve it by adding a guard to each command.

*Required approach:* the shape §2.1 already mandates for `app_status` — a small `Lifecycle` state managed at **build** time, before `setup` runs, holding "starting / migrating / ready / failed". Commands that need the pool take `State<'_, Lifecycle>` (always present) and ask it for the pool, returning `IpcError::not_ready(..)` when there is none; they must not take `State<'_, AppState>` directly. That keeps the answer in one place and keeps `NotReady` reachable.

**(b) The launcher query carries a window function that costs an extra sort — stream E's.**
`count(*) over (partition by i.kind)` gives each group its true pre-`LIMIT` total, and PostgreSQL sorts by `kind` to compute it, on top of the rank sort. On the < 100 ms hot path (§6.3) that is a real cost at corpus scale.

Deliberately not optimised here. Stream E rewrites this query anyway — it has to, for filters and smart lists — and E owns the < 100 ms benchmark that would tell either of us whether the sort actually matters. Guessing at it now would mean tuning a statement that is about to be replaced, against no measurement. (The *overall* total is no longer a window function: it was moved to its own `count(*)` statement, because a window needs a row to ride on and `limit 0` has none.)

**(c) A sync's HTTP work happens *inside* the advisory-locked transaction — stream F's.**
`knobas_sync::run_once` opens one transaction, takes the source's advisory lock, and calls `Source::sync` inside it, so every second the adapter spends on the network is a second that transaction stays open: it blocks the same source's next run and pins one of the pool's five connections (the M0 concurrency carry-over is the same fact from the other end). With a real adapter that is the whole duration of a paginated sync, not just one request.

Mitigated here, not fixed: `knobas_http::SEND_BUDGET` bounds any single `send` to 45 s with each attempt's timeout shortened to what remains, so one pathological request can no longer hold the transaction for minutes. That bounds the *unit*; it does not stop a hundred bounded requests from adding up.

*Required approach (F's):* the transaction must not span the network. Either the adapter's fetching happens outside it and only the writes are transactional (per batch, with the cursor advanced in the same transaction as the batch it describes), or the run keeps its own dedicated connection and the advisory lock is held on that rather than on a pool connection the rest of the app is competing for. F also owns the concurrency cap that decides how many of these may be in flight at once. Not attempted here because it changes `run_once`'s transaction boundary — the one thing M0's "a run either lands completely or not at all" guarantee rests on — and that is a design decision, not a contract-PR fix.

### 10.8 The M1 contract is frozen

Each of the following is a frozen surface. A change to one is ruled by the agent on the work (`docs/agents/working-model.md`, *Process*), announced in the PR as this section says, and never made on the side inside an unrelated ticket:

- migration `0003` (and `crates/knobas-db/migrations/**` at all — `0001` and `0002` are never edited: sqlx checksums applied migrations and an edit fails startup on every existing database),
- any change to `crates/knobas-source/src/**` (the `Source` trait, its DTOs, the contract battery),
- the IPC command and event schema, including the `commands/` + `ipc/` module layout and the two append-only barrels (`crates/knobas-app/src/lib.rs`'s handler list, `app/src/lib/ipc/index.ts`),
- `crates/knobas-http/**`,
- `crates/knobas-app/src/{error,profile}.rs`.

**How a change to one of these lands (since 2026-10-29).** The PR that touches a frozen surface says so in its body under a heading **Frozen surface**: which of the five it touches, what changed (one line per migration, command, event, DTO field, `WriteOp` variant or SPI method, naming the file), and the test that pins it. That section is what the merge-manager's deep pass reviews against the ADRs and this document. Nothing else is written down: no applied migration is ever edited, the generated IPC mirror carries the shapes, and the issue and the PR carry the argument. Where an ADR or an older document says "a §10.8 entry", read "a frozen-surface change announced this way". The 307 entries written before this rule, one file per change from 2026-08-25 to 2026-10-27, are archived verbatim under `docs/archive/contract-10.8/`.

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
