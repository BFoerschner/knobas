# `testenv` — the knobas test environment

A small fake company on a laptop: the Tidewater Freight fixture served by real
software (Gitea and Uptime Kuma always, TeamCity, Jira and Confluence behind
opt-in profiles) and, until the live suites replace it, by our own deprecated
mock.

```sh
cd testenv
docker compose up -d --build   # gitea, uptime-kuma, mockd
./seed                         # the Tidewater content, idempotent
```

**`./seed` needs `hetzner/hosts.env`** since 2026-09-06. Uptime Kuma's monitor
list is now the real estate — a ping per Hetzner server among them — and those
servers' IPs live in that gitignored file. `./seed-kuma.sh` refuses without it
rather than seeding a Kuma missing half the estate. `./seed` runs Gitea first,
so on a tree that has never run `./hetzner/provision.sh` the Gitea half
succeeds, the Kuma half refuses, and the run exits non-zero **before printing
the adapter env vars** — which is the part that bites. `./seed --env` prints
them for the Gitea half alone. See "Monitors" below for why, and
`hetzner/README.md` for the provisioning.

Two layers, and the difference is the point:

| Layer | What | Why |
|---|---|---|
| **Real products** | Gitea, Uptime Kuma v2 | Cheap to self-host, so the adapters are certified against the actual software rather than against our idea of it. |
| **mockd** (deprecated) | Jira Data Center v2, TeamCity REST | Built while Jira and TeamCity were "not cheap to self-host". **Deprecated 2026-09-03 (ADR-0013).** A mock certifies nothing, the real containers behind the profiles below are the witness for every adapter and every write path, and mockd gets nothing new; its tests stay until the live suites assert the same things, then it is deleted. `crates/knobas-mockd` still serves the *same* Tidewater fixture, so both layers tell one story while it lasts (design §14a). |

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
| 8212 | mockd — TeamCity REST | default |
| 8111 | real TeamCity server | `--profile real-teamcity` |
| 8080 | real Jira Software | `--profile real-atlassian` |
| 8090 | real Confluence | `--profile real-atlassian` |

One host port is used here that is **not** in that table and must not be:
`127.0.0.1:8299`, the canary (below). It is a host socket a helper script
binds, not a published container port, so `check-ports.sh` has nothing to
assert about it.

Two ports were once reserved here and both are free now. 8211, reserved for
a Confluence mock until 2026-09-03, is unreserved by ADR-0013; the real
Confluence on 8090 is the witness. 8213, reserved for a low-code-runtime
stub until 2026-09-06, is unreserved because that feature left the plan
(roadmap §2 M4).

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

`eval "$(./seed --env)"` re-prints them without re-seeding. Once `./seed-kuma.sh`
has run it also prints Uptime Kuma's four:

```sh
export KNOBAS_KUMA_URL=http://127.0.0.1:3001
export KNOBAS_KUMA_API_KEY=<the seed's API key, also in kuma-api-key>
export KNOBAS_KUMA_USER=knobas
export KNOBAS_KUMA_PASSWORD=knobas-dev
```

The account is the admin pair from the credentials table above, and it is
printed because Uptime Kuma needs **two** credentials to be exercised fully
(issue #452): the API key opens `/metrics` and nothing else, and pausing a
monitor is a socket.io login only an account can make. Printed rather than
defaulted in the suite, for `_require-live-env`'s reason — a suite that fell
back to `knobas`/`knobas-dev` would run against a Kuma whose admin is something
else and report the login failure as a broken adapter.

`eval "$(./seed --env-kuma)"` prints **only** those four, and needs no
`seed-state.json` — which is Gitea's, and re-minting it is how one worktree's
seed 401s another's live run. `just kuma-live` uses that form for exactly that
reason. Once `./seed
--teamcity` has run it also prints `KNOBAS_TEAMCITY_URL` and
`KNOBAS_TEAMCITY_TOKEN`; the seeded TeamCity suite additionally reads
`seed-state.json` for the fixture-number-to-id map, from
`testenv/seed-state.json` relative to the crate unless
`KNOBAS_TEAMCITY_SEED_STATE` names another path.

## On Hetzner instead of the laptop

Since 2026-09-06 the three opt-in real products run on Hetzner servers, one
product each, and the Atlassian pair is kept up between runs with its timebomb
licences renewed on a timer. `hetzner/README.md` has the whole of it; the short
form is `source hetzner/env` and `./hetzner/tunnel up` before any seed or `just
*-live` recipe.

One thing in this directory does change with them: **Uptime Kuma's monitors are
the estate**, so `./seed-kuma.sh` reads `hetzner/hosts.env` for the three
servers' IPs and refuses without it, and three of the eight monitors check the
products through the tunnel's forwards and go red when the tunnel does. See
"Monitors" below. Nothing else here knows about the servers.

## The live recipes

Every live suite has a `just` recipe, and the recipe is how it is run — a suite
with no recipe is one nothing runs, which is how
`crates/knobas-app/tests/start_work_live.rs` came to be carrying an assertion no
implementation could fail (issues #347, #350). Each recipe's own header in the
`justfile` says what it certifies and what it writes; this table says which
variables gate it and where those come from.

| Recipe | Suite | Gated on | From |
|---|---|---|---|
| `just gitea-live` | `knobas-source-gitea` / `live_gitea` — the adapter against the shapes interfaces §4.2 fixes | `KNOBAS_GITEA_URL`, `KNOBAS_GITEA_TOKEN` | `./seed-gitea.sh` then `eval "$(./seed --env)"` (the recipe does both) |
| `just gitea-live-capped` | `live_gitea_capped` — paged walks against a server capped to one record | as above | as above |
| `just start-work-live` | `knobas-app` / `start_work_live` — the start-work flow over this Gitea: ticket → branch → PR → link → status and back, and then that pull request on the standup digest (M3.3's digest witness for Gitea). Half of it is a mock, so it is **not** M2 exit criterion 1's certificate; the recipe's header says which half | as above | as above |
| `just teamcity-live` | `knobas-source-teamcity` / `live_teamcity` — the adapter against the **public JetBrains** instance, read-only | `KNOBAS_TEAMCITY_URL` | the repo-root `.env`: `cp .env.example .env` |
| `just teamcity-live-seeded` | two suites: `knobas-source-teamcity` / `live_teamcity_seeded` — the adapter against **our** seeded TeamCity — and then `knobas-app` / `teamcity_seeded_live`, read-only, which is M3.3's digest witness for this source (a seeded build the mirror attributes to the reader is on the digest for the day it ran) | `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` | `./seed --teamcity` then `eval "$(./seed --env)"` |
| `just kuma-live` | three suites: `knobas-source-kuma` / `live_kuma` — the Uptime Kuma adapter against **this** container: the estate mirrored, an idle poll that emits nothing, a monitor really deleted and reported gone, a wrong API key's 401, and the write half (#452) — a scratch monitor paused through `pause_monitor`, gone from the next poll, resumed back — then `knobas-app` / `kuma_write_live`, the same write half one seam higher: the pause queued through `submit_write`, settled, and read back out of `sync.item` — and then `knobas-app` / `alert_chain_live`, **M4.1's exit witness**: the canary released, the alert in the inbox because a context holds its asset, acked, the port rebound, the alert closed. The recipe binds the canary first and rebinds it from a trap | `KNOBAS_KUMA_URL`, `KNOBAS_KUMA_API_KEY`, `KNOBAS_KUMA_USER`, `KNOBAS_KUMA_PASSWORD` | `./seed-kuma.sh` then `eval "$(./seed --env-kuma)"` (the recipe does both) |
| `just atlassian-live` | four suites across Jira, Confluence and the app | `KNOBAS_JIRA_URL`/`USER`/`PASSWORD`, `KNOBAS_CONFLUENCE_URL`/`USER`/`PASSWORD` | the recipe seeds the pair and evals `./seed --env` itself |

**A recipe with nothing to run against fails; it does not pass quietly.** Each
one calls the `justfile`'s `_require-live-env` guard on the variables above
before it invokes cargo, and stops naming the ones that are missing and where
they come from. This is not hypothetical: `just teamcity-live` reported *"12
passed"* with every one of those twelve tests skipped, because there was no
`.env`, `KNOBAS_TEAMCITY_URL` was unset, and `live_or_skip!` returns early —
which libtest counts as a **pass**, so there is no skip total in cargo's output
to notice (issue #351). The guard checks the variables rather than the skips for
exactly that reason. It says nothing about an individual test: a suite whose
variables are all present may still skip one for a reason of its own and pass.

**What the guard does not claim: a variable that is set is not a server that
answers.** It closes the silent hole — the unset variable nothing complains
about — and nothing more. A wrong URL or a dead token still fails inside the
suite, loudly, as a connection error or a 401, which is where it belongs.

`just teamcity-live` reads the **repo-root `.env`** and points at a server that
is not ours; a green from it is not a statement about the container in this
directory, in either direction. That is `teamcity-live-seeded`.

## One environment, one owner at a time

The repo's standing rule is that a worktree has exactly one owner. **This
Docker environment is not covered by it**: there is one of it, shared by every
worktree on the machine, and two agents seeding or running live suites against
it at the same time will break each other. Four ways:

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
- **`./seed-kuma.sh` deletes every monitor `monitors.json` does not name**
  (since 2026-09-06, #441). That is what makes the seed idempotent, and it is
  also a sweep: a monitor a sibling added by hand, or that knobas itself
  created through the Kuma source's *Create monitor for this asset* (#453,
  landed 2026-09-07), is gone on anyone's next `./seed`, silently and with no
  way back. Add a monitor that has to survive to `monitors.json`, not to the
  UI.

- **`just kuma-live` re-mints the API key** when the worktree it runs from has
  no `kuma-api-key` — Kuma hands a key's clear text out once, so a key that
  exists in the instance with no copy on the host is replaced. A second agent
  running it mid-run makes the first agent's `/metrics` requests answer 401,
  which reads as a credential defect and is not one. It also adds and deletes
  four monitors of its own -- `knobas-live-scratch` and `knobas-write-scratch`
  through `kuma-monitor.sh`, `knobas-live-created` and `knobas-write-created`
  through knobas' own create (#453) -- which is why the sweep above matters: a
  killed run's leftovers are removed by the next `./seed-kuma.sh`.

So: **claim the environment before running `./seed`, `just gitea-live`,
`just gitea-live-capped`, `just start-work-live` or `just kuma-live`, and say
when you release it.** `start-work-live` seeds too, and its branches carry the same `knobas-`
prefix `live_gitea.rs` sweeps, so a concurrent `just gitea-live` deletes the
branch out from under it. The failure mode is a
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

The real TeamCity (`./seed --teamcity`) has the same class of gap, in its own
places:

- **Build ids are the server's.** The fixture (and mockd) spell build id and
  build number as the same value, `412`; a real server numbers builds by a
  per-configuration counter the seed sets, so the **numbers** come out as the
  fixture's, but the **ids** are 1, 2, 3 … in the order the builds were
  queued. `seed-state.json` maps fixture number to real id under
  `teamcity.builds`. Assert `number`, look `id` up.
- **Timestamps are when the build actually ran.** The fixture's `when`
  (`2026-08-22T10:10:00Z`) and `duration` (`4 m 12 s`) cannot be set on a
  real build: `startDate` is the moment the agent took it and the failing
  integration-test build finishes in seconds. Assert that a finished build
  has a `finishDate` after its `startDate`, never a particular value.
- **The triggerer is `knobas`.** Every seeded build is queued through the
  seed's token, so `triggered.type` is `user` and the user is the seed's
  administrator -- not the fixture's `mara` for 1188 and not a VCS trigger for
  the two the fixture attributes to nobody.
- **The running build is not running unless you asked.** One agent cannot hold
  `Payout_Build` 1188 at step 3/5 forever; see *TeamCity, end to end*.

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

### TeamCity, end to end

`./seed --teamcity` does not stop at a set-up server: `seed-teamcity-builds.sh`
runs at its end and puts the fixture's build content in, so the real server
tells the story mockd and the demo profile tell.

```sh
docker compose --profile real-teamcity up -d teamcity teamcity-agent
./seed                       # Gitea first: the VCS roots point at it
./seed --teamcity            # wizard, token, agent, then the Tidewater builds
./seed --teamcity --running  # ... and hold Payout_Build 1188 at step 3/5
```

**What it creates**, derived from `fixtures/tidewater/work.json`'s `builds`
by the rules of `crates/knobas-mockd/src/tc_state.rs`, so the ids are the ones
the adapter's tests and the project census already expect:

| Fixture `cfg` | Project (id = name) | Build configuration name | Build | Outcome |
|---|---|---|---|---|
| `Ledger_Deploy_Staging` | `Ledger` | `Deploy Staging` | `412` on `main` | SUCCESS |
| `Payout_IntegrationTests` | `Payout` | `IntegrationTests` | `1187` on `feature/PAY-231-sepa-retry` | FAILURE, the fixture's `log` in the build log |
| `Payout_Build` | `Payout` | `Build` | `1188` on `feature/PAY-231-sepa-retry`, `--running` only | running, held at step 3/5 `cargo test` |

No configuration has a description: the fixture gives none, and the seed
invents none, exactly as mockd does.

**The default branch is queued as `<default>`.** The VCS root's branch
specification names `main` along with the feature branches, and a build
queued with an explicit `branchName: main` resolves against that
specification to a *logical* branch called `main` that is a different branch
from `<default>`: the server marks it `defaultBranch: false` and every REST
listing under its default filter hides it (found by #266's first hour against
this server). So a build whose fixture branch is `main` is queued with no
branch name at all; TeamCity shows it as `<default>` while it waits and as
`main` with `defaultBranch: true` once it runs, which is the branch the
fixture means. 1187 and 1188 are queued on their feature branch by name.

**The builds are real and VCS-backed.** Each project has one Git VCS root
(`Payout_PayoutService`, `Ledger_LedgerApi`) pointing at the seeded Gitea over
the compose network (`http://gitea:3000/tidewater/<repo>.git`, authenticated
with the seed's Gitea token from `seed-state.json`), with a branch
specification naming every branch the Gitea seed created in that repository.
The agent clones the repository and runs a command-line step, and the
server reports the fixture's branch as the build's `branchName`. Nothing
here fell back to a parameter-only build. #265's brief said "a VCS root per
configuration"; the two Payout configurations build the same repository, so
the root is per project and attached to both -- one object to keep in step
with Gitea instead of two identical ones.

**The steps reproduce the fixture's outcome and nothing more.**
`Ledger_Deploy_Staging` prints one line and exits 0; `Payout_IntegrationTests`
prints the fixture's `log` and exits 1. `Payout_Build` is the one place the
seed departs from the brief's "one command-line step per configuration": it
has five, the third named `cargo test`, which sleeps -- the fixture holds
the build at "step 3/5 `cargo test`", and five real steps are what make the
server say `Step 3/5` itself rather than a single step pretending to. The
fixture names no other step, so the others are `step 1` .. `step 5`.

**The running build is opt-in.** A build server cannot hold a build running
forever on one agent, so the seed always makes the two finished shapes and
starts `Payout_Build` 1188 only with `--running`. It then holds the agent for
up to four hours (the `cargo test` step's sleep). To end it sooner:

```sh
eval "$(./seed --env)"
id=$(jq -r '.teamcity.builds[] | select(.fixture_number==1188) | .real_id' seed-state.json)
curl -X POST -H "Authorization: Bearer $KNOBAS_TEAMCITY_TOKEN" -H 'Content-Type: application/json' \
     -d '{"comment":"released by hand","readdIntoQueue":false}' "$KNOBAS_TEAMCITY_URL/app/rest/builds/id:$id"
```

A cancelled build finishes with status `UNKNOWN`, which is TeamCity's own
shape for one (mockd transcribes it the same way); the agent takes a few
seconds to kill the step, so `state` still reads `running` right after the
POST. A finished 1188 is left alone by later runs, `--running` or not. To put
the server back in the plain-seed state -- the two finished builds and
nothing else, which is what a live suite should find -- delete it:

```sh
curl -X DELETE -H "Authorization: Bearer $KNOBAS_TEAMCITY_TOKEN" "$KNOBAS_TEAMCITY_URL/app/rest/builds/id:$id"
```

The next `./seed --teamcity` then rewrites `teamcity.builds` without 1188,
and `--running` starts it again with the counter reset, so it comes out as
1188 once more.

**Idempotent, like the Gitea seed.** Every project, VCS root, configuration
and build is read before it is created, and a configuration gets its steps
only while it has none; a finished build by number on its configuration is
never re-triggered, a queued or running one from an interrupted run is
waited for. A second `./seed --teamcity` creates nothing and exits 0. Build **numbers** come out as the fixture's because the seed
sets each configuration's build number counter before its first build --
the `SEED_EXACT_PR_NUMBERS` idea, without the burning. Build **ids**,
timestamps and the triggerer are the server's; *What the seed cannot
reproduce* above says what to assert instead, and `seed-state.json` carries
the number-to-id map:

```json
"teamcity": { "...": "...", "builds": [
  { "fixture_number": 412,  "build_type": "Ledger_Deploy_Staging",   "real_id": 2 },
  { "fixture_number": 1187, "build_type": "Payout_IntegrationTests", "real_id": 1 } ] }
```

Where the endpoints and field names came from is in the script's header:
the vendored `specs/teamcity.json` for every `/app/rest` call, and the
server's own forms for the three things that swagger does not enumerate
(the Git root's property names, the command-line runner's, and the
`settings/buildNumberCounter` resource).

#### The seeded live suite

`crates/knobas-source-teamcity/tests/live_teamcity_seeded.rs` is the
adapter's certification against this server, the way `live_gitea.rs` is
against the seeded Gitea: it asserts the seeded content by id, number, status
and branch (reading `seed-state.json` for the number-to-id map), runs the
contract battery over it -- clause 2 included, which the public instance
cannot be held to -- and watches the `sinceBuild` watermark move on a build it
queues and stand still afterwards. With the environment up and seeded as
above:

```sh
just teamcity-live-seeded      # evals ./seed --env, then the suite, serially
```

**It writes, and it takes it away again.** One test queues one build of
`Ledger_Deploy_Staging` through `POST /app/rest/buildQueue` (on
`fix/PAY-228-partial-refund-drift`, so the incremental query is witnessed on a
feature branch), waits for the agent to finish it, and then **deletes** it
(`DELETE /app/rest/builds/id:<id>`) when the test ends, passing or panicking
alike; the deletion is checked, not hoped for. That build takes the
configuration's next number -- 413 after a fresh seed -- and the number
counter is not wound back, which is harmless: the seed only sets a counter
while the build with the fixture's number does not exist. Before any test
that asserts an exact set, the suite **clears what a killed run left**: every
build whose id is not in `seed-state.json` is canceled if in flight and
deleted, so recovery from a run that died mid-way is "run the suite again",
never `testenv/reset`. After a green run the server holds exactly the seeded
builds again.

**One owner at a time**, exactly as for the Gitea suites (see *One
environment, one owner at a time*): the leftover clearing cannot tell a sibling's build
from a corpse, and the battery's clause 2 needs a server where nothing is
running. `./seed --teamcity --running` and this suite are therefore mutually
exclusive on one environment: the suite refuses to start while a seeded build
is in flight and says how to release it, and a build somebody else queues
mid-run is the one case no check can catch.

### Jira and Confluence, end to end

Two containers plus a PostgreSQL each (~700 MB / ~800 MB, and Jira wants ~4 GB
of RAM). **The databases are not optional**: Jira 10 removed the embedded H2
engine, so without them the setup wizard stops at its database step.

Both are then set up unattended, from empty volumes, in the minutes the
`atlassian-live` recipe header measures — **one product at a time, Jira
first**:

```sh
# 10-user, 3-hour Data Center keys, free, public, and needing no
# my.atlassian.com account. Atlassian ended self-service 30-day DC trials on
# 2026-03-30, so these are the only free licences left. The script pulls them
# off Atlassian's page and checks each decodes to the 10-user, 3-hour Data
# Center licence for its product; nothing to type or paste. Run it BEFORE
# either `up`, because Confluence reads its key at first start.
eval "$(./fetch-timebomb-keys.sh)"

# Jira gets the VM to itself until it is RUNNING; Confluence is not created
# before that. Two JVMs claiming their heaps at once on the 8 GB VM is what
# pushed Jira's post-wizard restart past its wait (issue #314), and that
# restart is the long pole: Jira writes its schema and re-initialises its
# whole plugin system before `/status` says RUNNING.
docker compose --profile real-atlassian up -d --wait jira-db jira
./seed-atlassian.sh jira
docker compose --profile real-atlassian up -d --wait confluence-db confluence
./seed-atlassian.sh confluence

./seed-atlassian-content.sh # the Tidewater content (next section)
eval "$(./seed --env)"      # adds KNOBAS_JIRA_* and KNOBAS_CONFLUENCE_*
```

`just atlassian-live` is all of that as one command, and on a machine with
headroom `./seed --atlassian` still walks both wizards and seeds the content
against a pair brought up together.

The admin account is `knobas` / `knobas-dev` on both, and re-running the seed
against a set-up instance is a no-op.

**What the waits print.** Jira's post-wizard wait for `RUNNING` is capped at
900 s, and at 600 s each: the wait for a product to answer `/status` with a
state its wizard can be driven from, and — Jira only — the wait for it to
actually *serve* that wizard, which is a separate question and lands about a
minute later. A fourth sits inside the wizard walk itself, capped at 300 s: a
product already serving a form can still answer a POST to it with a 500 while
it finishes warming, or take the connection and never answer at all. That last
cap is the one number here nothing has measured — no recorded run has made the
wait print at all — and the script's comment block says so rather than guessing
a wider one (issue #332). All four print what the product is showing — the
state, the form, the code it is answering the POST with, or that it is not
answering — and how far into the cap they are, every 30 s.

All four also bound the single request they make, so that one unanswered call
cannot outlive the cap around it: `POLL_TIMEOUT_S` (10 s) covers three of them
— a `/status` poll, or a read of the wizard page — and `WIZARD_POST_TIMEOUT_S`
(240 s) the fourth, a wizard POST, which is the step's actual work and not a
free GET. The two differ by more than an order of magnitude because a cut poll
costs a repeat and a cut POST could cost a wizard step applied twice; the
slowest POST measured is 46 s (issue #367). The timeout is also what sets how
often a stalled wait can speak, since nothing prints while a request is still
outstanding: three of the four keep the 30 s cadence above through a hang, and
the wizard POST manages one line — it is silent for 240 s, prints once, and
then fails. Bounded and diagnosed beats prompt and wrong here, which is the
trade that comment block argues.

A state that climbs is a slow start, one state repeated to the cap is a hang,
and `UNREACHABLE` throughout is a container to read `docker logs` for. A cap
that fires is a statement about the machine.

The keys live in your **shell**, not in `.env` — that file is tracked
(`./fetch-timebomb-keys.sh --write` drops them in the git-ignored
`.env.licences` if you want them to survive a new terminal).
`CONFLUENCE_LICENSE_KEY` must be set when the container *first starts*, since
Confluence reads it at first-time setup, and `seed-atlassian.sh` checks the
container's own environment for it; Jira's goes in through the wizard, and the
seed fetches it itself when it is unset.

**Three hours is the shape of this environment.** The licence expires three
hours after it is applied, and restarting the container does not reset it. So
this is stand up → run what needs a real instance → `docker compose
--profile real-atlassian down -v jira jira-db confluence confluence-db`, not a
long-lived environment like Gitea's. **Name the four services.** A
profile-scoped `down -v` with no service named takes the default profile's
containers and volumes with it too — Gitea and its seeded corpus included
(checked with `--dry-run` on Compose v5.1.2); naming them removes exactly the
pair's four volumes.

The versions are pinned to what a timebomb key actually starts rather than to
the newest release — Jira 11.x is reported to reject it. `seed-atlassian.sh`
refuses to run against any other image digest, because the wizard endpoints it
drives are not an API and change between versions.

Real Confluence is here in M1 although Confluence is M3's target, because
Atlassian publishes **no machine-readable Confluence DC spec at all** — the
running container is the only contract there will ever be.

### The Tidewater content, and `just atlassian-live`

`seed-atlassian-content.sh` puts `fixtures/tidewater/work.json` into the pair
`seed-atlassian.sh` set up — the same file the demo loader compiles in, so
there is no second copy of the dataset — and `just atlassian-live` runs the
whole window as one command:

```sh
just atlassian-live      # from the repo root; refuses while knobas-teamcity is up
```

which is: `fetch-timebomb-keys.sh` (before either `up`, since Confluence reads
its key at first start) → `up -d --wait jira-db jira` →
`seed-atlassian.sh jira` → `up -d --wait confluence-db confluence` →
`seed-atlassian.sh confluence` → `seed-atlassian-content.sh` →
`seed-atlassian-content.sh --verify` → every crate test gated on
`KNOBAS_JIRA_URL` / `KNOBAS_CONFLUENCE_URL` → `down -v` of the four Atlassian
services, from a trap, so the teardown runs when a step fails and on Ctrl-C.
**One product at a time, Jira first**, for the reason above: the two JVMs
starting together on the 8 GB VM is what made Jira's wait too tight under load
(#314), and Confluence's container is not created until Jira is `RUNNING` with
its REST answering. The recipe's header carries the measured wall clock of a
full run and where a new live suite's line goes: measured 2026-09-03, a full
run from empty volumes takes **between 311 s and 392 s** depending on what else
the machine is doing — in the 327 s one, 265 of them the two wizard walks and
the content seed, 22 the three live suites once compiled, the rest the
teardown. The three-hour window holds with hours of margin.

**The suite gated on `KNOBAS_CONFLUENCE_URL`** (issue #284) is
`crates/knobas-source-confluence/tests/live_confluence_seeded.rs`, and it is
the **only** witness that adapter has or will have: ADR-0013 refused a mockd
Confluence half because there is no machine-readable spec to build one from, so
every claim the crate makes about a response shape — that a content search
answers `_links.next` and no total, that `version.when` carries the instance's
UTC offset, that `children.comment` is where an expanded discussion lands — is
a claim this file re-makes against Confluence itself. It writes exactly one
thing: a **title**, on one seeded page, to witness that a renamed page keeps
its content id. It puts it back from a `Drop` that checks, and its leftover
clearing restores every seeded page's title from `seed-state.json`, so a
*killed* run is recovered by the next one.

**The two suites gated on `KNOBAS_JIRA_URL`** (issue #276) are
`crates/knobas-source-jira/tests/live_jira_seeded.rs` — the adapter: sync,
payload, cursor, the real 401 — and `crates/knobas-app/tests/atlassian_live.rs`
— the engine and the write queue: credential health end to end, and the three
write ops read back out of Jira. Both write, and both take back what they wrote
from a `Drop` that checks rather than assumes. What a *killed* run leaves is put
back by the next run of the **adapter's** suite, whose leftover clearing works
from `seed-state.json` rather than from anything a run remembers — a stray issue
and a stray comment deleted, the `knobas-live-suite` label removed, a status
moved back through the workflow — and prints what it put back. So after a green
run the server holds exactly the seeded corpus, and recovery from a dirty one is
"run the suite again". The one thing neither can undo is the `PAY` key counter:
the create leaves the project one key further on, so nothing may assume the
fixture's keys are the highest ones.

**It refuses while TeamCity is up.** Docker Desktop's VM here has 8 GB; Jira
wants ~4 GB, Confluence ~2 GB, plus a PostgreSQL each, and the seeded TeamCity
(server and agent, ~2.2 GB) does not fit beside them. The recipe prints the
`stop` to run — `docker compose --profile real-teamcity stop teamcity
teamcity-agent`, *stop* and never `down -v`, because those volumes hold the
seeded builds — and does not run it itself: the TeamCity environment is
somebody's (see *One environment, one owner at a time*). Gitea stays up
throughout; the pair does not need it.

**What the content seed creates.** In Jira, the fixture's five people as
users (`tidewater-dev`, six accounts on the ten-user key), the `PAY` and
`OPS` projects from Jira Software's *Basic software development* template
(`com.pyxis.greenhopper.jira:basic-software-development-template`, the key
is fixed in the script), every fixture issue **at its fixture key** with its
type, priority, assignee, estimate, description, status, Epic Name and Epic
Link, its comments and worklogs, and `blocked_by` as a *Blocks* link. In
Confluence, the `ENG` space (named *Engineering*; the fixture names only the
key), its five pages with the fixture's `##` sections as `<h2>`/`<p>` storage
format, their comments, and *Standup protocols* as the empty page the standup
flow will publish under. Only *SEPA payout retry design* has a body and a
comment in the fixture — the other four are titles, which is what makes them
the test that an empty page is still a page. **The tree is the seed's
decision**: the fixture names no ancestors, so four pages sit directly under
the space home page and *SEPA payout retry design* is nested one level deeper,
under *Payments architecture overview*. That nesting is not decoration — a
launcher hit's ancestor path over a flat space is a single segment, so the
separator and the outermost-first ordering would be witnessed on fixtures only
(#396). The ids Confluence assigns are recorded as `confluence.pages[]`, each
with the `parent_id` it was put under, with `confluence.space`,
`confluence.home_page_id` and `confluence.author` beside them; the Confluence
live suite reads all four. The
template's workflow is *Software Simplified Workflow for Project `<KEY>`*:
**To Do, In Progress, In Review, Done**, every transition available from
every status — exactly the fixture's four statuses, and the names the live
suites' transition tests use. The template's issue type scheme has no
*Story*, so the seed adds the global Story type to each project's scheme
over `PUT /rest/api/2/issuetypescheme/<id>`.

**Epic membership is in the Epic Link custom field, and only there.** `PAY`
and `OPS` are *classic* Data Center projects, so `fields.parent` — the
spelling a next-gen or a recent company-managed project uses — is **absent
from every issue**, the epic's children included. The relationship lives in
the "Epic Link" custom field, whose id is this instance's own and differs
between seeds of the same script (`customfield_10101`, `_10102` and `_10109`
on three of them), which is why the seed records it as
`jira.epic_link_field`. A Jira source configured without the adapter's
`epic_link_field` option therefore mirrors no epic membership at all from this
server. `knobas-mockd` serves both spellings, which is what hid that until
#276.

**Never send this Jira a wrong password.** Jira DC counts failed password
logins per account and, past the container's default of a few, answers `403
Basic Authentication Failure - Reason : AUTHENTICATION_DENIED` — to the
**correct** password as well, until an administrator clears the elevated
security check. A suite that draws its 401s from a wrong password locks
`knobas` out part-way through its own run and fails everything after it on a
cause none of those failures name. Take refusals from a **bearer token**
instead, which never reaches Seraph and counts against nothing; if it has
already happened, the quickest way out inside a window is
`docker exec knobas-jira-db psql -U jira -d jira -c "update
cwd_user_attributes set attribute_value='0' where attribute_name =
'login.currentFailedCount'"`, and the proper one is `down -v` and seed again.

**An unresolvable bearer token searches anonymously.** It is *not* a failed
login: `/rest/api/2/serverInfo`, `/myself` and `/issue/{key}/…` answer `401`
with no `X-Seraph-LoginReason` at all, and `/rest/api/2/search` answers **`200`
with `total: 0`**, because asking needs no permission. Only a wrong *password*
carries `X-Seraph-LoginReason: AUTHENTICATED_FAILED`, with Jira's HTML login
page for a body rather than the `errorMessages` envelope. `docs/contract.md`'s
promise of that header on "an invalid/absent Bearer" was written from the
documentation and is wrong for the product; `knobas-mockd`'s deviations 14–16
record all of it.

**The keys are the fixture's because the keys in front of them are burned.**
Jira allocates keys from a per-project counter no REST call sets, so the seed
bulk-creates placeholders (summary `(reserved)`, label `knobas-placeholder`)
up to the key before each fixture issue, creates the issue, asserts the key,
and deletes every placeholder at the end. That is why the fixture's keys are
reachable only in a project the seed created from empty: an issue deleted by
hand cannot come back at its key (the counter never rewinds), and the seed
says so and stops rather than seeding PAY-240 as PAY-241. The remedy is the
one the recipe ends with anyway: `down -v` of the four services, seed again.

**The admin username tests must configure.** Comments and worklogs are
authored by `knobas` — Jira's and Confluence's REST take no author on
either — and `seed-state.json` records it as `jira.author` and
`confluence.author`. An identity-dependent live test configures that username
(it is what `KNOBAS_JIRA_USER` / `KNOBAS_CONFLUENCE_USER` from `./seed --env`
already say); the fixture's people exist so that assignees are the fixture's,
but nothing is written as them.

**What it cannot reproduce**, as for Gitea: created and updated timestamps
(the server stamps now; worklogs carry the fixture's date at 09:00 UTC), the
fixture's `spent_week_m`, a page's `edited` date and author. The ids the
server assigns — issue ids, comment ids, worklog ids, page and page-comment
ids, the space home page, and the Epic Link custom field's id — go to
`seed-state.json` under `jira.issues`, `jira.epic_link_field` and
`confluence.pages`, keyed by fixture key and fixture page id, so a live
assertion looks a literal id up there and asserts everything else by key,
title and text. Should a fixture status ever be missing from the workflow,
`jira.unreachable_statuses` says which issue kept which status; today it is
empty.

**Idempotent, and it says what it skipped.** Every create is preceded by a
read — issues by key, comments and worklogs by text, pages by title in the
space, page comments by text — and the run ends with `done: created N,
skipped M`. Re-running against a half-seeded instance finishes it; against a
seeded one it creates nothing.

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

Three pins are **guarded**: the Jira and Confluence images (`seed-atlassian.sh`)
and the TeamCity server image (`seed-teamcity.sh`) are the ones a seed walks a
first-start wizard on, and each seed refuses any digest but the one its form
fields were read off (`VERIFIED_*_IMAGE` in the seed script). For those,
`pin-images.sh` writes the seed's digest and, when the tag has moved on (even a
patch tag does: `confluence:9.2.21` was retagged upstream between 2026-08-31
and 2026-09-02), prints
the drift with both digests and the guarding seed, exit 0 — the file is right
for the seeds as they are. `./pin-images.sh --move CONFLUENCE_IMAGE` takes the
new digest for that one variable; the seed then refuses its container until
its wizard walk is re-derived and its `VERIFIED_*_IMAGE` updated. The guard
values are read out of the seed scripts, not copied into `pin-images.sh`, so
the two cannot disagree. Everything else (Gitea, Kuma, node, the build images,
the TeamCity agent) resolves fresh on every run; the agent is pinned by the
server's version tag rather than `latest`, because an agent ahead of the server
is refused at registration (#267's finding, not re-run since). That tag is a
hand-kept copy of the server's version, the one duplicate here that nothing
reads for you; `--move TEAMCITY_IMAGE` names it as the thing to bump in the
same change. A held pin says so on its own comment line in `.env`. `--out PATH`
writes elsewhere, to compare.

The two build stages of `mockd.Dockerfile` are pinned the same way — an
unpinned `rust:1-slim` would make the mockd container unreproducible.
`rust-toolchain.toml` still decides the compiler.

## Network access

The environment's **containers and seeds** run offline once the images are
pulled, with **one exception**: `kuma-seed` runs `npm i socket.io-client@4`
into a scratch prefix at seed time, because Uptime Kuma v2 has no REST API for
configuration and the client has to come from somewhere.

Uptime Kuma itself does not, and since 2026-09-06 it deliberately does not: six
of its eight monitors leave the laptop, three pinging the Hetzner servers and
three reaching the products through the SSH tunnel. That is the point of them —
they watch an estate that is elsewhere. Off the network, those six read down
and the two local ones (`gitea`, `canary`) stay green.

## Monitors

`monitors.json` is **the estate**, not fixture content. Since 2026-09-06
(issue #441, M4 spec #427) the list is the real infrastructure this repository
runs on: one ping per Hetzner server, one HTTP check per product through the
tunnel, one on the local Gitea, and a canary that exists to be knocked over.
`fixtures/tidewater/work.json` still contains no monitors — a monitor is a
*mirrored item of the Kuma source*, never fixture work (M4 spec, "Assets are
knobas-owned; monitors are mirrored").

| Monitor | Type | Checks | Goes red when |
|---|---|---|---|
| `knobas-teamcity` | ping | `${KNOBAS_HETZNER_TEAMCITY_IP}` | the server is down or off the network |
| `knobas-jira` | ping | `${KNOBAS_HETZNER_JIRA_IP}` | ditto |
| `knobas-confluence` | ping | `${KNOBAS_HETZNER_CONFLUENCE_IP}` | ditto |
| `teamcity (tunnel)` | http | `http://host.docker.internal:8111/login.html` | TeamCity is down **or the tunnel is** |
| `jira (tunnel)` | http | `http://host.docker.internal:8080/status` | ditto |
| `confluence (tunnel)` | http | `http://host.docker.internal:8090/status` | ditto |
| `gitea` | http | `http://gitea:3000/api/healthz` | the local Gitea container is down |
| `canary` | http | `http://host.docker.internal:8299/` | `./canary.sh down` released the port |

The three tunnel checks are **named `(tunnel)` because they fall with the
tunnel**, which is not the same fact as the product being down — the ping on
the same server stays green and says so. Read the two together: all three
`(tunnel)` checks red with all three pings green is a dead tunnel (`./hetzner/tunnel
up`); one ping red with its own `(tunnel)` check is a dead server.

The endpoints are the unauthenticated ones each product answers `200` on
(`/login.html`, `/status`, `/status`); the obvious REST paths answer `401`,
which `accepted_statuscodes: ["200-299"]` reads as down.

**Can the Kuma container send ICMP?** Yes, and the per-server checks are
therefore real pings rather than TCP on 22. Measured on the pinned image
(2.5.3) under OrbStack on 2026-09-06: the container runs as `uid=0(root)`,
`/proc/sys/net/ipv4/ping_group_range` is `0 2147483647`, `/usr/bin/ping` is
present, and `docker exec knobas-uptime-kuma ping -c 2 46.224.117.158` came
back `2 packets transmitted, 2 received, 0% packet loss` for all three servers.
This matters because `hetzner/firewall-rules.json` opens **only** TCP 22 and
ICMP: with no ICMP the only honest per-server check would have been TCP on 22,
which measures sshd rather than the machine. Re-measure the `ping -c 2` above
if this environment ever moves off OrbStack; if it comes back with 100% loss,
switch the three
entries to Kuma's `port` type on 22 and say here that ICMP was the reason. That
is not only a `monitors.json` edit: `kuma-seed.mjs`'s `payload()` branches on
`ping` against everything-else-is-http, and `OWNED` (what counts as drift) has
no `port`, so both need a third case first. The check would also be a weaker
one — it measures sshd, not the machine.

**The three IPs come from `hetzner/hosts.env`**, which `hetzner/provision.sh`
writes and `.gitignore` keeps out of the repo (one person's account). So
`monitors.json` carries `${KNOBAS_HETZNER_*_IP}` placeholders, `seed-kuma.sh`
sources that file and passes the values into the seed container, and
`kuma-seed.mjs` refuses an unresolved placeholder — a ping monitor on the
literal string `${KNOBAS_HETZNER_JIRA_IP}` is a permanent red that looks like
an outage. **`./seed-kuma.sh` therefore fails without `hetzner/hosts.env`**,
and says to run `./hetzner/provision.sh`.

**Four of the eight check the host, not the compose network**, through
`host.docker.internal` — the three tunnel forwards and the canary are host
sockets bound to `127.0.0.1`. (`gitea` is the fourth non-server check and it is
*not* one of them: it reaches the container by its compose service name.) Docker Desktop and OrbStack define that alias themselves;
the `extra_hosts: host.docker.internal:host-gateway` line on the `uptime-kuma`
service is what makes the name *resolve* on a plain Linux engine, which defines
nothing. It does not make those four monitors *work* there: `host-gateway` is
the bridge gateway, and both the tunnel (`hetzner/tunnel`: `-L
127.0.0.1:$port:...`) and the canary bind `127.0.0.1` only, deliberately, so a
bridge-gateway client cannot reach them. Running this on Linux would also mean
binding those four targets on the bridge address; nobody has needed that, and
this environment is a laptop's. Measured on OrbStack: with and without the line
the alias resolves to `0.250.250.254` and reaches a host socket bound to
`127.0.0.1`, so it is a no-op here.

Adding it did cost one thing, once: a compose service whose definition changes
is **recreated** on the next `docker compose up -d`, so the shared Kuma was
stopped and replaced the first time anyone ran that after 2026-09-06. Its data
is in the `kuma-data` volume and survived; the cost is the seconds of downtime
and any live run reading Kuma across them. It is paid once per machine, and it
is the reason a compose edit belongs in the list under *One environment, one
owner at a time* rather than in a stream's own head.

**The seed owns the list.** `kuma-seed.mjs` deletes any monitor Kuma holds that
`monitors.json` no longer names (and any second monitor sharing a name with one
it does, which the UI will happily create), deletes and re-adds one whose type,
URL, hostname or interval drifted, and leaves a matching one alone — so a second
run changes nothing and an old list's leftovers do not linger. Everything else
about a monitor (notifications, tags, a paused state a live run left behind) is
the instance's business and is not touched.

### The canary

`./canary.sh up` binds `127.0.0.1:8299` with a one-line responder (`canary`,
200); `./canary.sh down` releases it; `./canary.sh status` says which. Both are
idempotent, and `up` waits until the port actually answers.

It exists because M4.1's alert chain has to be proven against the real Kuma —
a monitor goes down, an alert opens, it is acked, the thing recovers, the
alert closes — and **every other thing Kuma watches here is shared**. Gitea
serves the live suites, the three products serve the Atlassian and TeamCity
runs, the servers are the estate. Stopping one of those to manufacture a red
is how one stream's live run becomes another's mystery failure, so the estate
carries one check whose entire blast radius is a port nobody else binds. **No
live run stops a shared container** (M4 spec #427).

Port 8299 is deliberately **outside** the §5 map above: it is a host port, not
a published container port, so `check-ports.sh` neither knows nor should know
about it. The pid lives in `~/.knobas-canary-8299.pid` — in `$HOME`, for the same reason
`hetzner/tunnel` keeps its own in `~/.ssh/`: a process that outlives a shell
does not belong in the repository directory, and `./reset` therefore does not
have to learn to kill it.

Its interval is 20 s rather than the others' 60 s, so a live recipe's
knock-down and recovery are seconds rather than minutes, and every leg lands
inside one interval. Measured 2026-09-06 over two cycles, polling `/metrics` at
1 s: released, `monitor_status` read `0` after 14 s and 17 s; rebound, it read
`1` after 17 s and 17 s. An earlier pair of cycles saw a 5 s -- where in the
interval the release falls is what varies, so a recipe waits for the state and
not for a duration.

**Who knocks it over** (#450, M4.1's exit). `just kuma-live` binds the port
before its two suites and rebinds it from a `trap` however the run ends, and
`crates/knobas-app/tests/alert_chain_live.rs` — the exit witness — releases it,
waits for Kuma to publish `down`, syncs, acks the alert, binds it again and
waits for the recovery. Both go through this script and touch nothing else, so
"no live run stops a shared container" is a sentence you can check by reading
two call sites. The suite carries a rebind in `Drop` as well as the recipe's
trap: a killed run and a panicking one leave the estate green either way.
Measured through the suite on 2026-09-07 (the notebook otherwise idle,
`/metrics` polled every 2 s through the adapter): `down` after 14.9 s, `up`
again after 21.2 s, the whole chain — import, fall, inbox, ack, recovery — in
38.7 s.

### Reading the state back

Verify through the channel the adapter will actually use — `/metrics`, not the
UI:

```sh
curl -fsS -u ":$(cat kuma-api-key)" http://127.0.0.1:3001/metrics | grep monitor_status
```

Uptime Kuma v2 prunes raw heartbeats to ~24 h, **absence of a metric means
*unknown* rather than down**, and a response time of `-1` is a sentinel. The
seed asserts presence, never a particular value, and never waits for heartbeat
history.

**A paused monitor is not in `/metrics` at all.** Measured on the pinned image
on 2026-09-06 (#442): pausing a monitor over socket.io (`pauseMonitor`) removed
every one of its eight series, and `resumeMonitor` put all eight back within one
interval. So through this channel — which is the only channel an API key opens,
and the whole of `knobas-source-kuma`'s read path — *paused* and *deleted* are
**one observation**, and knobas tombstones a paused monitor and re-mirrors it on
resume.

That matters beyond the seed, because M4 spec #427's story 38 says a paused
monitor reads as *none* in an asset's health rollup: a monitor knobas cannot
see is not a monitor knobas can call paused. Telling the two apart needs the
socket.io channel, which is the Kuma write half's ticket. Recorded here rather
than left to be rediscovered, and stated in `crates/knobas-source-kuma/src/
cursor.rs` where the tombstone is written.

What the *adapter* reads from this document, and what each value means, is in
that crate's `model.rs` — `monitor_id` as the identity, the literal string
`"null"` for a field a monitor of that type has not got, `-1` as the response
time of a check that did not answer, and the four state codes off
`monitor_status`'s own `# HELP` line.

A gotcha of the `add` event, hit while adding the first ping monitor:
`server.js` runs `monitor.accepted_statuscodes.every(...)` on **every** monitor
on the way in, with no type check first. A ping monitor needs no status codes
at all, and one sent without the field is refused with `Cannot read properties
of undefined (reading 'every')` — an error naming neither the field nor the
monitor. `kuma-seed.mjs` therefore sends `accepted_statuscodes` for every type.

## Signed dev build (macOS)

Desktop notifications (M3.3, #290) need a `.app` bundle with a stable signed
identity, and an unsigned build re-prompts the keychain on every run (roadmap
gotcha 10). `tauri dev` produces neither, so the check runs against a bundle.
No Apple identity is required: a self-signed code-signing certificate is a
stable identity as far as the keychain and TCC are concerned.

The identity on the dev Mac is **`knobas-dev`**, a self-signed certificate
created 2026-09-03 (#273). If `security find-identity -v -p codesigning` does
not list it, recreate it:

```sh
cat > knobas-dev.cnf <<'CNF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = knobas-dev
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config knobas-dev.cnf \
  -keyout knobas-dev.key -out knobas-dev.pem
openssl pkcs12 -export -inkey knobas-dev.key -in knobas-dev.pem -name knobas-dev \
  -passout pass:knobas -out knobas-dev.p12
security import knobas-dev.p12 -k ~/Library/Keychains/login.keychain-db -P knobas \
  -T /usr/bin/codesign -T /usr/bin/security
security add-trusted-cert -r trustRoot -p codeSign \
  -k ~/Library/Keychains/login.keychain-db knobas-dev.pem
```

(`openssl pkcs12 -export` may need `-legacy` on an OpenSSL 3 that refuses the
default cipher. An Apple Development certificate from Xcode works the same way;
export its full quoted name instead.)

Build and sign a debug bundle from the crate that owns `tauri.conf.json`, the
way the `dev` recipe does. `env -u RUSTUP_TOOLCHAIN` for the reason at the top
of the justfile; `--bundles app` skips the dmg:

```sh
export APPLE_SIGNING_IDENTITY="knobas-dev"
cd crates/knobas-app
env -u RUSTUP_TOOLCHAIN PATH="$PWD/../../app/node_modules/.bin:$PATH" \
  tauri build --debug --bundles app
```

Verify, then launch the bundle rather than the bare binary -- **and launch it
from where it was built**, not from a copy:

```sh
codesign -dv --verbose=2 target/debug/bundle/macos/knobas.app   # Authority=knobas-dev
codesign --verify --deep --strict target/debug/bundle/macos/knobas.app
open --stdout /tmp/knobas.out --stderr /tmp/knobas.err target/debug/bundle/macos/knobas.app
```

(`--stdout`/`--stderr` are optional; they are where the `tracing` lines below
land, since a bundle launched from Finder or `open` has no terminal.)

`TeamIdentifier=not set` is expected for a self-signed certificate; the
notarization warning is expected too. The check has two steps:

1. **The banner** (#290): in that bundle, switch one category on, unfocus
   the window, and see one desktop notification in Notification Center.
2. **The click** (#339): click the banner's *body* -- there is no button --
   and knobas comes to the front on that item's room (`#/inbox` for a
   credential expiry, which has no entity). stderr carries the two lines to
   look for, `notification shown, waiting on its click` with the `address`,
   and then `notification clicked`; a clear from Notification Center logs
   `notification closed without a click` at `debug` instead
   (`RUST_LOG=knobas_app=debug` to see it).

**The Launch Services caveat, and why the path matters.** knobas sends
through `notify-rust`'s `UNUserNotificationCenter` backend, and a click on
one of its banners activates *the bundle Launch Services has registered for
`dev.knobas.desktop`* -- which is the one `tauri build` wrote at
`target/debug/bundle/macos/knobas.app`, because that is the copy LS saw
first. Run 3 of the prototype (`prototype/notification-click`, its log)
launched a copy from elsewhere, and the click started a **second instance**
from the registered path while the running one waited on. Launched from the
registered path (run 4) it stayed single-instance and the click landed. If a
click ever launches a second knobas, that is this, not the waiter.

Also from the ruling: **`tauri dev` shows no desktop notification on macOS.** The UN
backend refuses a bare binary with *No bundle identifier found.
UNUserNotificationCenter requires a valid .app bundle.*; the `notify` command
rejects with that sentence, the store swallows it, and the dev terminal shows
it at `warn` (`notification not shown`). The permission prompt and the
setting still work in dev; the banner and its click are the bundle's.

**The first run of that check can show nothing, and if it does, look at macOS
before knobas.** Written down here because it cost an afternoon once. A fresh
bundle identifier has not been authorized to notify, and knobas' own code asks
for nothing at send time: the send is knobas' own `notify` command calling
`notify-rust` on its `UNUserNotificationCenter` backend (since #339), and the
plugin is asked only `is_permission_granted` / `request_permission`, on the
click that switches a category on. What `notify-rust`'s UN backend itself does
about authorization at that moment is the part nobody has watched. What was
observed on 2026-09-03 (#290), while the send still went the plugin's way
(`tauri-plugin-notification` -> `notify-rust` -> `mac-notification-sys`, the
legacy `NSUserNotification` path): macOS raised its own banner-shaped prompt
-- *"knobas Notifications: Notifications may include alerts, sounds, and icon
badges"* -- on the app's first contact with `usernoted`, and **withdrew it
unanswered when the app quit**, after which it was not asked again. Until
somebody answered it, every desktop notification was *delivered and never
presented*: it went into Notification Center's store and no banner appeared.
**Whether a first run behaves the same under the UN backend has not been
re-observed.** The dev Mac has had `dev.knobas.desktop` authorized since that
day and there is no cheap way to un-authorize it, so every run since #339 has
been a later run, and the prompt's withdrawal is #290's dated observation of
the NS path, not a fact about the current send. Read the log below before
trusting a silent run either way.

Both states are visible in the unified log, and this is the way to tell them
apart without guessing:

```sh
/usr/bin/log show --last 5m --info --predicate 'process == "usernoted"' --style compact \
  | grep -E 'knobas.desktop|askpermissions'
```

(`/usr/bin/log` by path, because zsh has a builtin named `log` that answers
"too many arguments" and shows nothing; `--info`, because both lines below
are info-level and the default level filter hides them.)

* `Delivering <NotificationRecord app:"dev.knobas.desktop" ...> to
  [ .alert .lockScreen .notificationCenter ]` with **no matching `Presenting`
  line** -- knobas' `notify` command posted it and macOS held the desktop
  notification back. Not authorized.
* `Presenting <NotificationRecord app:"dev.knobas.desktop" ...> as banner` --
  the check passed.
* `Event was resolved: ... outcome: allowed; reason: disabled` from
  `donotdisturbd` says no Focus mode is swallowing anything; an `outcome`
  other than `allowed` is a Focus, not a knobas bug.

**To answer the prompt after it has been withdrawn:** System Settings ->
Notifications -> Application Notifications -> **knobas** -> *Allow
notifications* on. The app is listed there once it has posted at least one
desktop notification, even though it never appears in
`~/Library/Preferences/com.apple.ncprefs.plist` while it is refused -- an
absence from that file is therefore not evidence that the code never ran.

**Result of the check, 2026-09-03, macOS 26.5.2 (25F84), `knobas-dev`
self-signed:** with authorization on, one `credential_expiry` item arriving on
an unfocused window produced exactly one banner, *"Tidewater (mock) -- the
Tidewater (mock) credential expires on Sunday 06 Sep 2026"*, with
`Presenting ... as banner` in the log. A self-signed certificate with
`TeamIdentifier=not set` is sufficient; no Apple identity is needed.

**The click, since #339.** Until then the check could not prove it:
`tauri-plugin-notification` 2.4.0's desktop `notify` discards the
`notify-rust` handle a click arrives on, and its `register_listener` -- the
command behind `onAction` -- is mobile-only. knobas now sends through
`notify-rust` itself (`crates/knobas-app/src/notify.rs`), waits on the handle
off the main thread, and emits `notification:clicked` with the item's
address; step 2 above is its witness, and the prototype's verdict table on
#339 is the evidence the ruling was made on. **What no run proves is Linux or
Windows**: the freedesktop `default` action and the Windows handle API are
written from `notify-rust` 4.18.0's sources and have not been clicked on
either platform -- disclosed here, in `notify.rs`'s and
`notify.svelte.ts`'s module headers, and in `docs/contract.md` §10.8.

## The desktop witness (macOS)

Three of knobas' features belong to the operating system rather than to the
app -- open-in-editor, open-in-terminal and the ⌘K capture shortcut -- and
there is no instance to run a suite against, so ADR-0016 makes their witness a
*scripted run against a real bundle*, not a checklist somebody ticks. This is
that harness (issue #500).

```sh
just desktop-witness launcher-hotkey    # the drivers are listed if you omit one
```

It builds and signs a debug bundle with the `knobas-dev` identity from the
section above, registers it with Launch Services, checks that this session can
actually drive a desktop, launches the bundle **from the path Launch Services
resolves `dev.knobas.desktop` to** on the demo profile, waits for its window,
asserts that exactly one instance is running, runs the named driver, and quits
the app with ⌘Q. Drivers live in `testenv/desktop-witness/drivers/`, and there
are two.

`launcher-hotkey` (#500) presses ⌘K, asserts through the accessibility tree
that the launcher's query box has focus (`AXTextField`, labelled *Search or
act*, which is the `aria-label` in `QueryBox.svelte`), and presses Escape --
after which the box must be gone from the tree, because `Launcher.svelte`
renders the overlay under `{#if open}` and a closed launcher takes its input
out of the DOM.

`open-in-editor` (#501) is the spawn: it writes a stub executable and a fake
clone into a scratch directory, types that directory into the **Clones root**
field and `<stub> {path}` into the **Open in VS Code** field in Settings,
opens a repo detail through the launcher, presses *Open in VS Code*, and
asserts that the stub ran with **exactly one** argument and that it is the
checkout path. Exactly one is the assertion ADR-0016 asks for: what the
program received is what the disk answered, and nothing the mirror holds.

Every accessible name either driver acts on is pinned against the file that
carries it by `just witness-unit`, so an edit to one reads as a stale driver
at the gate rather than as a failed run minutes into somebody's screen.

**Scope: OS-level features only.** That is the v1.5 grilling's ruling, and the
reason for it is that those features have no instance to run a suite against.
A rendered panel is witnessed by headless Chrome against the `?fake-ipc` dev
server -- the deputy's ruling of 2026-09-08 on #496, which seven tickets
depend on -- and not here. This harness takes the screen and runs one at a
time; putting panel QA on it would serialise the milestone behind it.

One witness runs on a machine at a time, and the harness enforces it rather
than asking: it takes `$TMPDIR/knobas-desktop-witness.lock` with `mkdir` and
refuses if another run holds it. Two runs would fight over one screen, one
bundle identifier and one Launch Services registration, and the loser would
report the winner's app as its own.

`just desktop-witness` is **not** part of `just check` and must not become
part of it. What the gate carries is `just witness-unit`
(`testenv/desktop-witness-test.sh`), over every piece of the harness that is a
decision rather than a side effect: how it compares Launch Services' answer
against the bundle it built, how it reads the accessibility probe, and the
three text functions `open-in-editor` is built out of -- the stub it writes,
the git config it writes, and its reading of what the stub recorded. A driver
is not exempt from the gate because its *run* is.

### The prerequisites

1. **The `knobas-dev` code-signing identity**, exactly as the *Signed dev
   build* section above creates it.

2. **Accessibility, granted once to the terminal that runs the witness.**
   System Settings → Privacy & Security → **Accessibility**, and the entry to
   add is the terminal application (`com.mitchellh.ghostty` on the dev Mac),
   not a script. A shell started before the grant does not see it; start a new
   one.

   **Accessibility, and deliberately not Automation.** The obvious way to
   write a driver is `osascript` against `System Events`, and every one of
   those is an Apple Event, which TCC gates under *Automation* with a per-
   target modal prompt. Nobody is at this machine when the ticket loop runs,
   and an unanswered prompt is not a deferral -- TCC records it as a **denial**
   (`auth_reason 9`). Both of the dev Mac's Automation rows were written that
   way on 2026-09-08, one for `com.apple.systemevents` and one for
   `dev.knobas.desktop`, by two probes nobody was there to answer, and a
   denied grant is sticky. So `testenv/desktop-witness/ax.swift` uses the
   accessibility API and `CGEvent` directly instead: one grant, made once, no
   prompt at run time. `AXIsProcessTrusted()` and
   `CGPreflightPostEventAccess()` are what the harness probes, and neither
   prompts.

3. **An unlocked screen.** Not a formality: no synthetic keystroke reaches an
   application behind the lock screen, and the window server answers a query
   for a third-party app's windows with the *application* element instead of
   the window, so a driver behind a lock reads an empty tree and blames the
   app. Measured on 2026-09-08, locked: Ghostty, knobas, WhatsApp, Helium and
   Proton Mail all answered `AXWindows` with one element whose role was
   `AXApplication`, while `AXMenuBar` came back in full. The harness probes
   `CGSessionCopyCurrentDictionary`'s `CGSSessionScreenIsLocked` and refuses,
   twice -- once before the build and once after it, because a build takes
   minutes and the only moment worth probing is the one just before the
   keystroke.

4. **No other copy of `dev.knobas.desktop` registered with Launch Services.**
   The witness has to launch the copy LS has registered, for the reason the
   *Signed dev build* section gives: a notification click activates that copy,
   whichever it is, and a click that finds a different one running starts a
   second instance. Measured on 2026-09-08: with `/Applications/knobas.app`
   installed (v0.1.0), `lsregister -f` on a freshly built debug bundle did
   **not** move the registration -- `dev.knobas.desktop` still resolved to
   `/Applications/knobas.app`. Which copy LS prefers among several carrying
   one identifier is not a documented API and the installed one wins, so the
   harness refuses rather than launching a copy LS does not name. Move or
   remove the installed knobas for the length of the run — that is the
   runner's job, not the harness's, and #525 carries it as a precondition. The
   harness never moves it for you, and it re-registers whatever was registered
   before on its way out.

5. **`swiftc`**, from the Command Line Tools. The accessibility helper is one
   Swift file compiled into a scratch directory on each run (about 2 s);
   nothing is committed as a binary.

Every refusal names the permission by the name it has in System Settings and
points back at this section, and exits non-zero.

### What is not witnessed yet

As of 2026-09-08 **no green driver run exists**, and the debt has a number:
**issue #525**, *Desktop witness: the first unlocked run*, which carries the
run for every driver this harness gains and which the v1.5 exit waits on. The
dev Mac has been locked since 22:47 CEST with no HID input for nearly three
hours, and Björn is away for the milestone; the harness refuses at its first
probe, which is the correct behaviour and is **not** a witness of the ⌘K
assertion — a refusal is not a witness of the assertion. The deputy's ruling
of 2026-09-08 on #500 settles what follows from that: the ticket merges with
its run-criterion open and disclosed, nothing stands in for the run (no fake,
no dry-run mode, no hand checklist — ADR-0013), and ADR-0016 now carries the
dated consequence that *"No human step" is not "no human precondition"*.

Four things are therefore still open, and none should be read as proven by
this file existing:

* whether a Tauri window's `WKWebView` exposes the launcher's input to the
  accessibility API at all. Some web views build their tree only when an
  assistive client asks, and behind the lock screen there is no way to find
  out; `ax dump` in the driver's failure path is there so that the first
  unlocked run diagnoses itself rather than needing a second.
* the launch, single-instance, driver and quit steps, which begin after the
  probe the harness stops at.
* which of `AXDescription` and `AXTitle` a WebKit text field carries an
  `aria-label` on. The driver accepts either, and asserts the role separately,
  so this cannot make it pass on the wrong element -- but it has not been seen.
  The same question, one step further, for a field named by a `<label for=…>`
  rather than an `aria-label`, which is how the **Clones root** and the three
  command fields are named: `open-in-editor` asks for **exactly one** element
  carrying that name, so a WebKit that exposed the `<label>` element under the
  same name as well would make it refuse -- with the tree dump beside the
  refusal, which is what that dump is for.
* whether `AXFocused` and `AXPress` reach a WebKit element at all, which is
  what `ax focus` and `ax press` (#501) do and what `open-in-editor` is built
  on. Both are the documented way to drive an accessibility tree; neither has
  been sent at this app.

**And one gap that is not about the lock at all: the demo profile carries no
repo entity.** `knobas_source_mock::items` emits tickets, PRs, builds, pages
and commits; the fixture's `repos` and `branches` (`fixtures/tidewater/work.json`)
are parsed and never sent. So `--demo` -- the profile this harness launches,
on purpose, so that a run cannot mix fixture data into somebody's real corpus
-- has no repo detail to open, and `open-in-editor` refuses at that step with
a message that says exactly this. **An unlocked Mac is therefore necessary and
not sufficient for #501's witness**: #525 needs a demo corpus that carries a
repo before the driver can reach its button. Nothing in #501 papers over it,
because a fake would not be a witness (ADR-0013), and widening the demo corpus
is a change to the reference adapter that #501 does not name.

What *is* witnessed, on 2026-09-08, and by what:

* **from a harness run**, `just desktop-witness launcher-hotkey` and
  `just desktop-witness open-in-editor`: the refusal. `screen-locked`, naming
  the permission and this section, exit 1. A merge-manager re-running either
  recipe on this Mac gets the same refusal, and that is the expected result and
  not a regression.
* **from its own commands run by hand**, because the harness refuses before it
  reaches them: the build and signing step -- `Authority=knobas-dev`,
  `Identifier=dev.knobas.desktop`, `codesign --verify --deep --strict` clean,
  the bundle at `target/debug/bundle/macos/knobas.app`, which is the first of
  the two paths the harness looks in -- and the Launch Services measurement in
  prerequisite 4 above.

Nothing between the probe and the quit has run at all.

**A merge-manager re-running `just desktop-witness` on this Mac gets the same
`screen-locked` refusal, exit 1.** That is the expected result here, not a
regression; #525 is where it stops being.

## Scripts

| Script | Does |
|---|---|
| `./seed` | Gitea and Kuma, in order. Idempotent — re-running is a no-op that exits 0. Needs `hetzner/hosts.env`, because `./seed-kuma.sh` does. |
| `./seed-gitea.sh` | Org, users, repos, branches, commits, PRs, comments, reviews. |
| `./seed-kuma.sh` | Kuma admin account, monitors, API key. Owns the monitor list: what `monitors.json` no longer names is deleted. Needs `hetzner/hosts.env` for the three server IPs. |
| `./kuma-monitor.sh` | `add <name> <url>` / `delete <name>` on one throwaway monitor, through the same socket.io channel the seed uses. Idempotent both ways. What `just kuma-live` witnesses a deletion with; not part of the baseline, so `./seed-kuma.sh` sweeps whatever it leaves. |
| `./canary.sh` | `up` / `down` / `status` on the canary's host socket, `127.0.0.1:8299`. The one thing a live run may knock over. |
| `./fetch-timebomb-keys.sh` | Pulls the two 10-user, 3-hour Data Center timebomb keys off Atlassian's public page, checks each decodes to the right product, prints `export` lines (`--write` also drops them in the git-ignored `.env.licences`). `seed-atlassian.sh` calls it when a key is unset. |
| `./seed-atlassian.sh` | The real Jira and Confluence containers' setup wizards, unattended (`--profile real-atlassian`); `./seed --atlassian` runs the script below after it. Takes `jira` or `confluence` to walk one wizard, which is the shape a loaded machine wants — see *Jira and Confluence, end to end*. |
| `./seed-atlassian-content.sh` | The Tidewater people, projects, issues, comments, worklogs and links in the real Jira; the ENG space, pages and comments in the real Confluence. `--verify` reads PAY-231 and one page back. `just atlassian-live` runs the whole window. |
| `./seed-teamcity.sh` | The real TeamCity container's first start, an access token and one authorised agent (`--profile real-teamcity`); then runs the script below. |
| `./seed-teamcity-builds.sh` | The Tidewater projects, build configurations, VCS roots and builds in the real TeamCity; `--running` for the fixture's running build. |
| `./pin-images.sh` | Re-resolve image tags to digests into `.env`; guarded pins are held, `--move VAR` takes a new one. |
| `./check-ports.sh` | Assert the compose file against the §5 port table, default profile, opt-in profiles and the capped overlay. Starts nothing. |
| `./desktop-witness.sh` | The desktop witness (#500): builds and signs the debug bundle, launches the copy Launch Services registered, runs one driver from `desktop-witness/drivers/` against the real accessibility tree, quits. Takes the screen; one at a time; not part of `just check`. See *The desktop witness (macOS)*. |
| `./desktop-witness-test.sh` | The witness's own unit tests, over `desktop-witness-lib.sh` -- the Launch Services path comparison and the accessibility probe's reading. No screen, no bundle, no macOS. `just witness-unit`, and part of `just check`. |
| `./reset` | `down -v` every profile, and delete the seed's outputs. |

## mockd's documented deviations

mockd is deliberately **stricter** than the real products in several places, so
that an adapter which passes here passes there — never the reverse. The list is
maintained in one place and must not be copied:

> **`crates/knobas-mockd/src/lib.rs`**, the *Documented deviations* section of
> the module docs.

Read it before concluding that mockd is wrong.
