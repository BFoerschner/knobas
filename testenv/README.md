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
| 8213 | **reserved** — Flowrun stub (M4) | *bound by nothing* |
| 8111 | real TeamCity server | `--profile real-teamcity` |
| 8080 | real Jira Software | `--profile real-atlassian` |
| 8090 | real Confluence | `--profile real-atlassian` |

8213 is reserved on purpose and bound by nothing — in the compose file *and*
in the `mockd` binary. It was not forgotten: Flowrun is ADR-0013's single
named exception. 8211, reserved for a Confluence mock until 2026-09-03, is
unreserved by the same ADR; the real Confluence on 8090 is the witness.

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

`eval "$(./seed --env)"` re-prints them without re-seeding. Once `./seed
--teamcity` has run it also prints `KNOBAS_TEAMCITY_URL` and
`KNOBAS_TEAMCITY_TOKEN`; the seeded TeamCity suite additionally reads
`seed-state.json` for the fixture-number-to-id map, from
`testenv/seed-state.json` relative to the crate unless
`KNOBAS_TEAMCITY_SEED_STATE` names another path.

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
| `just start-work-live` | `knobas-app` / `start_work_live` — M2 exit criterion 1, ticket → branch → PR → link → status and back | as above | as above |
| `just teamcity-live` | `knobas-source-teamcity` / `live_teamcity` — the adapter against the **public JetBrains** instance, read-only | `KNOBAS_TEAMCITY_URL` | the repo-root `.env`: `cp .env.example .env` |
| `just teamcity-live-seeded` | `live_teamcity_seeded` — the adapter against **our** seeded TeamCity | `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` | `./seed --teamcity` then `eval "$(./seed --env)"` |
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

`just teamcity-live` reads the **repo-root `.env`** and points at a server that
is not ours; a green from it is not a statement about the container in this
directory, in either direction. That is `teamcity-live-seeded`.

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
minute later. All three print the state or the form the product is showing,
and how far into the cap they are, every 30 s.
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
key), its five pages under the space home page with the fixture's `##`
sections as `<h2>`/`<p>` storage format, their comments, and *Standup
protocols* as the empty page the standup flow will publish under. Only
*SEPA payout retry design* has a body and a comment in the fixture — the other
four are titles under the space home, which is what makes them the test that
an empty page is still a page. The ids Confluence assigns are recorded as
`confluence.pages[]`, with `confluence.space`, `confluence.home_page_id` and
`confluence.author` beside them; the Confluence live suite reads all four. The
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

Verify, then launch the bundle rather than the bare binary:

```sh
codesign -dv --verbose=2 target/debug/bundle/macos/knobas.app   # Authority=knobas-dev
codesign --verify --deep --strict target/debug/bundle/macos/knobas.app
open target/debug/bundle/macos/knobas.app
```

`TeamIdentifier=not set` is expected for a self-signed certificate; the
notarization warning is expected too. The #290 manual check is: in that
bundle, switch one notification kind on, unfocus the window, and see one
notification in Notification Center.

**The first run of that check shows nothing, and the reason is macOS, not
knobas.** Written down here because it cost an afternoon once. A fresh bundle
identifier has no notification authorization, and the legacy
`NSUserNotification` path the plugin ends up on (`tauri-plugin-notification` ->
`notify-rust` -> `mac-notification-sys`) does not raise the permission sheet
itself. macOS raises its own banner-shaped prompt -- *"knobas Notifications:
Notifications may include alerts, sounds, and icon badges"* -- on the app's
first contact with `usernoted`, and **withdraws it unanswered when the app
quits**, after which it is not asked again. Until somebody answers it, every
notification is *delivered and never presented*: it goes into Notification
Center's store and no banner appears.

Both states are visible in the unified log, and this is the way to tell them
apart without guessing:

```sh
log show --last 5m --predicate 'process == "usernoted"' --style compact \
  | grep -E 'knobas.desktop|askpermissions'
```

* `Delivering <NotificationRecord app:"dev.knobas.desktop" ...> to
  [ .alert .lockScreen .notificationCenter ]` with **no matching `Presenting`
  line** -- knobas called the plugin and macOS held the notification back.
  Not authorized.
* `Presenting <NotificationRecord app:"dev.knobas.desktop" ...> as banner` --
  the check passed.
* `Event was resolved: ... outcome: allowed; reason: disabled` from
  `donotdisturbd` says no Focus mode is swallowing anything; an `outcome`
  other than `allowed` is a Focus, not a knobas bug.

**To answer the prompt after it has been withdrawn:** System Settings ->
Notifications -> Application Notifications -> **knobas** -> *Allow
notifications* on. The app is listed there once it has posted at least one
notification, even though it never appears in
`~/Library/Preferences/com.apple.ncprefs.plist` while it is refused -- an
absence from that file is therefore not evidence that the code never ran.

**Result of the check, 2026-09-03, macOS 26.5.2 (25F84), `knobas-dev`
self-signed:** with authorization on, one `credential_expiry` item arriving on
an unfocused window produced exactly one banner, *"Tidewater (mock) -- the
Tidewater (mock) credential expires on Sunday 06 Sep 2026"*, with
`Presenting ... as banner` in the log. A self-signed certificate with
`TeamIdentifier=not set` is sufficient; no Apple identity is needed.

**What the check still cannot prove is the click.**
`tauri-plugin-notification` 2.4.0's desktop `notify` hands the notification to
`notify-rust` and discards the result (`let _ = notification.show()` inside a
spawned task), and its `register_listener` -- the command behind `onAction` --
is mobile-only. So a notification's click has no channel to arrive on, and
`app/src/lib/inbox/notify.svelte.ts`'s navigation is proven against the stub
only. That is a gap in the plugin, recorded in the module header and in
`docs/contract.md` §10.8.

## Scripts

| Script | Does |
|---|---|
| `./seed` | Everything below, in order. Idempotent — re-running is a no-op that exits 0. |
| `./seed-gitea.sh` | Org, users, repos, branches, commits, PRs, comments, reviews. |
| `./seed-kuma.sh` | Kuma admin account, monitors, API key. |
| `./fetch-timebomb-keys.sh` | Pulls the two 10-user, 3-hour Data Center timebomb keys off Atlassian's public page, checks each decodes to the right product, prints `export` lines (`--write` also drops them in the git-ignored `.env.licences`). `seed-atlassian.sh` calls it when a key is unset. |
| `./seed-atlassian.sh` | The real Jira and Confluence containers' setup wizards, unattended (`--profile real-atlassian`); `./seed --atlassian` runs the script below after it. Takes `jira` or `confluence` to walk one wizard, which is the shape a loaded machine wants — see *Jira and Confluence, end to end*. |
| `./seed-atlassian-content.sh` | The Tidewater people, projects, issues, comments, worklogs and links in the real Jira; the ENG space, pages and comments in the real Confluence. `--verify` reads PAY-231 and one page back. `just atlassian-live` runs the whole window. |
| `./seed-teamcity.sh` | The real TeamCity container's first start, an access token and one authorised agent (`--profile real-teamcity`); then runs the script below. |
| `./seed-teamcity-builds.sh` | The Tidewater projects, build configurations, VCS roots and builds in the real TeamCity; `--running` for the fixture's running build. |
| `./pin-images.sh` | Re-resolve image tags to digests into `.env`; guarded pins are held, `--move VAR` takes a new one. |
| `./check-ports.sh` | Assert the compose file against the §5 port table, default profile, opt-in profiles and the capped overlay. Starts nothing. |
| `./reset` | `down -v` every profile, and delete the seed's outputs. |

## mockd's documented deviations

mockd is deliberately **stricter** than the real products in several places, so
that an adapter which passes here passes there — never the reverse. The list is
maintained in one place and must not be copied:

> **`crates/knobas-mockd/src/lib.rs`**, the *Documented deviations* section of
> the module docs.

Read it before concluding that mockd is wrong.
