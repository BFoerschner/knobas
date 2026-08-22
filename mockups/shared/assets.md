# Asset dataset extension — Tidewater Freight infrastructure

Extends `dataset.md` (same company, people, tickets, time). Use these exact names. Today is Friday 2026-08-22, 14:32.

## Asset model
Every asset has: `id`, `type`, `name`, `env` (prod / stage / dev / shared), `owner` (team or person), typed properties from its type's schema, optional **custom properties** (user-added key/value with a type: text / number / date / url / secret), relations, monitors, linked work (contexts, tickets, PRs, pages, notes), and a change history.

**Types and their typed properties**
| type | properties |
|---|---|
| VM | hostname, IP, OS, vCPU, RAM, disk, hypervisor node, env, owner |
| Container | image:tag, ports, restart policy, compose project, runs-on (VM), state |
| Service | name, port, version, repo (Gitea), owner, health URL |
| Reverse proxy | software + version, runs-on (VM), routes |
| Route / URL | URL, target service:port, TLS cert expiry, visibility (internal / public), middleware |
| Database | engine + version, database name, runs-on, size |
| Flowrun instance | env, Flowrun version, URL, runs-on, scenario count |
| Scenario | name, version per env, trigger (cron / webhook / manual), last run, runs today, owner |

**Relation types** (directed, shown from both ends): runs-on · hosts · exposes · routes-to · depends-on · monitored-by · deployed-from (repo / build) · documented-in (page) · in-context.

## VMs
| id | hostname | env | IP | OS | size | hypervisor | owner | notes |
|---|---|---|---|---|---|---|---|---|
| vm-pay-01 | vm-pay-01 | prod | 10.20.4.11 | Ubuntu 24.04 | 8 vCPU · 32 GB · 200 GB | Proxmox `pve-02` | Payments | hosts the prod payout stack |
| vm-pay-stage-01 | vm-pay-stage-01 | stage | 10.20.5.11 | Ubuntu 24.04 | 4 vCPU · 16 GB · 120 GB | Proxmox `pve-01` | Payments | stage stack + staging DB |
| vm-edge-01 | vm-edge-01 | shared | 10.20.1.5 | Debian 12 | 2 vCPU · 4 GB · 40 GB | Proxmox `pve-01` | Platform (Tomasz) | runs Traefik |
| vm-db-01 | vm-db-01 | prod | 10.20.4.20 | Ubuntu 24.04 | 8 vCPU · 64 GB · 1 TB | Proxmox `pve-02` | Platform | Postgres |
| vm-lowcode-01 | vm-lowcode-01 | shared | 10.20.6.20 | Ubuntu 22.04 | 8 vCPU · 32 GB · 300 GB | Proxmox `pve-03` | Integrations (Lena's team) | Flowrun stage + prod |
| vm-lowcode-dev-01 | vm-lowcode-dev-01 | dev | 10.20.6.30 | Ubuntu 22.04 | 4 vCPU · 16 GB · 100 GB | Proxmox `pve-03` | Integrations | Flowrun dev |

Custom property examples already set: vm-pay-01 `backup-window` = "02:00–03:00" (text), `last-patched` = 2026-08-10 (date); vm-edge-01 `acme-email` = ops@tidewater.example (text).

## Containers
| id | name | image:tag | ports | runs-on | env | state | notes |
|---|---|---|---|---|---|---|---|
| c-payout-prod | payout-service | ghcr.io/tidewater/payout-service:1.8.2 | 8080 | vm-pay-01 | prod | running · up 11 d | deployed-from build #1180 |
| c-ledger-prod | ledger-api | ghcr.io/tidewater/ledger-api:2.3.0 | 8090 | vm-pay-01 | prod | running · up 11 d | |
| c-worker-prod | payout-worker | ghcr.io/tidewater/payout-service:1.8.2 (`--worker`) | — | vm-pay-01 | prod | **exited (137) at 11:58** | OOM-killed; monitor down |
| c-payout-stage | payout-service | ghcr.io/tidewater/payout-service:1.9.0-rc1 | 8080 | vm-pay-stage-01 | stage | running · up 2 h | candidate from PR #142 / build #1188 |
| c-ledger-stage | ledger-api | ghcr.io/tidewater/ledger-api:2.3.0 | 8090 | vm-pay-stage-01 | stage | running | |
| c-pg-stage | pg-payments-stage | postgres:16.3 | 5432 | vm-pay-stage-01 | stage | running | the "Staging DB configuration" work is about this |

Restart policy `unless-stopped`; compose project `payments` (prod) / `payments-stage`.

## Services
| id | name | port | version (prod / stage) | repo | owner | health |
|---|---|---|---|---|---|---|
| svc-payout | payout-service | 8080 | 1.8.2 / 1.9.0-rc1 | `tidewater/payout-service` | Payments | /health |
| svc-ledger | ledger-api | 8090 | 2.3.0 / 2.3.0 | `tidewater/ledger-api` | Payments | /health |
| svc-dashboard | payout-dashboard | 3000 | 0.9.4 / — | `tidewater/payout-dashboard` (not cloned) | Payments | / — **slow, PAY-240** |
| svc-traefik | traefik | 443/80 | 3.1 | — | Platform | dashboard :8080 on vm-edge-01 |

## Reverse proxy and routes
**traefik** (`rp-edge`, Traefik 3.1) runs on vm-edge-01 and owns these routes:
| id | URL | → target | cert expires | visibility | middleware |
|---|---|---|---|---|---|
| r-payouts | https://payouts.tidewater.internal | payout-service:8080 (vm-pay-01) | **2026-08-31 (9 days)** | internal | auth-forward |
| r-ledger | https://ledger.tidewater.internal | ledger-api:8090 (vm-pay-01) | 2026-10-14 | internal | auth-forward |
| r-dashboard | https://dashboard.tidewater.internal | payout-dashboard:3000 (vm-pay-01) | 2026-10-14 | internal | — |
| r-flows | https://flows.tidewater.internal | flowrun-prod:9000 (vm-lowcode-01) | 2026-11-02 | internal | — |
| r-flows-stage | https://flows-stage.tidewater.internal | flowrun-stage:9001 (vm-lowcode-01) | 2026-11-02 | internal | — |
| r-payouts-stage | https://payouts-stage.tidewater.internal | payout-service:8080 (vm-pay-stage-01) | 2026-10-14 | internal | auth-forward |

## Databases
| id | name | engine | db | runs-on | env | size |
|---|---|---|---|---|---|---|
| db-payments | pg-payments | PostgreSQL 16.3 (system service) | payments | vm-db-01 | prod | 48 GB |
| db-payments-stage | pg-payments-stage | PostgreSQL 16.3 (container c-pg-stage) | payments | vm-pay-stage-01 | stage | 6 GB |

## Flowrun (low-code scenario runtime) — synced from the Flowrun source
**Instances**
| id | name | env | Flowrun version | URL | runs-on | scenarios | health |
|---|---|---|---|---|---|---|---|
| fr-dev | flowrun-dev | dev | 4.2.1 | https://flows-dev.tidewater.internal (no route, VPN only) | vm-lowcode-dev-01 | 4 | up |
| fr-stage | flowrun-stage | stage | 4.2.1 | https://flows-stage.tidewater.internal | vm-lowcode-01 | 3 | **down since 12:40** |
| fr-prod | flowrun-prod | prod | 4.1.7 | https://flows.tidewater.internal | vm-lowcode-01 | 3 | up |

**Scenarios** (a scenario is deployed per environment with its own version)
| id | name | dev | stage | prod | trigger | owner | last runs | linked |
|---|---|---|---|---|---|---|---|---|
| sc-sepa | sepa-payout-export | v14 | **v13 — failed 12:40 "PSP sandbox timeout after 3 retries" (run #8811, 6 m 02 s)** | v12 — ok 13:00, 2 m 14 s (run #8812) | cron `0 13 * * *` | Mara | 3 today | PAY-231, page *SEPA payout retry design* |
| sc-invoice | invoice-sync | v7 | v7 | v7 — ok 14:00, 41 s | cron hourly | Lena | 14 today | — |
| sc-refund | refund-reconciliation | v3 | — | — | manual | Mara | 1 today (dev, ok) | PAY-228 |
| sc-customs | customs-docs-fetch | v21 | v21 | v21 — ok 13:45, 12 s | webhook | Tomasz | 27 today | page *Payments architecture overview* |

Run log excerpt for sc-sepa stage run #8811:
```
12:40:02 start sepa-payout-export v13 (stage) batch=2026-08-22
12:40:04 fetch pending payouts … 412 rows
12:40:05 POST psp-sandbox/batches … 503 TEMP_UNAVAILABLE (attempt 1/3)
12:40:35 POST psp-sandbox/batches … 503 TEMP_UNAVAILABLE (attempt 2/3)
12:41:35 POST psp-sandbox/batches … timeout 60 s (attempt 3/3)
12:46:04 FAILED: PSP sandbox timeout after 3 retries
```
Promote flow to show: **sepa-payout-export v13 stage → prod** — review dialog lists: version diff (v12 → v13: "retry with backoff on TEMP_UNAVAILABLE; jitter ±10 %"), linked ticket PAY-231 (In Progress), stage status (last run failed — warn), prod instance version 4.1.7 (compatible), *Promote* / *Cancel*. Optimistic result: prod column flips to v13, run #8813 queued, toast "Promoted sepa-payout-export v13 to prod", history line.

## Monitoring — synced from Uptime Kuma
| id | monitor | type | target | state | response | uptime 30 d | asset | notes |
|---|---|---|---|---|---|---|---|---|
| m-payouts | payouts /health | HTTP | https://payouts.tidewater.internal/health | up | 120 ms | 99.98 % | r-payouts, svc-payout | |
| m-ledger | ledger /health | HTTP | https://ledger.tidewater.internal/health | up | 95 ms | 99.99 % | r-ledger, svc-ledger | |
| m-dashboard | dashboard | HTTP | https://dashboard.tidewater.internal | **warning** | **8 400 ms** (threshold 2 000) | 99.6 % | r-dashboard, svc-dashboard | PAY-240 |
| m-flows-stage | flows-stage | HTTP | https://flows-stage.tidewater.internal | **down since 12:40** (1 h 52 m) | — | 97.1 % | r-flows-stage, fr-stage | 3 notifications sent |
| m-flows | flows | HTTP | https://flows.tidewater.internal | up | 210 ms | 99.95 % | r-flows, fr-prod | |
| m-worker | payout-worker container | Docker | c-worker-prod | **down since 11:58** | — | 98.4 % | c-worker-prod | exited 137 |
| m-pg | vm-db-01:5432 | TCP | 10.20.4.20:5432 | up | 3 ms | 100 % | db-payments, vm-db-01 | |
| m-vm-pay | vm-pay-01 ping | Ping | 10.20.4.11 | up | 0.4 ms | 100 % | vm-pay-01 | |
| m-cert | payouts cert | Cert expiry | payouts.tidewater.internal | **warning — 9 days** | — | — | r-payouts | notify at 14 / 7 / 1 days |
| m-pg-stage | pg-payments-stage | TCP | 10.20.5.11:5432 | up | 2 ms | 99.2 % | db-payments-stage, c-pg-stage | |

Status page **Payments** (public to the company): payouts, ledger, dashboard, flows. Check interval 60 s; 24 h bar = 1 440 checks (render as a 48-segment bar).

**New inbox items (newest first, merge with the existing six):**
| when | item | actions |
|---|---|---|
| today 12:40 | flows-stage is **down** (HTTP, 1 h 52 m) — affects flowrun-stage, scenario sepa-payout-export v13 | Open asset · Ack · Snooze |
| today 11:58 | payout-worker container **down** on vm-pay-01 (exited 137) | Open asset · Restart container · Ack |
| snoozed | payouts certificate expires in 9 days (2026-08-31) | returns 2026-08-29 |

## Sources (add to the four existing)
| id | type | display name | URL | auth | sync |
|---|---|---|---|---|---|
| uptimekuma | Uptime Kuma | status.tidewater.internal | https://status.tidewater.internal | API key | synced 1 min ago · 10 monitors · writes: create monitor, pause, ack |
| flowrun | Flowrun | flows.tidewater.internal | https://flows.tidewater.internal/api | PAT | synced 3 min ago · 3 instances · 4 scenarios · writes: run, promote |

Available but not configured (shown in *Add source* under "Assets"): **Proxmox VE** (imports VMs), **Docker host** (imports containers via socket/TCP), **Traefik** (imports routes). Manual assets are the default; imports create assets that stay editable.

## Context membership (ASSETS tile per room)
- **PAY-200 Payout reliability**: vm-pay-01, c-payout-prod, c-worker-prod (down), db-payments, r-payouts (cert 9 d), monitors m-payouts, m-worker, m-cert.
- **PAY-231**: c-payout-stage (1.9.0-rc1), vm-pay-stage-01, fr-stage (down), sc-sepa (stage v13 failed), r-payouts-stage, m-flows-stage.
- **Staging DB configuration** (ad-hoc): db-payments-stage, c-pg-stage, vm-pay-stage-01, m-pg-stage.

## Suggested asset links (unconfirmed)
- r-dashboard → svc-dashboard — *host name matches service name*
- c-payout-stage → build #1188 — *image tag 1.9.0-rc1 matches the build's candidate tag*
- sc-refund → PAY-228 — *key in scenario description*
- vm-lowcode-01 → page *Payments architecture overview* — *mentions "lowcode-01" twice*

## Smart lists (add)
| list | count | note |
|---|---|---|
| Assets with open alerts in my contexts | 3 | c-worker-prod, fr-stage, r-payouts (cert) — joins assets ↔ monitors ↔ contexts |
| Drifted between stage and prod | 2 | payout-service 1.9.0-rc1 vs 1.8.2 · sepa-payout-export v13 vs v12 |
| Certificates expiring < 30 days | 1 | r-payouts |

## Search examples
- `payout` → tickets PAY-231/240, PR #142, containers c-payout-prod/stage, svc-payout, route r-payouts, monitor m-payouts, scenario sc-sepa.
- `asset: stage` / `/assets env:stage` → everything with env = stage.
- `health:down` → c-worker-prod, fr-stage (via m-worker, m-flows-stage).
- Filter chips on asset results: type (VM · Container · Service · Route · DB · Runtime · Scenario), env (prod · stage · dev), health (up · warn · down), owner (@me, @tomasz, @lena).

## Change history examples
- vm-pay-01 — 2026-08-10 Tomasz: `last-patched` set · 2026-07-30 Mara: linked to PAY-200 · 2026-07-12 import from Proxmox.
- c-payout-stage — today 12:05 Flowrun/Docker sync: image tag 1.8.2 → 1.9.0-rc1 · today 12:06 suggestion: link to build #1188.
- sc-sepa — today 12:40 stage run #8811 failed · today 10:50 Mara deployed v13 to stage · yesterday 17:45 Mara deployed v14 to dev.
