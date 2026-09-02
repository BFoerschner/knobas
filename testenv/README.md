# `testenv` — the knobas test environment

A small fake company on a laptop: the Tidewater Freight fixture served by real
software where we can self-host it, and by our own faithful mock where we
cannot.

```sh
cd testenv
docker compose up -d --build   # gitea, uptime-kuma, mockd
./seed                         # the Tidewater content, idempotent
```

Two layers, and the difference is the point:

| Layer | What | Why |
|---|---|---|
| **Real products** | Gitea, Uptime Kuma v2 | Cheap to self-host, so the adapters are certified against the actual software rather than against our idea of it. |
| **mockd** | Jira Data Center v2, TeamCity REST | Jira and TeamCity are not cheap to self-host (see the profiles below). `crates/knobas-mockd` serves the *same* Tidewater fixture, so both layers tell one story (design §14a). |

## Ports

Fixed by the M1 interfaces doc §5. Everything binds `127.0.0.1` only — this
environment ships developer-grade credentials and must never be reachable
off-host. `./check-ports.sh` asserts the compose file against this table and CI
runs it, so the table and the file cannot drift.

| Port | Service | Profile |
|---|---|---|
| 3000 | Gitea | default |
| 3001 | Uptime Kuma v2 | default |
| 8200 | mockd — health + `/__mock/*` admin API | default |
| 8210 | mockd — Jira Data Center REST v2 | default |
| 8211 | **reserved** — Confluence DC mock (M3) | *bound by nothing* |
| 8212 | mockd — TeamCity REST | default |
| 8213 | **reserved** — Flowrun stub (M4) | *bound by nothing* |
| 8111 | real TeamCity server | `--profile real-teamcity` |
| 8080 | real Jira Software | `--profile real-atlassian` |
| 8090 | real Confluence | `--profile real-atlassian` |

8211 and 8213 are reserved on purpose and bound by nothing — in the compose
file *and* in the `mockd` binary. They were not forgotten.

## Credentials

All developer-grade, all in the clear on purpose — there is nothing here worth
protecting, and a seed that prompts is a seed nobody runs.

| Where | User | Password |
|---|---|---|
| Gitea admin | `knobas` | `knobas-dev` |
| Gitea people (`mara.lindqvist`, …) | fixture username | `tidewater-dev` |
| Uptime Kuma admin | `knobas` | `knobas-dev` |
| TeamCity admin (`--profile real-teamcity`) | `knobas` | `knobas-dev` |
| Jira / Confluence admin (`--profile real-atlassian`) | `knobas` | `knobas-dev` |
| mockd | any non-empty `Bearer`/`Basic` token | — |

`./seed` writes two files, both git-ignored:

- **`kuma-api-key`** — the Uptime Kuma API key, for `/metrics`.
- **`seed-state.json`** — the fixture→reality id mapping (see *What the seed
  cannot reproduce* below).

## Environment variables for adapter test suites

`./seed` prints these at the end, ready to paste or `eval`:

```sh
export KNOBAS_GITEA_URL=http://127.0.0.1:3000
export KNOBAS_GITEA_TOKEN=<the seed's admin token, also in seed-state.json>
export KNOBAS_GITEA_OWNER=tidewater
export KNOBAS_GITEA_REPO=payout-service
```

`eval "$(./seed --env)"` re-prints them without re-seeding.

## One environment, one owner at a time

The repo's standing rule is that a worktree has exactly one owner. **This
Docker environment is not covered by it**: there is one of it, shared by every
worktree on the machine, and two agents seeding or running live suites against
it at the same time will break each other. Three ways:

- **The seed re-mints the Gitea token.** `seed-gitea.sh` reuses the token
  recorded in `seed-state.json` while it still authenticates, but
  `seed-state.json` is git-ignored and therefore lives *inside one worktree*. A
  second agent seeding from its own worktree finds no recorded token, deletes
  every existing `knobas-seed` token and mints a fresh one — and the first
  agent's in-flight run starts answering 401. This happened during PR #130's
  review. The same mechanism leaves **every other worktree's `seed-state.json`
  holding a token that is already dead**, so a 401 from `eval "$(./seed --env)"`
  usually means "another tree seeded last", not "the environment is broken";
  re-running `./seed-gitea.sh` from *your* tree fixes it and rotates the token
  again, which is the same collision from the other side.
- **The Gitea live suite sweeps.** `tests/live_gitea.rs` removes any leftover
  `knobas-` branch in `payout-service` before it starts, which is how a killed
  run heals (issue #143). It cannot tell a sibling's live branch from a corpse.
- **`just gitea-live-capped` reconfigures the shared container.** It recreates
  Gitea with `MAX_RESPONSE_ITEMS: 1` and uncaps it again from a trap, so for
  the length of that run every other reader of this environment is talking to a
  server that answers one record per page. A concurrent `just gitea-live` fails
  on missing records, which reads as an adapter defect and is not one.

So: **claim the environment before running `./seed`, `just gitea-live` or
`just gitea-live-capped`, and say when you release it.** The failure mode is a
mid-run 401 or a vanished branch, neither of which reads as "somebody else is
in here". Nothing in the tooling enforces this, and closing it properly would
mean a lock the tooling does not have.

## What the seed **cannot** reproduce — read this before writing assertions

`fixtures/tidewater/work.json` records commit shas like `c90d11` and pull
request numbers like `142`. **Gitea and git assign their own.** The seed cannot
make a real git commit hash come out as a chosen six-character prefix, and it
cannot retroactively renumber a pull request.

Consequences for a live test suite:

- **Assert by form and by title, never by literal id.** `gitea:tidewater/payout-service@<sha40>`
  is the key *form* the §4.2 contract fixes; the sha inside it is whatever git
  produced. A test that hardcodes `c90d11` is testing the fixture, not the
  adapter.
- **PR numbers are best-effort.** `SEED_EXACT_PR_NUMBERS=1` (the default) burns
  the preceding issue indices so PR #142 really is #142 — Gitea allocates issue
  and pull indices from one per-repo counter, so this is possible, and it is
  worth the ~140 extra calls because every mockup and link test was written
  against those numbers. Set `SEED_EXACT_PR_NUMBERS=0` for a fast seed; the PRs
  then land at 1, 2, … and the mapping goes to `seed-state.json`.
- **`seed-state.json` is the bridge.** It maps every fixture short sha to the
  real 40-character sha, and every fixture PR number to the real index. Read it
  rather than guessing.

Nothing in the §4.2 key forms depends on the fixture's abbreviations.

## Opt-in profiles

Off by default because they are expensive. Approximate costs are in `.env`.

```sh
docker compose --profile real-teamcity  up -d teamcity teamcity-agent   # ~3.5 GB pull, ~10 GB on disk
docker compose --profile real-atlassian up -d jira-db jira confluence-db confluence
```

### TeamCity, with one build agent

The server and one agent (`jetbrains/teamcity-agent`, no published port,
`SERVER_URL=http://teamcity:8111`). The first start is a browser wizard —
data directory, database choice, licence agreement, administrator account —
and until #264 the README said no script could drive it. `seed-teamcity.sh`
drives it now, the way `seed-atlassian.sh` drives Jira's: through the
wizard's own `/mnt/do/*` commands and `createAdminSubmit.html`, which are not
an API, so the script **refuses any image digest it was not derived on** and
the fix for that refusal is to re-derive the sequence (the recipe is in the
script's header). About forty seconds from empty volumes, unattended:

```sh
docker compose --profile real-teamcity up -d teamcity teamcity-agent
./seed --teamcity           # or ./seed-teamcity.sh
eval "$(./seed --env)"      # adds KNOBAS_TEAMCITY_URL and KNOBAS_TEAMCITY_TOKEN
```

The database is the internal HSQLDB (evaluation-grade, and this environment's
lifetime is `down -v`, so that is the right grade), the administrator is
`knobas` / `knobas-dev`, the access token `knobas-seed` goes to
`seed-state.json` and is reused while it authenticates, and the agent
`knobas-agent` is authorised over REST. Re-running against a set-up server is
a no-op. `openssl` is needed besides docker, curl and jq: the wizard's
administrator form RSA-encrypts the password in the browser, and the script
does the same with openssl.

Note that the repo-root `.env.example` points `just teamcity-live` at
JetBrains' public guest instance; `./seed --env` printing `KNOBAS_TEAMCITY_*`
does not change that default.

A running server is also the only way to obtain `/app/rest/swagger.json`; see
`specs/README.md`'s blocker and `specs/fetch.sh --teamcity`.

### Jira and Confluence, end to end

Two containers plus a PostgreSQL each (~700 MB / ~800 MB, and Jira wants ~4 GB
of RAM). **The databases are not optional**: Jira 10 removed the embedded H2
engine, so without them the setup wizard stops at its database step.

Both are then set up unattended, in about three minutes from empty volumes:

```sh
# 10-user, 3-hour Data Center keys, free and needing no my.atlassian.com
# account. Atlassian ended self-service 30-day DC trials on 2026-03-30, so
# these are the only free licences left:
#   https://developer.atlassian.com/platform/marketplace/timebomb-licenses-for-testing-server-apps/
export JIRA_LICENSE_KEY='AAAB...' CONFLUENCE_LICENSE_KEY='AAAB...'

docker compose --profile real-atlassian up -d jira-db jira confluence-db confluence
./seed --atlassian          # or ./seed-atlassian.sh jira|confluence
eval "$(./seed --env)"      # adds KNOBAS_JIRA_* and KNOBAS_CONFLUENCE_*
```

The admin account is `knobas` / `knobas-dev` on both, and re-running the seed
against a set-up instance is a no-op.

Put the keys in your **shell**, not in `.env` — that file is tracked.
`CONFLUENCE_LICENSE_KEY` must be set when the container *first starts*, since
Confluence reads it at first-time setup; Jira's goes in through the wizard and
can be exported later.

**Three hours is the shape of this environment.** The licence expires three
hours after it is applied, and restarting the container does not reset it. So
this is stand up → run what needs a real instance → `docker compose
--profile real-atlassian down -v`, not a long-lived environment like Gitea's.

The versions are pinned to what a timebomb key actually starts rather than to
the newest release — Jira 11.x is reported to reject it. `seed-atlassian.sh`
refuses to run against any other image digest, because the wizard endpoints it
drives are not an API and change between versions.

Real Confluence is here in M1 although Confluence is M3's target, because
Atlassian publishes **no machine-readable Confluence DC spec at all** — the
running container is the only contract there will ever be.

## The capped-Gitea overlay

`docker-compose.capped.yml` recreates the Gitea service with
`[api] MAX_RESPONSE_ITEMS = 1`, so every listing answers **fewer** records than
the `limit=50` the Gitea adapter asks for. It exists for one test —
`crates/knobas-source-gitea/tests/live_gitea_capped.rs`, run by
`just gitea-live-capped` — which certifies that the adapter's paged walks end on
an *empty* page and not on a *short* one (issue #81, live-certified by #115).
The default compose file cannot certify that: every corpus the seed creates fits
in one page of 50, so both terminations agree on every request.

Two things about it are not obvious:

- **Seed first, uncapped.** `seed-gitea.sh` decides what already exists by
  reading listings with `limit=50` and no paging. Capped to one record,
  `commit_exists` sees only a branch's newest commit, judges the rest missing,
  re-POSTs a file that is already there and dies on the 422. `just
  gitea-live-capped` seeds against the default file and applies the overlay
  afterwards; the container is recreated over the same volume, so the corpus
  survives.
- **The cap is sticky, and the default file is what unsticks it.** Gitea's
  `environment-to-ini` only ever *writes* `/data/gitea/conf/app.ini`; dropping
  the variable does not remove the line, so a plain `docker compose up` after a
  capped run comes back on a container with no such variable and a still-capped
  server. That is why `docker-compose.yml` pins
  `GITEA__api__MAX_RESPONSE_ITEMS: "50"` — Gitea's own default, and
  `client::PAGE_SIZE` — explicitly rather than leaving it implicit.

## Image pinning

No `:latest` anywhere in the compose file. `.env` holds one
`NAME=repo@sha256:…` line per image and is **committed**: it contains public
image digests and no secrets. The compose file interpolates them with a failing
default, so a missing pin stops `docker compose config`, not `docker compose up`
three minutes later.

```sh
./pin-images.sh   # re-resolve the tags in the script to digests, rewrite .env
git diff .env     # review it: a digest change is an environment change
```

`pin-images.sh` resolves digests with `docker buildx imagetools inspect`, which
reads the registry manifest and **downloads nothing**. Pinning by `docker pull`
instead would cost ~13 GB, most of it the TeamCity image that the whole point of
`profiles:` is to keep off the disk. The two agree: for a multi-arch tag both
report the digest of the manifest index.

The two build stages of `mockd.Dockerfile` are pinned the same way — an
unpinned `rust:1-slim` would make the mockd container unreproducible.
`rust-toolchain.toml` still decides the compiler.

## Network access

The environment runs offline once the images are pulled, with **one
exception**: `kuma-seed` runs `npm i socket.io-client@4` into a scratch prefix
at seed time, because Uptime Kuma v2 has no REST API for configuration and the
client has to come from somewhere.

## Monitors

`monitors.json` is a **baseline of this stream's own**, not fixture content:
`fixtures/tidewater/work.json` contains no monitors, because assets and
monitors are M4 (interfaces §1). The four entries point at containers on the
compose network, so they are genuinely up rather than four permanent outages.
**M4 carry-over:** fold this file into the asset fixture when assets land.

Verify through the channel the adapter will actually use — `/metrics`, not the
UI:

```sh
curl -fsS -u ":$(cat kuma-api-key)" http://127.0.0.1:3001/metrics | grep monitor_status
```

Uptime Kuma v2 prunes raw heartbeats to ~24 h, **absence of a metric means
*unknown* rather than down**, and a response time of `-1` is a sentinel. The
seed asserts presence, never a particular value, and never waits for heartbeat
history.

## Scripts

| Script | Does |
|---|---|
| `./seed` | Everything below, in order. Idempotent — re-running is a no-op that exits 0. |
| `./seed-gitea.sh` | Org, users, repos, branches, commits, PRs, comments, reviews. |
| `./seed-kuma.sh` | Kuma admin account, monitors, API key. |
| `./seed-atlassian.sh` | The real Jira and Confluence containers' setup wizards, unattended (`--profile real-atlassian`). |
| `./seed-teamcity.sh` | The real TeamCity container's first start, an access token and one authorised agent (`--profile real-teamcity`). |
| `./pin-images.sh` | Re-resolve image tags to digests into `.env`. |
| `./check-ports.sh` | Assert the compose file against the §5 port table, default profile, opt-in profiles and the capped overlay. Starts nothing. |
| `./reset` | `down -v` every profile, and delete the seed's outputs. |

## mockd's documented deviations

mockd is deliberately **stricter** than the real products in several places, so
that an adapter which passes here passes there — never the reverse. The list is
maintained in one place and must not be copied:

> **`crates/knobas-mockd/src/lib.rs`**, the *Documented deviations* section of
> the module docs.

Read it before concluding that mockd is wrong.
