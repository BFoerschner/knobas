# `testenv`, the knobas test environment

A small fake company, Tidewater Freight, served by real products in Docker: Gitea and Uptime Kuma by default, and
Forgejo, TeamCity, Jira and Confluence on demand. The seeds fill them with the Tidewater fixture, and the live suites
run against them. This front page covers starting the environment, its ports, credentials and variables, the live
recipes, who may use the environment, and the scripts. The long topics have pages of their own, listed under *Find your
way*.

Start it on this machine and seed it:

```sh
docker compose --env-file images.env up -d   # gitea, https-front, docker-proxy, uptime-kuma
./seed                                       # the Tidewater content into Gitea and Uptime Kuma
eval "$(./seed --env)"                       # the variables a live suite reads
```

- Every compose call passes `--env-file images.env`, the file of image pins. A bare `.env` in this repository means the
  repo-root file of tokens and nothing else.
- `./seed` needs docker, curl and jq, waits up to 120 s for each product, and changes nothing when run again.
- These commands talk to this machine's Docker. Only `testenv/live`, the live recipes and the scripts that source
  `live-lib.sh` read `testenv/live.env`, which says where the live products run. When it names the WSL box, start
  products with `just live-up <product>` ([wsl/README.md](wsl/README.md)).

Commands on this page and the pages it links run in `testenv/` unless the text says otherwise. An agent's shell starts
at the repository root, where a hook refuses `cd testenv && ...`, inside `$(...)` too. So an agent prefixes each command
with `env -C testenv`, as in `env -C testenv ./seed` and `eval "$(env -C testenv ./seed --env)"`.

## Find your way

| You want to | Read |
| --- | --- |
| Start the environment and seed it | The commands above, then [Ports](#ports) and [Credentials](#credentials) |
| Run a live suite | [The live recipes](#the-live-recipes), then [Live products, on demand](#live-products-on-demand) |
| Keep one product up for several runs | [Live products, on demand](#live-products-on-demand), then [docs/live-products.md](docs/live-products.md) |
| Clone or fetch from the live Gitea or Forgejo over https | [docs/live-products.md](docs/live-products.md), *The https front* |
| Show the app on live data | [Live products as sources](#live-products-as-sources-just-live-sources-2371) |
| Use the WSL box | [wsl/README.md](wsl/README.md) |
| Know who may use the environment, and when | [One environment, one owner at a time](#one-environment-one-owner-at-a-time) |
| Write an assertion against seeded data | [What the seed cannot reproduce](#what-the-seed-cannot-reproduce--read-this-before-writing-assertions) |
| Start a real TeamCity, Jira or Confluence by hand, or see what their seeds hold | [docs/real-products.md](docs/real-products.md) |
| Test at Björn's scale | [scale/README.md](scale/README.md) |
| Change the estate or the monitors, or use the canary | [docs/estate-and-monitors.md](docs/estate-and-monitors.md) |
| Measure frames or how fast a page opens | [perf/README.md](perf/README.md) |
| Drive the real macOS app | [desktop-witness/README.md](desktop-witness/README.md) |
| Find a script | [Scripts](#scripts) |
| Press through the app in a browser, or drive its real window | [ui-checks/README.md](ui-checks/README.md) |
| Fix a slow or broken build cache, or see why the gate runs as it does | [docs/build-and-gate-speed.md](docs/build-and-gate-speed.md) |

## Ports

Every published port binds `127.0.0.1` only. The environment holds developer-grade credentials and must never be
reachable from another machine.

| Port | Service | Profile |
| --- | --- | --- |
| 3000 | Gitea | default |
| 3001 | Uptime Kuma v2 | default |
| 3002 | Forgejo (#1195) | `--profile forgejo` |
| 3443 | Gitea over https, through `https-front` (#2377) | default |
| 3445 | Forgejo over https, through `https-front` (#2377) | default |
| 8111 | TeamCity server | `--profile real-teamcity` |
| 8080 | Jira Software | `--profile real-atlassian` |
| 8090 | Confluence | `--profile real-atlassian` |
| 8095 | `docs-assets`, the Swagger UI and files the Confluence showcase's embeds load (#2456) | `--profile real-atlassian` |

`./check-ports.sh` holds the same map and asserts the resolved compose model against it, for the default profile, every
profile together and both overlays. It fails on any bind that is not `127.0.0.1` and starts nothing. `.github/workflows/testenv.yml` runs it, but
that workflow is disabled since 2026-10-24, so run `./check-ports.sh` yourself after you touch `testenv/`. Nothing reads this table, so change a port in the compose file, in `check-ports.sh`
and here together.

Two host ports are outside the map on purpose, because a helper script binds each and no container publishes it. 8299 is
the canary (`./canary.sh`, [docs/estate-and-monitors.md](docs/estate-and-monitors.md)). 8298 is the stall (`./stall.sh`,
remote target only). No other port is reserved: 8200, 8210, 8211, 8212 and 8213, once held for mockd and two stubs, are
ordinary free ports.

## Credentials

All developer-grade and all in the clear on purpose. Nothing here is worth protecting, and nobody runs a seed that
prompts.

| Where | User | Password |
| --- | --- | --- |
| Gitea admin | `knobas` | `knobas-dev` |
| Gitea people (`mara.lindqvist` and the other fixture usernames) | fixture username | `tidewater-dev` |
| Uptime Kuma admin | `knobas` | `knobas-dev` |
| TeamCity admin (`--profile real-teamcity`) | `knobas` | `knobas-dev` |
| Jira and Confluence admin (`--profile real-atlassian`) | `knobas` | `knobas-dev` |

The seeds write their outputs into `testenv/`, all git-ignored. They live in one worktree, while every worktree on the
machine shares the containers.

| File | Written by | Holds |
| --- | --- | --- |
| `seed-state.json` | `./seed-gitea.sh` and the TeamCity, Jira and Confluence seeds | The Gitea token, each product's address and credentials, and the map from fixture ids to real ones |
| `seed-state-forgejo.json` | `./seed-forgejo.sh` | The same for Forgejo, so Gitea's token and id map are never touched |
| `kuma-api-key` | `./seed-kuma.sh` | The Uptime Kuma API key, which opens `/metrics` |

## Environment variables for adapter test suites

`./seed` prints these at its end, ready to paste or `eval`. `eval "$(./seed --env)"` prints them again without seeding.

| Variables | Printed by | Once |
| --- | --- | --- |
| `KNOBAS_GITEA_URL`, `KNOBAS_GITEA_TOKEN`, `KNOBAS_GITEA_OWNER`, `KNOBAS_GITEA_REPO` | `./seed --env` | `./seed-gitea.sh` has run |
| `KNOBAS_KUMA_URL`, `KNOBAS_KUMA_API_KEY`, `KNOBAS_KUMA_USER`, `KNOBAS_KUMA_PASSWORD` | `./seed --env`, `./seed --env-kuma` | `./seed-kuma.sh` has run |
| `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` | `./seed --env` | `./seed --teamcity` has run |
| `KNOBAS_JIRA_URL`, `KNOBAS_JIRA_USER`, `KNOBAS_JIRA_PASSWORD` | `./seed --env` | Jira is seeded |
| `KNOBAS_CONFLUENCE_URL`, `KNOBAS_CONFLUENCE_USER`, `KNOBAS_CONFLUENCE_PASSWORD` | `./seed --env` | Confluence is seeded |
| The four `KNOBAS_GITEA_*` for Forgejo, and `KNOBAS_GITEA_SEED_STATE` | `./seed --env-forgejo` | `./seed-forgejo.sh` has run |

The Gitea address is `http://127.0.0.1:3000`, the owner `tidewater` and the repository `payout-service`. Kuma is at
`http://127.0.0.1:3001`, with the admin account from the credentials table.

- `./seed --env` needs `seed-state.json`. Before it prints a Gitea token it asks Gitea whether the token still works
  (#1377). On any answer but 200 it prints nothing and exits non-zero, naming the status. Run `./seed` and try again.
- `./seed --env-kuma` prints only Kuma's four and needs no `seed-state.json`. `just kuma-live` uses it, so a Kuma run
  never starts or seeds Gitea.
- Uptime Kuma needs two credentials (#452). The API key opens `/metrics` and nothing else, and pausing a monitor is a
  socket.io login only the account can make. The suites never fall back to `knobas` and `knobas-dev`, so a Kuma with
  another admin fails as a missing variable and not as a broken adapter.
- The seeded TeamCity suites also read the map from fixture run numbers to real ids, from `testenv/seed-state.json`
  relative to the crate unless `KNOBAS_TEAMCITY_SEED_STATE` names another path.
- `just teamcity-live` and `just estate-live` read the repo-root `.env` instead (`cp .env.example .env`):
  `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` and `HETZNER_API_TOKEN`.

## Live products, on demand

On the local target the live products run on this machine, on demand, one JVM product at a time (Björn,
2026-10-12, #633). Every live recipe starts what it needs and puts it back when it ends, so one command is usually the
whole of it. For a session of several runs, start the product once:

```sh
just live-up jira        # = testenv/live up jira: start it and what it needs, wait, seed
just jira-live           # about 60 s instead of about 5 min: only the suites
just live-down jira      # = testenv/live down jira
just live-status         # one line per product: up or down
```

A run puts its product back as it found it. It stops a product it started and leaves one that was already up. Gitea is
the exception. No run stops Gitea, only `just live-down gitea` does, because another worktree's run may be using it by
then (#698).

| Product | `live-up` starts | and seeds with | `live-down` |
| --- | --- | --- | --- |
| `gitea` | `knobas-gitea` and the https front | `seed-gitea.sh` | stops it, and the https front when Forgejo is down too |
| `forgejo` | `knobas-forgejo` (`--profile forgejo`, #1195) and the https front | `seed-forgejo.sh` | stops it, and the https front when Gitea is down too |
| `teamcity` | Gitea first, because the VCS roots clone it, then `knobas-teamcity` and `knobas-teamcity-agent` | `seed-gitea.sh`, then `seed-teamcity.sh` (wizard, token, agent, runs) | stops the server and the agent, and Gitea stays up |
| `jira` | `knobas-jira-db`, `knobas-jira` | a timebomb key, `seed-atlassian.sh jira`, `seed-atlassian-content.sh jira` twice, then `--verify` | removes both with their volumes, and the `jira` block of `seed-state.json` |
| `confluence` | `knobas-confluence-db`, `knobas-confluence`, `knobas-docs-assets` | the same for `confluence` | removes all three with the volumes, and the `confluence` block of `seed-state.json` |

Three rules decide who runs when:

- **One JVM product at a time.** The Docker VM here has 8 GB, and Jira alone wants about 4 GB. `live-up` and every live
  recipe refuse TeamCity, Jira or Confluence while another of the three is up, in one line that names it. If that
  product is yours, `just live-down` it. The recipes never stop it themselves, because it may be somebody's session.
  Gitea and Forgejo are never refused.
- **JVM runs take turns on one lock** (#690). Every recipe that starts or uses a JVM product holds
  `$TMPDIR/knobas-live-jvm.lock` for its whole run. A run that finds it held says who holds it and waits up to 45
  minutes (`KNOBAS_LIVE_LOCK_WAIT`, in seconds), then refuses. `readlink` the lock to see the holder. A dead holder's
  lock is taken over.
- **Gitea runs take turns on a lock of their own** (#724). `just gitea-live`, `just gitea-live-capped` and
  `just start-work-live` hold `$TMPDIR/knobas-live-gitea.lock`. A TeamCity run and `just live-up gitea` take no Gitea
  lock.

A JVM product you started with `just live-up` keeps the JVM lock until `just live-down` (#2121), and every other
worktree's JVM run queues behind it. So run `just live-down <product>` when your session is over, from any worktree. A
forgotten keeper lets go after 3 hours (`KNOBAS_LIVE_KEEPER_MAX`) or when its worktree is removed. A product started by
hand with `docker compose` takes no lock.

A remote target has rules of its own: no run stops a product there, the one-JVM refusal is off, and each JVM product has
its own lock ([wsl/README.md](wsl/README.md)).

[docs/live-products.md](docs/live-products.md) has the rest: how the locks and the keeper work, Forgejo as a Gitea
source, why `down` differs by product, the docker context a start refuses, and *The https front*, the Caddy container
that puts Gitea and Forgejo behind https, with its one trust step, `just live-trust-ca`.

## Live products on a remote machine (WSL)

The live products may run around the clock on a WSL box instead of on demand in this machine's Docker (Björn,
2026-11-01). `testenv/live.env` in the main checkout is the switch, and with no file everything on this page holds as
written. On a remote target every docker call goes to the box's engine, an ssh tunnel brings the product ports to this
machine's loopback, and a memory budget decides what runs at once. Read the page to set a box up, to switch targets, or
when a run meets the tunnel, the memory budget or a licence renewal.

See [wsl/README.md](wsl/README.md).

## Live products as sources (`just live-sources`, #2371)

`just live-sources` adds one source per live product to the demo profile: `live-gitea`, `live-forgejo`, `live-jira`,
`live-confluence`, `live-teamcity` and `live-kuma`. `just live-demo` does the same and then opens the app. That is how
to show Björn the app, never `just dev`, `just demo`, `just shot` or `just live-shot` (Björn, 2026-11-02). He has none
of the mock corpus, and a screenshot is not the app.

Before you run it:

- Quit any demo knobas. The command starts the demo profile's own database, which one process holds at a time.
- The products you want must be seeded and answering (`just live-status`). One with no seed, or with nothing answering
  at its address, is skipped with one line, and the rest are still added.
- Seed outputs are per worktree, so in a fresh worktree every product is skipped as "not seeded". Copy
  `seed-state.json`, `seed-state-forgejo.json` and `live.env` from the root checkout's `testenv/` first, and
  `kuma-api-key` for the Kuma source.
- Run `just live-trust-ca` once on the Mac. Gitea and Forgejo are registered at the https front
  (`https://127.0.0.1:3443` and `:3445`), so that *Clone* and a pull request's fetch work from the app. Without the
  trust step the run still adds them and says the certificate is not trusted yet.

```sh
just live-sources                          # target/debug/knobas-app, then the demo profile
just live-demo                             # the same, then `just demo`
just live-demo-bundle path/to/knobas.app   # the bundle's own binary, then opens it with --demo
just live-sources-remove [knobas.app]      # the six sources off again, with what they synced
```

What it does:

- Addresses and credentials come from `./seed --env`, `--env-forgejo` and `--env-kuma`, so the target is whatever seeded
  them. Gitea and Forgejo sync the seeded owner's repositories (`tidewater`), Jira the seeded projects, Confluence the
  seeded space.
- It runs `knobas-app live-sources apply --demo`, which a release binary refuses. Each source goes through the app's own
  add or update and then `set_secret`, which stores the credential in the demo profile's secret store and tests it, as
  the Settings form does. The sources go in on stdin, never in argv.
- Each line says whether the source was added or updated and what its credential test said. Running it again updates
  every source in place, which is also how a freshly minted token is picked up.
- `testenv/live-sources.sh` takes `--data-dir <dir>` to put the profile, and its credential file, in a scratch
  directory.

A debug build never asks for the keychain (#2384). It keeps the demo profile's credentials in `dev-secrets.json`
(mode 0600) inside the demo profile's directory, and every debug binary and bundle reads that file. Release builds and
your own profile still use the keychain. A source you added by hand on the demo profile before #2384 needs its
credential entered again in Settings.

When it fails:

- A line says `credential unauthorized` and the run exits 1, listing the sources not connected. The product refuses the
  recorded credential. The source is still saved, and `just live-demo` stops before the app opens. Run
  `just live-up <product>` in the same checkout to mint fresh credentials, then run it again.
- The run says `./seed --env refused`, skips Gitea, Jira, Confluence and TeamCity, adds the rest and exits 1. Gitea
  refuses this worktree's recorded token, so `./seed --env` prints nothing. The fix is the same.

## The live recipes

Every live suite that certifies something has a `just` recipe, and the recipe is how you run it. The suites are
`#[ignore]`d, so `just check` never runs them, and a suite with no recipe is one nothing runs (#347, #350). Each
recipe's header in the `justfile` says what it certifies and what it writes. A new recipe named `<x>-live` needs a row
here, and a row needs its recipe: `scripts/doc-lists.py` fails `just check` on either miss.

| Recipe | Suite | Gated on | Where the variables come from |
| --- | --- | --- | --- |
| `just gitea-live` | `knobas-source-gitea` / `live_gitea`, the adapter against the shapes interfaces §4.2 fixes. Then named tests of `knobas-app` / `start_work_live`, and `knobas-app` / `clone_live` and `read_cache_live`. `just gitea-live forgejo` runs the same against Forgejo | `KNOBAS_GITEA_URL`, `KNOBAS_GITEA_TOKEN` | `live_session gitea`: Gitea started if down, `./seed-gitea.sh`, `eval "$(./seed --env)"`. Gitea stays up afterwards |
| `just gitea-live-capped` | `knobas-source-gitea` / `live_gitea_capped`, one test: paged walks against a server capped to one entry per page | as above | as above, then the capped overlay, taken off again from a trap |
| `just start-work-live` | `knobas-app` / `start_work_live`, the whole file: the start-work flow over this Gitea (ticket, branch, pull request, link, status and back), then that pull request on the standup digest, M3.3's digest witness for Gitea. The ticket side is mockd in process, so this is not M2 exit criterion 1's certificate | as above | as above |
| `just teamcity-live` | `knobas-source-teamcity` / `live_teamcity`, the adapter against the public JetBrains instance, read-only | `KNOBAS_TEAMCITY_URL` | the repo-root `.env`: `cp .env.example .env` |
| `just teamcity-live-seeded` | `knobas-source-teamcity` / `live_teamcity_seeded`, the adapter against our seeded TeamCity. Then `knobas-app` / `teamcity_seeded_live`, read-only, M3.3's digest witness for this source | `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` | `live_session teamcity`: Gitea and TeamCity started and seeded. TeamCity is stopped again if the run started it |
| `just kuma-live` | `knobas-source-kuma` / `live_kuma`, the adapter against this Uptime Kuma, pause and resume included (#452). Then `knobas-app` / `kuma_write_live`, the same write through the write queue. Then `knobas-app` / `alert_chain_live`, M4.1's exit witness, which releases the canary. The recipe binds the canary first and rebinds it from a trap | `KNOBAS_KUMA_URL`, `KNOBAS_KUMA_API_KEY`, `KNOBAS_KUMA_USER`, `KNOBAS_KUMA_PASSWORD` | the recipe starts `uptime-kuma`, runs `./seed-kuma.sh`, then `eval "$(./seed --env-kuma)"` |
| `just jira-live` | `knobas-source-jira` / `live_jira_seeded`, the Jira half of `knobas-app` / `atlassian_live`, and `knobas-app` / `share_exit` | `KNOBAS_JIRA_URL`, `KNOBAS_JIRA_USER`, `KNOBAS_JIRA_PASSWORD` | `live_session jira`: the recipe starts, seeds and removes Jira itself |
| `just confluence-live` | `knobas-source-confluence` / `live_confluence_seeded`, the Confluence half of `atlassian_live` (by name, in the `justfile`'s `atlassian_live_confluence_tests`), and `knobas-app` / `confluence_live` | `KNOBAS_CONFLUENCE_URL`, `KNOBAS_CONFLUENCE_USER`, `KNOBAS_CONFLUENCE_PASSWORD` | `live_session confluence`, likewise |
| `just atlassian-live` | `jira-live`, then `confluence-live`, under one hold of the JVM lock. Never both products at once | as those two | as those two |
| `just estate-live` | `knobas-app` / `estate_live`, two tests. The hcloud importer's round trip over the knobas Hetzner project. The Docker importer against the WSL box's engine through the `wsl` docker context (`wsl/connect.sh` makes it), where every running container must be in `estate.json`. No container started, no seed | `HETZNER_API_TOKEN`, `ESTATE_LIVE_HCLOUD_SERVERS`, and the docker CLI on the PATH | the repo-root `.env` (a read-only token for the knobas project), and the count on the command line |

What to expect from a run:

- **A recipe with nothing to run against fails.** Each one calls the `justfile`'s `_require-live-env` guard on its
  variables before it starts cargo, and stops naming the missing ones and where they come from. libtest counts a skipped
  test as passed, so without the guard an unset variable gives a green run in which nothing ran (#351). A set variable
  is still no server that answers: a wrong URL or a dead token fails inside the suite, as a connection error or a 401.
- **`just teamcity-live` says nothing about our TeamCity.** It reads the repo-root `.env` and by default points at
  JetBrains' public instance. The container here is `just teamcity-live-seeded`.
- **No gate runs the Forgejo form.** A change that `just gitea-live` exercises, a test or the adapter code under one, is
  run with `just gitea-live forgejo` as well (#2034).
- **`just estate-live` takes the server count on the command line.** The Hetzner project is empty since #634, so an
  ordinary run is `ESTATE_LIVE_HCLOUD_SERVERS=0 just estate-live`. A PR that touches the hcloud producer
  (`crates/knobas-app/src/assets/hcloud.rs` or the code it calls) owes a run at `=1`. Björn creates and deletes that
  server himself. No agent and no recipe does (the deputy's ruling on #634). Read a red Docker half that names a
  container first as `estate.json` being behind the engine.
- **Timings.** On this machine from empty volumes, `just jira-live` took 299 s and `just confluence-live` 91 to 104 s
  (#633). With the product already up, a run is the suites alone.

Two files in `crates/knobas-app/tests/` measure and certify nothing, and have no recipe: `slow_forge_live.rs` and
`slow_sources_live.rs` time the app's reads through a slow proxy. Run them by hand as their headers say. A suite run by
hand takes no lock.

## One environment, one owner at a time

A worktree has exactly one owner. That rule does not cover this Docker environment. There is one of it, shared by every
worktree on the machine, and two agents seeding or running live suites against it at once break each other. So claim the
environment before you run `./seed`, `./seed-kuma.sh`, `just kuma-live` or `just gitea-live-capped`, and say when you
release it.

The tooling enforces part of it. The Gitea recipes queue on the Gitea lock (#724) and the JVM recipes on the JVM lock
(#690), so those wait for each other without a claim. Nothing guards `just kuma-live`, `./seed`, `./seed-kuma.sh` or a
hand `docker compose` call. `just gitea-live-capped` holds the Gitea lock and still wants a claim, because a TeamCity
run takes no Gitea lock and may be cloning from the Gitea it caps.

What two users of the environment do to each other:

- **The Gitea suites clear leftover branches.** `live_gitea.rs` removes every `knobas-` branch in `payout-service`
  before it starts, which is how a killed run heals (#143). It cannot tell a sibling's live branch from a leftover, and
  `start-work-live` cuts branches with the same prefix.
- **`just gitea-live-capped` reconfigures the shared container.** Until its trap uncaps Gitea, every reader gets one
  entry per page. A run beside it fails on missing entries, which reads as an adapter defect and is not one.
- **`./seed-kuma.sh` deletes every monitor `monitors.json` does not name** (#441), except one named `scale-*`, which
  belongs to `just seed-scale kuma`. A monitor someone added in the UI, or that knobas created through *Create monitor
  for this asset* (#453), is gone on anyone's next seed, with no way back. Add a monitor that has to survive to
  `monitors.json`.
- **`just kuma-live` adds and deletes four monitors of its own**: `knobas-live-scratch`, `knobas-write-scratch`,
  `knobas-live-created` and `knobas-write-created`. It pauses and resumes them, and it releases and rebinds the canary.
  A sibling counting monitors mid-run counts them. The next `./seed-kuma.sh` removes what a killed run left.
- **A compose edit recreates a shared container.** A service whose definition changed is replaced on the next
  `docker compose --env-file images.env up -d`. Its volume survives. The cost is seconds of downtime for every run
  reading it.

Credentials no longer collide, because each worktree has its own:

- **The Gitea token** (#684). `seed-gitea.sh` keeps the token in this worktree's `seed-state.json` while Gitea answers
  200 to it on `/user`. On a 401, or with no token, it mints one named
  `knobas-seed-<worktree directory>-<checksum of its path>` and first deletes only tokens of that name. Any other
  answer, or none, is a refusal that changes nothing. The rule is `gitea-token.sh`, tested by `gitea-token-test.sh` in
  `just witness-unit`.
- **The Kuma API key** (#1484). `seed-kuma.sh` tries the key in `kuma-api-key` against `/metrics`. On 200 it keeps the
  key. On 401 or 403 it mints a new one under the same kind of name and deletes only keys of that name. On any other
  answer it refuses, because Kuma shows a key's clear text once and a re-mint on a guess throws away a key that may
  still work.

One token and one key per worktree that ever seeded stay on the instance until a `down -v`.

Kuma has one Docker host, `testenv-docker-proxy` (#978), which `./seed-kuma.sh` creates and repairs. It points at the
`docker-proxy` service: container reads only, no POST, no published port, on an internal network it shares with Kuma
alone. Do not mount `/var/run/docker.sock` into Kuma, which would give that container control of the Docker daemon. Do
not put another service on that network either, since container reads include every container's environment.

When something is off:

- A branch or a monitor vanished, or a count is one off. Somebody else is in the environment, and none of these failures
  says so. Find out who holds it and run again once they release it.
- `eval "$(./seed --env)"` exits non-zero and names a 401. This worktree's own token is gone, after a reset Gitea volume
  or a hand-deleted token. Run `./seed-gitea.sh` here.
- `/metrics` answers 401. This worktree's own key is gone. Run `./seed-kuma.sh` here.
- `seed-kuma.sh` says the key "can be neither kept nor replaced". The published port did not answer. Run
  `docker compose --env-file images.env up -d --wait uptime-kuma` and seed again.
- `just gitea-live` fails on missing entries. A capped run is under way, or one was killed before its trap ran. A plain
  `docker compose --env-file images.env up -d --wait gitea` puts the uncapped server back
  ([The capped-Gitea overlay](#the-capped-gitea-overlay)).

## What the seed **cannot** reproduce — read this before writing assertions

`fixtures/tidewater/work.json` records commit shas like `c90d11` and pull request numbers like `142`. The products
assign their own ids, so an assertion on a literal id tests the fixture and not the adapter. `seed-state.json` maps
every fixture id to the real one. Read it and never guess.

Gitea:

- **Assert by form and by title.** `gitea:tidewater/payout-service@<sha40>` is the key form the §4.2 contract fixes. The
  sha inside it is whatever git produced. `seed-state.json` maps each fixture short sha to the real 40-character sha.
- **Pull request numbers are best-effort.** `SEED_EXACT_PR_NUMBERS=1`, the default, uses up the preceding issue numbers
  with closed `(reserved)` placeholders so PR #142 really is #142. That is about 140 placeholder issues per repository.
  With `SEED_EXACT_PR_NUMBERS=0` the seed is fast, the pull requests land at 1, 2 and so on, and the mapping goes to
  `seed-state.json`.
- **Issues come back as tickets** (#1206). The seed files three issues in `ledger-api`, after every pull request so no
  pull request number moves: one with `Priority/High`, `Kind/Bug`, an assignee and a comment, one with `Priority/Medium`
  and nobody assigned, and one closed. Knobas mirrors them as tickets (`gitea:tidewater/ledger-api/issues/<n>`). The
  `(reserved)` placeholders are issues too, so they come back as closed tickets.

TeamCity (`./seed --teamcity`):

- **Run ids are the server's.** The fixture spells a run's id and number as one value, `412`. A real server numbers runs
  by a per-pipeline counter, which the seed sets, so the numbers come out as the fixture's. The ids are 1, 2, 3 and so
  on in the order the runs were queued. `seed-state.json` maps fixture number to real id under `teamcity.builds`. Assert
  `number`, look `id` up.
- **Timestamps are when the run ran.** The fixture's `when` and `duration` cannot be set on a real run. Assert that a
  finished run has a `finishDate` after its `startDate`, never a value.
- **The triggerer is `knobas`.** The seed's token queues every run, so `triggered.type` is `user` and the user is the
  seed's administrator. It is not the fixture's `mara` for 1188, and never a VCS trigger.
- **The running run is not running unless you asked.** `./seed --teamcity --running` holds `Payout_Build` 1188
  ([docs/real-products.md](docs/real-products.md)).

Jira: the live suites take back what they write, but nothing can undo the `PAY` key counter. Do not assume the fixture's
keys are the highest ones.

## Opt-in profiles

TeamCity, Jira and Confluence sit behind compose profiles, `real-teamcity` and `real-atlassian`. They are off by default
because they are expensive, and `images.env` gives the approximate costs. The live recipes and `just live-up` start them
for you. Read the page when you start one by hand, when a seed refuses an image digest, when Jira locks the admin
account after a wrong password, or when you need to know what the seeds put into each product, the Docs reader showcase
included.

See [docs/real-products.md](docs/real-products.md).

## A seeded pull request that is no longer open (#884)

`seed-gitea.sh` requires the pull requests the fixture says are open to be open on the server: "SEPA retry with
exponential backoff" and "Add payout CSV export".

| On the server it is | The seed |
| --- | --- |
| open | goes on |
| closed | reopens it (#2110). It first closes any other open pull request on the same head and base, or Gitea would leave the fixture one open pull request over |
| closed, and Gitea refuses the reopen because the head branch is gone | refuses, with Gitea's answer in the message |
| merged | refuses and prints the remedy below |

A merged pull request cannot be reopened, and a replacement would invent history on `main`, so the seed does not repair
it. The remedy is Gitea on a fresh volume:

```sh
docker compose --env-file images.env rm -sfv gitea
docker volume rm $(docker volume ls -q --filter name=gitea-data)
rm -f seed-state.json
./seed-gitea.sh
```

`./reset` does the same for every product. No live suite causes this, since each merges only the pull requests it opened
on `knobas-` branches (audited in #884). A person can, by pressing *Close* or merging a seeded pull request in the app
while it runs on the live Gitea. The rule is `gitea-pr-state.sh`, tested by `gitea-pr-state-test.sh` in
`just witness-unit`.

## The capped-Gitea overlay

`docker-compose.capped.yml` recreates the Gitea service with `[api] MAX_RESPONSE_ITEMS = 1`, so every listing answers
fewer entries than the `limit=50` the Gitea adapter asks for. It exists for one test,
`crates/knobas-source-gitea/tests/live_gitea_capped.rs`, which certifies that the adapter's paged walks end on an empty
page and not on a short one (#81, live-certified by #115). The default compose file cannot certify that, because every
corpus the seed creates fits in one page of 50.

```sh
just gitea-live-capped   # from the repository root: seed, overlay, the test, uncap from a trap
```

It reconfigures a container everyone shares, so claim the environment first
([One environment, one owner at a time](#one-environment-one-owner-at-a-time)). Three more things to know before you
touch it:

- **Seed first, uncapped.** `seed-gitea.sh` decides what exists by reading listings with `limit=50` and no paging.
  Capped to one entry it judges existing commits missing, posts a file again and dies on the 422. The recipe seeds
  against the default file and applies the overlay after. The container is recreated over the same volume, so the corpus
  survives.
- **The cap is sticky, and the default file is what removes it.** Gitea's `environment-to-ini` only ever writes
  `app.ini`, so dropping the variable does not remove the line. That is why `docker-compose.yml` pins
  `GITEA__api__MAX_RESPONSE_ITEMS: "50"`, Gitea's own default and the adapter's `client::PAGE_SIZE`. Do not remove the
  pin. Without it a plain `up` after a capped run comes back on a still-capped server.
- **Never commit to the default branch of `payout-service`** (#373). The test measures at most 19 entries per listing,
  and the seed leaves that branch at exactly 19 commits. One more turns `just gitea-live-capped` red, and only a
  force-push or `./reset` takes it back.

## Image pinning

No `:latest` anywhere in the compose file. `images.env` holds one `NAME=repo@sha256:...` line per image and is
committed, since it holds public digests and no secrets. The compose file interpolates each with a failing default, so a
missing pin stops `docker compose config`.

```sh
./pin-images.sh        # resolve the tags named in the script to digests, rewrite images.env
git diff images.env    # review it: a digest change is an environment change
```

`pin-images.sh` needs `docker buildx`. It resolves digests with `docker buildx imagetools inspect`, which reads the
registry manifest and downloads nothing. Pinning by `docker pull` would cost about 13 GB.

Three pins are guarded: `TEAMCITY_IMAGE` by `seed-teamcity.sh`, and `JIRA_IMAGE` and `CONFLUENCE_IMAGE` by
`seed-atlassian.sh`. Those seeds walk a first-start wizard and refuse any digest but the one the walk was derived on,
`VERIFIED_*_IMAGE` in the seed script.

- For a guarded pin the script writes the seed's digest. When the tag has moved on it prints `HOLDING` with both digests
  and exits 0, and the pin's comment line in `images.env` says it is held. Today `TEAMCITY_IMAGE` and `CONFLUENCE_IMAGE`
  are held.
- `./pin-images.sh --move CONFLUENCE_IMAGE` takes the new digest for that one variable. The seed then refuses its
  container until its wizard walk is derived again and its `VERIFIED_*_IMAGE` is set to the new digest. `--move` on an
  unguarded pin exits 2.
- `TEAMCITY_AGENT_IMAGE` is pinned by the server's version tag, a hand-kept copy that nothing reads for you. An agent
  ahead of the server is refused at registration (#267's finding, not run again since). Bump the tag in the same change
  as `--move TEAMCITY_IMAGE`.
- Every other pin resolves fresh on each run. `--out PATH` writes somewhere else, to compare.

Three tags are chosen, not the newest. Jira stays on the 10.3 line and Confluence on 9.2 because those start on a
timebomb licence, and Jira 11.x is reported to reject the key. The Atlassian database is PostgreSQL 15 because
Atlassian's matrix does not support 17 for Jira 10.3.

## Network access

The containers and the seeds run offline once the images are pulled. These steps reach the internet:

| Step | Reaches | When |
| --- | --- | --- |
| The `kuma-seed` container (`./seed-kuma.sh`, `./kuma-monitor.sh`) | npm, for `socket.io-client@4`, because Uptime Kuma v2 has no REST API for configuration | every run |
| `./fetch-timebomb-keys.sh` | Atlassian's public page of timebomb keys | every Jira or Confluence start with no key in the shell |
| `./docs-assets.sh fetch` | npm's registry, for one pinned Swagger UI tarball | once, then the copy is reused |
| `./pin-images.sh` | the image registries | when you re-pin |
| `just teamcity-live`, `just estate-live` | JetBrains' public TeamCity, the Hetzner API | every run |

No monitor leaves your machines. Five of the six watch the compose network or the engine's host, and `notebook` pings
the notebook's Tailscale address.

## The responsiveness probe's saved database

`just responsiveness-probe` (#753) times every read the shell makes against a seeded mirror of 22,000 items. Building
that mirror takes minutes, so the probe keeps the built database under `$TMPDIR/knobas-probe-snapshots/` and a run
restores a copy in seconds (#781). Read the page for how a snapshot is keyed, when to rebuild with `RP_FRESH=1`, the
smaller `RP_ITEMS` size to iterate on, `RP_PASS`, and how old snapshots leave the disk.

See [scale/README.md](scale/README.md).

## Björn's scale (`just seed-scale`, `just scale-sync`)

Björn's real sources hold about 5,000 Confluence pages, 20,000 Jira issues, 4,000 TeamCity projects, pipelines and runs,
and 10,000 Gitea items. The seed holds a few dozen of each. `just seed-scale <product>` fills a live product to his size
beside the seed, in projects, spaces and organizations of its own, so the live suites still find exactly what the seed
made. `just scale-sync <root>` syncs the filled products into a scratch profile and times each source's first sync. Read
the page before a fill: it lists what each one writes, how long it takes, and how to resume one that stopped.

See [scale/README.md](scale/README.md).

## The estate file

`estate.json` is the estate knobas models, written down: the notebook, the Windows desktop, the WSL VM and its Docker
engine, every container the compose file runs there, and the tunnelled ports as routes. It is a fixture with readers,
the Import and `--demo` among them, and tests in `just check` hold it to the compose file and to `monitors.json`.
`just estate-live` fails when a container runs on the box's engine that the file does not hold. Read the page before you
add a service to the compose file or change an address.

See [docs/estate-and-monitors.md](docs/estate-and-monitors.md).

## Monitors

`monitors.json` lists the six Uptime Kuma monitors, and it is the real estate too: a ping on the notebook, one http
monitor each for TeamCity, Jira, Confluence and Gitea, and the canary. `./seed-kuma.sh` owns the list and deletes what
the file does not name. The page says when each monitor reads red, how to use the canary (`./canary.sh`, the one thing a
live run may knock over), and how to read a monitor's state back from `/metrics`.

See [docs/estate-and-monitors.md](docs/estate-and-monitors.md).

## sccache on a dev machine

Björn's Mac builds through sccache as a machine-wide `rustc-wrapper`. The repository expects it and does not
require it. `just pre-gate` warns when the configured wrapper is missing or does not run, and when a niced
sccache server is slowing every compile.

[docs/build-and-gate-speed.md](docs/build-and-gate-speed.md) has the setup and both fixes, and the measurements
behind the test profile's settings, the gate's halves and the scratch-database template.

## Signed dev build (macOS)

Desktop notifications (M3.3, #290) need a `.app` bundle with a stable signed identity, and `tauri dev` produces neither.
The dev Mac signs a debug bundle with `knobas-dev`, a self-signed code-signing certificate (#273). No Apple identity is
needed. Read the page when `security find-identity -v -p codesigning` does not list `knobas-dev`, or when you build and
sign a bundle to check a desktop notification.

See [desktop-witness/README.md](desktop-witness/README.md).

## The desktop witness (macOS)

Open-in-editor, open-in-terminal and the global shortcut belong to the operating system, so there is no instance to run
a suite against. ADR-0016 makes their witness a scripted run against a real signed bundle:
`just desktop-witness <driver>` (#500). It takes the screen, needs the `knobas-dev` identity and an Accessibility grant,
runs one at a time, and is not part of `just check`. The gate carries `witness-unit` in its place. The page also covers
`just live-shot` (#1483), screenshots of the real app against live products.

See [desktop-witness/README.md](desktop-witness/README.md).

## Scripts

Everything is in `testenv/` unless the path says otherwise.

| Script | Does |
| --- | --- |
| `./seed` | Gitea and Uptime Kuma, in order, then prints the variables. `--env`, `--env-kuma` and `--env-forgejo` only print. `--atlassian` and `--teamcity [--running]` set up and seed those products |
| `./seed-gitea.sh` | Organization, teams, users, repositories, branches, commits, pull requests, comments, reviews and three issues. Sources `gitea-token.sh` and `gitea-pr-state.sh` |
| `./seed-forgejo.sh` | `seed-gitea.sh` pointed at Forgejo, with `seed-state-forgejo.json` as its state |
| `./seed-kuma.sh` | The Kuma admin account, the monitors, the Docker host and the API key. Deletes what `monitors.json` does not name, `scale-*` monitors excepted |
| `./kuma-monitor.sh` | `add <name> <url>` and `delete <name>` on one throwaway monitor, through the socket.io channel the seed uses. `just kuma-live` uses it |
| `./canary.sh` | `up`, `down` and `status` on the canary's host socket, `127.0.0.1:8299`. On a remote target it runs on that machine and binds its Docker bridge |
| `./stall.sh` | Remote target only: `up`, `down` and `url` on a socket that accepts and never answers, port 8298. `just kuma-live` binds and releases it |
| `./seed-teamcity.sh` | TeamCity's first start, an access token and one authorised agent. Then runs the script below |
| `./seed-teamcity-builds.sh` | The Tidewater projects, pipelines, VCS roots and runs in TeamCity. `--running` for the fixture's running run |
| `./fetch-timebomb-keys.sh` | Pulls the two 10-user, 3-hour Data Center timebomb keys off Atlassian's public page, checks each, and prints `export` lines. `--write` also puts them in the git-ignored `.env.licences` |
| `./seed-atlassian.sh` | The Jira and Confluence setup wizards, unattended. Takes `jira` or `confluence` to walk one |
| `./seed-atlassian-content.sh` | The Tidewater content in Jira and Confluence. Takes `jira` or `confluence`. `--verify` reads it back |
| `./docs-assets.sh` | `fetch` downloads Swagger UI once. `dir` prints the directory copied into the `docs-assets` container (#2456) |
| `./seed-scale.sh`, `./scale-sync.sh` | Fill one live product to Björn's scale, and sync the filled products into a scratch profile ([scale/README.md](scale/README.md)) |
| `./live` | `up` and `down` of `gitea`, `forgejo`, `teamcity`, `jira` or `confluence`, and `status`. Also `tunnel up`, `tunnel down`, `tunnel status`, `remote-check` and `refresh` for a remote target, and `trust-ca` for the https front |
| `./live-lib.sh` | What `./live` and the live recipes source: the target, the locks, `live_session`, the starts and the put-back |
| `./live-sources.sh` | `add` and `remove` of the six live sources on the demo profile |
| `./live.env.example` | The template for `live.env`, which says where the live products run |
| `./wsl/` | One setup script per machine for a WSL box, and `refresh-agent.sh`, the launchd agent that keeps Jira and Confluence licensed there ([wsl/README.md](wsl/README.md)) |
| `./check-ports.sh` | Asserts the compose file against the port map. Starts nothing |
| `./pin-images.sh` | Resolves image tags to digests into `images.env`. Guarded pins are held, and `--move VAR` takes a new one |
| `./reset` | `down -v` on every profile, and deletes `kuma-api-key` and `seed-state.json`. It leaves the canary bound and says so |
| `./desktop-witness.sh` | The desktop witness (#500). Takes the screen, not part of `just check` ([desktop-witness/README.md](desktop-witness/README.md)) |
| `./live-shot.sh` | Screenshots of the real app against live products (#1483), from a steps file. Takes the screen and the witness's lock. Same page, *Screenshots of the real app* |
| `./ui-checks/` | The press trips, the screenshot comparison and the real-window driver ([UI checks](#ui-checks)) |

These tests start no container and run in `just witness-unit`, so in `just check`:

| Script | Does |
| --- | --- |
| `./live-test.sh` | `live-lib.sh` against a stub `docker`: the one-JVM refusal, the docker context refusal, what a run puts back, that no run stops Gitea (#698), how `live.env` is read, a remote target's rules, the licence renewal and the one retry from empty |
| `./live-lock-test.sh` | The JVM and Gitea locks against a scratch lock: a second run waits, a dead holder's lock is taken over, the wait ends naming the holder, a `live-up` keeper holds until `live-down`. On a remote target, one lock per product and the memory section |
| `./gitea-token-test.sh` | A worktree's seed never revokes another worktree's token (#684) |
| `./gitea-pr-state-test.sh` | A merged seeded pull request is refused, a closed one reopened (#884, #2110) |
| `./seed-env-test.sh` | `./seed --env` with no Gitea block exports no Gitea variable (#982), and a refused token prints nothing (#1377) |
| `./seed-teamcity-builds-test.sh` | What makes `seed-teamcity-builds.sh` delete a run (#2480) |
| `./desktop-witness-test.sh` | The desktop witness's own logic in `desktop-witness-lib.sh`. No screen, no bundle |

One script outside this folder is of the same kind, since it measures the app and not a product.
`scripts/keep-alive-budget.sh <slot>`, or `just keep-alive-budget <slot>`, is keep-alive's budget and memory witness
(#2250). It runs headless Chrome against the `?fake-ipc` dev server on port `5300 + slot`, reused when one is up, with
debug port `9400 + slot`. It starts no product and is not part of `just check`.

- It times the return to a repository page's Files tab with a 10,000-line file open, after Commits: 7 runs, exit 1 when
  the median is over 50 ms (`KEEP_ALIVE_RETURNS`, `KEEP_ALIVE_BUDGET_MS`).
- Then it prints the JS heap after a forced garbage collection and the DOM node count with five pages held, at *Pages
  kept open* 0 and 5, twice each.
- It prints `scripts/machine-load.sh`'s line first. Quote a timing beside that line.

## UI checks

`testenv/ui-checks/` holds three checks the UX polish loop (#2479) ran on every round: the press trips, the
screenshot comparison and the real-window driver. None is a gate and none starts a product.

[ui-checks/README.md](ui-checks/README.md) has how to run each, and [perf/README.md](perf/README.md) the frame
and page-open probes.

## mockd's documented deviations

mockd is our deprecated mock of Jira Data Center and TeamCity (ADR-0013). It no longer runs as a container here. The
tests that still use it spawn the `knobas-mockd` crate in process (`knobas_mockd::spawn_*`), and it serves the same
Tidewater fixture.

It is stricter than the real products in several places on purpose, so that an adapter which passes against mockd passes
against them, never the reverse. The list lives in one place and must not be copied: the *Documented deviations* section
of the module docs in `crates/knobas-mockd/src/lib.rs`. Read it before you conclude that mockd is wrong.
