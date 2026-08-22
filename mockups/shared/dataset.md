# Shared example dataset — Tidewater Freight

Every mockup uses exactly these keys, names, statuses, people and timestamps, so the 25 mockups are comparable. Do not invent other tickets/repos/pages; you may omit items a screen doesn't need.

**Today is Friday 2026-08-22, current time 14:32.**

## You
**Mara Lindqvist** (`mara.lindqvist`, initials ML) — backend engineer, Payments team, Tidewater Freight (freight forwarding).

## People
- **Jonas Becker** (`jonas.becker`) — senior engineer, reviewer
- **Priya Nair** (`priya.nair`) — tech lead, Payments
- **Tomasz Wójcik** (`tomasz.wojcik`) — SRE, on-call
- **Lena Hoffmann** (`lena.hoffmann`) — product manager

## Configured sources
| id | type | display name | URL | auth | sync state |
|---|---|---|---|---|---|
| jira | Jira Cloud | Tidewater Jira | https://tidewater.atlassian.net | PAT — expires 2026-09-03 (in 12 days) | synced 4 min ago · 1,284 issues |
| confluence | Confluence Cloud | Tidewater Wiki | https://tidewater.atlassian.net/wiki | same PAT as Jira | synced 4 min ago · 3,912 pages |
| gitea | Gitea | git.tidewater.internal | https://git.tidewater.internal | access token | synced 2 min ago · 47 repos |
| teamcity | TeamCity | ci.tidewater.internal | https://ci.tidewater.internal | user + password | **sync failed 38 min ago: 401 Unauthorized** — password may have expired |

Sync schedule: every 5 minutes. Local database: Postgres `knobas`, 212 MB, full-text index up to date. Pending writes queue: 0 (show "1 pending" after the user edits something while TeamCity is 401, if relevant).

Source types offered by *Add source*: Jira, Confluence, Gitea, GitHub, GitLab, TeamCity, GitLab CI, Jenkins, Generic Git remote. Auth methods: user + password, personal access token (PAT), API token, OAuth (coming soon).

## Jira
Projects: **PAY** — Payments Platform; **OPS** — Operations. Workflow: To Do → In Progress → In Review → Done.

Epic **PAY-200** *Payout reliability* (In Progress) contains PAY-231, PAY-228, PAY-240, PAY-219, PAY-236.

| key | summary | type | status | priority | assignee | updated |
|---|---|---|---|---|---|---|
| PAY-231 | Retry failed SEPA payouts | Story | In Progress | High | Mara | today 11:48 |
| PAY-228 | Ledger drift on partial refunds | Bug | In Review | High | Mara | yesterday 16:05 |
| PAY-240 | Payout dashboard latency | Task | To Do | Medium | Mara (assigned today 08:12 by Priya) | today 08:12 |
| PAY-219 | Rotate PSP credentials | Task | Done | Medium | Mara | 2026-08-18 |
| PAY-236 | Payout CSV export for finance | Story | To Do | Low | Jonas | 2026-08-20 |
| OPS-77 | On-call runbook outdated | Task | To Do | Medium | Tomasz | 2026-08-19 |

**PAY-231** description: "When the SEPA batch at the PSP returns a transient error (HTTP 503 or PSP code `TEMP_UNAVAILABLE`), retry the payout with exponential backoff (base 30 s, factor 2, max 5 attempts, jitter). Permanent failures go to the manual review queue."
- Comments: Priya, yesterday 14:20 — "Please make sure the backoff policy matches the design page — finance wants max 5 attempts." · Mara, today 10:15 — "Backoff + jitter implemented in a41f2c, integration test still red (#1187), investigating."
- Estimate 2d · time spent this week 6h 30m · no worklog yet today.
- Links: see *Confirmed links*.

**PAY-228** is *blocked by* OPS-77 (staging verification needs the updated runbook). **PAY-240** description: "Finance reports the payout dashboard takes 8–12 s to load for August. Target < 2 s."

## Gitea — org `tidewater`
| repo | language | local clone | note |
|---|---|---|---|
| payout-service | Rust | ~/code/payout-service | default branch `main`, 312 commits |
| ledger-api | Kotlin | ~/code/ledger-api | |
| ops-runbooks | Markdown | not cloned | |

Branches on `payout-service`: `main`, `feature/PAY-231-sepa-retry` (3 commits ahead of main, checked out locally), `fix/PAY-228-partial-refund-drift` (merged).

Commits on `feature/PAY-231-sepa-retry`:
- `c90d11` today 11:42 Mara — "PAY-231: jitter in backoff, cap at 5 attempts"
- `a41f2c` today 10:02 Mara — "PAY-231 backoff jitter"
- `7be0e4` yesterday 17:30 Mara — "PAY-231: retry SEPA payouts on transient PSP errors"

Pull requests:
- **PR #142** *SEPA retry with exponential backoff* — payout-service, `feature/PAY-231-sepa-retry` → `main`, **open**, by Mara, opened yesterday 17:41. 2 approvals required, 1 given (Priya). Review requested from Jonas today 09:05. Checks: Payout_Build #1188 running · Payout_IntegrationTests #1187 failed. Comments: Jonas today 09:30 — "Should the jitter be bounded? ±20 % feels wide." · Mara today 10:20 — "Bounded to ±10 % in c90d11." Files changed: `src/sepa/retry.rs` (+84 −12), `tests/sepa_retry.rs` (+51), `docs/backoff.md` (+9).
- **PR #144** *Add payout CSV export* — payout-service, by Jonas, **open**, review requested from **you** today 13:50. For PAY-236.
- **PR #139** *Fix ledger drift on partial refunds* — ledger-api, by Mara, **merged** yesterday 16:00. For PAY-228.

## Confluence — space `ENG` (Engineering)
| page | last edited | by |
|---|---|---|
| SEPA payout retry design | today 10:40 (section "Backoff policy") | Mara |
| Ledger reconciliation runbook | 2026-08-19 | Tomasz |
| On-call handbook | 2026-07-02 | Tomasz |
| Payments architecture overview | 2026-08-11 | Priya |
| Standup protocols (parent of daily protocol pages) | yesterday | Mara |

*SEPA payout retry design* excerpt: "## Backoff policy — base 30 s, factor 2, max 5 attempts, jitter ±10 %. Transient codes: HTTP 503, `TEMP_UNAVAILABLE`. After the final attempt the payout is moved to the manual review queue (see PAY-231). ## Manual review queue — *SLA: to be defined.*"

Comment with @mention: Priya, today 12:05, on *SEPA payout retry design* — "@Mara can you add the manual-review queue SLA here?"

## TeamCity — project *Payouts*
| build configuration | last build | status | branch | when |
|---|---|---|---|---|
| Payout_Build | #1188 | **running** — step 3/5 `cargo test` | feature/PAY-231-sepa-retry | started today 11:45 |
| Payout_IntegrationTests | #1187 | **failed** | feature/PAY-231-sepa-retry | today 10:10 · 4 m 12 s |
| Ledger_Deploy_Staging | #412 | success | main | yesterday 16:20 |

Build #1187 log excerpt:
```
test sepa::retry::gives_up_after_max_attempts ... FAILED
assertion failed: attempts == 5 (left: 6, right: 5)
test result: FAILED. 41 passed; 1 failed
```
Build #1187 parameters: `branch=feature/PAY-231-sepa-retry`, `env=staging`. Trigger options for a new build: branch, env (staging/prod-dryrun), "clean checkout" toggle.

## Local notes
| note | edited | links | content |
|---|---|---|---|
| Standup 2026-08-21 | yesterday 09:05 | [[PAY-228]] [[PR #139]] | "Yesterday: drift fix. Today: re-test partial refunds on staging, open PR. Blocker: none." |
| SEPA retry investigation | today 11:20 | [[PAY-231]] [[#1187]] [[SEPA payout retry design]] | "Test expects 5 attempts, code counts the initial try as attempt 0 → off-by-one. Fix: start counter at 1. Also: ask Priya about manual-review SLA." |
| Credentials to rotate | 2026-08-18 | [[PAY-219]] | "TeamCity password expires end of August. Jira PAT expires 2026-09-03." |

## Confirmed links
- PAY-231 ↔ branch `feature/PAY-231-sepa-retry` ↔ PR #142 ↔ builds #1187, #1188 ↔ page *SEPA payout retry design* ↔ note *SEPA retry investigation*
- PAY-228 ↔ PR #139 ↔ page *Ledger reconciliation runbook* ↔ note *Standup 2026-08-21*
- PAY-219 ↔ note *Credentials to rotate*
- OPS-77 ↔ page *On-call handbook*
- PAY-236 ↔ PR #144

## Suggested links (auto-detected, unconfirmed — show as proposals with a reason and a one-click confirm/dismiss)
- commit `a41f2c` "PAY-231 backoff jitter" → PAY-231 — *key in commit message*
- build #1187 → PAY-231 — *key in build parameter `branch`*
- page *Payments architecture overview* → repo `ledger-api` — *mentions `ledger-api` 4×*
- PAY-240 → page *Payments architecture overview* — *text similarity 0.71 ("dashboard latency")*

## Smart lists (saved local queries; the first is not expressible in JQL)
| list | count | note |
|---|---|---|
| My tickets with a failing build | 1 | PAY-231 — joins Jira ↔ Gitea ↔ TeamCity |
| PRs waiting on me | 1 | PR #144 |
| PRs idle > 5 days | 2 | in other repos |
| Pages I edited this week | 3 | |
| Blocked tickets | 1 | PAY-228 |
| Unlogged time this week | 4h 45m | |

Example search query to show: **"sepa retry"** → results: PAY-231 (ticket), *SEPA payout retry design* (page), PR #142, commits a41f2c / c90d11, build #1187, note *SEPA retry investigation*. Each result shows its source and "synced N min ago".

## Inbox (attention queue), newest first
| when | item | actions |
|---|---|---|
| today 13:50 | Jonas requested your review on PR #144 *Add payout CSV export* | Review · Snooze · Done |
| today 12:05 | Priya mentioned you on *SEPA payout retry design*: "@Mara can you add the manual-review queue SLA here?" | Reply · Open page · Done |
| today 10:14 | Build #1187 Payout_IntegrationTests **failed** on feature/PAY-231-sepa-retry | Open log · Re-run · Comment on PAY-231 |
| today 09:30 | Jonas commented on your PR #142: "Should the jitter be bounded? ±20 % feels wide." | Reply · Open PR |
| today 08:12 | Priya assigned PAY-240 *Payout dashboard latency* to you | Open · Start work |
| yesterday 16:30 | PAY-228 is blocked by OPS-77 *On-call runbook outdated* | Open OPS-77 · Ping Tomasz |
Snoozed (1): "Jira PAT expires in 12 days" — returns 2026-08-29.

## Time tracking — today, Friday 2026-08-22, now 14:32
**Timer: running** on **PAY-231** since 13:58 (0:34 elapsed), started manually.

Passive blocks recorded today (from what was open in the app; nothing is keyed to git branches):
| time | context | how |
|---|---|---|
| 08:30–08:45 | Inbox + standup digest | passive |
| 08:45–09:40 | PAY-228 + note *Standup 2026-08-21* | passive |
| 09:40–11:50 | PAY-231 (PR #142, design page, investigation note) | passive |
| 11:50–12:20 | **unattributed** (app open, no entity) | passive — needs assignment |
| 12:20–13:10 | — (no activity, lunch) | gap |
| 13:10–13:58 | ad-hoc label **"Staging DB configuration"** (no ticket) | manual label |
| 13:58–now | PAY-231 | manual timer |

Worklogs already logged: yesterday — PAY-228 1h 30m "Review PR #139 fixes, re-test partial refund drift"; yesterday — PAY-231 3h "Retry loop implementation". **Today: nothing logged yet** (5h 28m tracked).

Week: Mon 7h 45m · Tue 8h 00m · Wed 6h 30m · Thu 7h 10m · Fri 5h 28m so far. Tracked 34h 10m, logged 29h 25m, **unlogged 4h 45m**.

**Worklog draft** (shown when the PAY-231 timer is stopped; the suggested interval is 09:40–11:50 + 13:58–14:32 = 2h 44m, editable): activity in the interval, each a checkbox that adds a line to the comment —
- [x] commit `a41f2c` "PAY-231 backoff jitter" (10:02)
- [x] commit `c90d11` "PAY-231: jitter in backoff, cap at 5 attempts" (11:42)
- [x] replied to Jonas on PR #142 (10:20)
- [ ] triggered build #1188 (11:45)
- [x] edited *SEPA payout retry design* › Backoff policy (10:40)
- [ ] edited note *SEPA retry investigation* (11:20)
Generated comment: "Backoff jitter (a41f2c, c90d11); replied to review on PR #142; updated Backoff policy in the design page." Target: PAY-231 (Jira worklog). Buttons: *Log 2h 44m to PAY-231* · *Edit* · *Discard*.

The "Staging DB configuration" block has no ticket: offer *Log to a ticket…* (suggest OPS-77) or *Keep local only*.

## Standup — today
**Digest (generated)**
- Yesterday: merged PR #139 (ledger-api) fixing partial refund drift; PAY-228 → In Review; implemented retry loop for PAY-231 (7be0e4); logged 4h 30m.
- Today: PAY-231 backoff + jitter (a41f2c, c90d11); fix integration test #1187 (off-by-one in attempt counter); reply to Priya about the manual-review SLA; review PR #144.
- Blockers: PAY-228 staging verification blocked by OPS-77 (runbook outdated, Tomasz).

**Standup protocol 2026-08-22** (minutes; editable; *Save as note* · *Publish to Confluence → ENG › Standup protocols › 2026-08-22*)
- Attendees: Mara, Jonas, Priya, Tomasz, Lena
- Jonas: PR #144 CSV export ready for review.
- Priya: finance wants the manual-review SLA documented by Monday.
- Tomasz: on-call handbook update in progress, ETA Tuesday (OPS-77).
- Lena: finance complaints about dashboard latency → PAY-240.
- Action items: Mara → add SLA to *SEPA payout retry design* (PAY-231) · Tomasz → OPS-77 by Tuesday · Lena → clarify PAY-240 acceptance criteria.
