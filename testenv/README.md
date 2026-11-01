# `testenv` — the knobas test environment

A small fake company on a laptop: the Tidewater Freight fixture served by real
software (Uptime Kuma always; Gitea, TeamCity, Jira and Confluence started on
demand by the live recipes, one JVM product at a time, the runs queued on a
lock — see *Live products, on demand*). Since 2026-11-01 they run around the
clock on a WSL box, switched by `testenv/live.env` (*Live products on a remote
machine (WSL)*), and `estate.json` describes that box.

```sh
cd testenv
# every compose call passes the pins file: --env-file images.env (never a bare
# `.env`, which means a token and nothing else in this repo)
docker compose --env-file images.env up -d   # gitea, uptime-kuma
./seed                         # the Tidewater content, idempotent
```

Every command in this file runs in `testenv/`. An agent's Bash call starts at
the repository root, and the bash-guard refuses `cd testenv && …`, inside
`$(…)` too, so an agent prefixes each one with `env -C testenv`: `env -C
testenv ./seed`, `eval "$(env -C testenv ./seed --env)"`.

Uptime Kuma's monitor list is the real estate (see "Monitors" below), and
since #634 every value in it is literal: `./seed` needs nothing beyond this
directory. (From 2026-09-06 it needed a gitignored `hetzner/hosts.env` for
three Hetzner servers' addresses; the servers were decommissioned and
`testenv/hetzner/` went with them.)

One layer, and that is the point:

| Layer | What | Why |
|---|---|---|
| **Real products** | Gitea, Uptime Kuma v2 | Cheap to self-host, so the adapters are certified against the actual software rather than against our idea of it. |

mockd, our deprecated mock of Jira Data Center and TeamCity (ADR-0013), ran
here as a container until 2026-11-01; the compose service, its Dockerfile and
its two build-stage pins are gone. `crates/knobas-mockd` stays: the tests that
still use it spawn it in process (`knobas_mockd::spawn_*`), and it serves the
same Tidewater fixture (design §14a).

## Ports

Fixed by the M1 interfaces doc §5. Everything binds `127.0.0.1` only — this
environment ships developer-grade credentials and must never be reachable
off-host. `./check-ports.sh` asserts the compose file against this table and CI
runs it, so the table and the file cannot drift.

| Port | Service | Profile |
|---|---|---|
| 3000 | Gitea | default |
| 3001 | Uptime Kuma v2 | default |
| 3002 | Forgejo (#1195) | `--profile forgejo` |
| 8111 | real TeamCity server | `--profile real-teamcity` |
| 8080 | real Jira Software | `--profile real-atlassian` |
| 8090 | real Confluence | `--profile real-atlassian` |

One host port is used here that is **not** in that table and must not be:
`127.0.0.1:8299`, the canary (below). It is a host socket a helper script
binds, not a published container port, so `check-ports.sh` has nothing to
assert about it.

Five ports were once reserved here and all are free now. 8211, reserved for
a Confluence mock until 2026-09-03, is unreserved by ADR-0013; the real
Confluence on 8090 is the witness. 8213, reserved for a low-code-runtime
stub until 2026-09-06, is unreserved because that feature left the plan
(roadmap §2 M4). 8200, 8210 and 8212, mockd's admin, Jira and TeamCity
ports, are free since 2026-11-01, when the mockd container left the compose
file; the tests that spawn the crate in process do not use them.

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
`seed-state.json` — which is Gitea's, and a Kuma suite has no business
starting and seeding Gitea. `just kuma-live` uses that form for exactly that
reason. Once `./seed
--teamcity` has run it also prints `KNOBAS_TEAMCITY_URL` and
`KNOBAS_TEAMCITY_TOKEN`; the seeded TeamCity suite additionally reads
`seed-state.json` for the fixture-number-to-id map, from
`testenv/seed-state.json` relative to the crate unless
`KNOBAS_TEAMCITY_SEED_STATE` names another path.

## Live products, on demand

**Björn's decision, 2026-10-12** (issue #633): the real products a live suite
runs against — TeamCity, Jira, Confluence and Gitea — run **on this machine,
on demand, one at a time**, started when a live suite needs one and stopped
after it. *"Since we focus on one source now at a time we don't have to run
them 24/7, so we can just spin one up when we have to test."* It supersedes,
for live suites, the 2026-09-06 move of the three JVM products to Hetzner,
and those servers were decommissioned after it (#634); ADR-0013 is unchanged — the witness is still the real product, now in the
notebook's Docker instead of on a server.

Every live recipe does it itself, so the whole of it is usually just:

```sh
just teamcity-live-seeded   # Gitea + TeamCity up, seeded, two suites, TeamCity stopped again
just jira-live              # Jira + its PostgreSQL, wizard, content, three suites, removed again
just confluence-live        # the same for Confluence
just atlassian-live         # jira-live, then confluence-live -- never the two at once
just gitea-live             # also gitea-live-capped and start-work-live
just gitea-live forgejo     # the same suite against Forgejo (below)
```

For a session of several runs, start the product once and the recipes leave
it up — **a run puts its product back as it found it**: if the run started
it, it stops it, and if it was already up it stays up. Gitea is the one
exception: **no run stops Gitea, only `just live-down gitea` does** — not a
Gitea run that started it, not a TeamCity run that brought it up (see *Gitea
outlives the run that started it* below):

```sh
just live-up jira           # = testenv/live up jira
just jira-live              # ~60 s instead of ~5 min: the suites
just live-down jira         # = testenv/live down jira
just live-status
```

| Product | `live-up` starts | and seeds with | `live-down` |
|---|---|---|---|
| `gitea` | `knobas-gitea` | `seed-gitea.sh` | stops it |
| `forgejo` | `knobas-forgejo` (`--profile forgejo`, #1195) | `seed-forgejo.sh` | stops it |
| `teamcity` | Gitea (the VCS roots clone it), then `knobas-teamcity` and `knobas-teamcity-agent` | `seed-teamcity.sh` (wizard, token, agent, builds) | stops the server and agent; Gitea stays |
| `jira` | `knobas-jira-db`, `knobas-jira` | timebomb key, `seed-atlassian.sh jira`, `seed-atlassian-content.sh jira` twice, `--verify` | removes both **and their volumes** |
| `confluence` | `knobas-confluence-db`, `knobas-confluence` | the same, for `confluence` | removes both **and their volumes** |

**One JVM product at a time.** The Docker VM here has 8 GB; Jira wants ~4 GB,
Confluence ~2 GB, TeamCity's server and agent ~2.2 GB, and two JVMs claiming
their heaps at once is what killed #313's windows. `live-up` (and every live
recipe) refuses TeamCity, Jira or Confluence while another of the three is up,
in one line that names it:

```text
live: knobas-teamcity is running; one JVM product at a time on the 8 GB Docker VM -- 'just live-down teamcity' first
```

It never stops the other product itself — that is somebody's session going
away under them. Gitea is not a JVM and is never refused.

**Several JVM tickets at once; their live runs take turns (#690).** The
refusal above is the backstop, not the queue. Every recipe that starts or
uses a JVM product — `teamcity-live`, `teamcity-live-seeded` (which runs the
app's TeamCity suite too), `jira-live`, `confluence-live`, `atlassian-live`,
and `live-up teamcity|jira|confluence` — first takes one machine-wide lock,
`$TMPDIR/knobas-live-jvm.lock`, and holds it for the whole run: start, seed,
suites, and the closing `live-down`. A run that finds it held **waits**,
saying once who holds it and since when:

```text
live: teamcity-live-seeded: the JVM lock is held by 'teamcity-live-seeded' in /Users/dev/knobas/.worktrees/issue-683 (pid 51234, since 14:02:11); waiting up to 2700s
live: teamcity-live-seeded: took the JVM lock after 312s
```

and goes on when it is released, so it never meets the refusal the first
run's product would cause. After 45 minutes (`KNOBAS_LIVE_LOCK_WAIT`, in
seconds) it gives up and refuses, naming the holder. A lock whose holder is
dead — a SIGKILLed run — is taken over by the next taker, which says so;
anything else is waited for. So several JVM tickets may be implemented at
once, and only the few minutes each spends in TeamCity are serialised. The
lock is a symlink whose target names the holder (`<pid> <epoch> <recipe>
<worktree>`), taken with `ln -s` as the gate slots are (macOS has no
`flock`); `readlink` it to see who holds it. `atlassian-live` holds it once
for both of its halves, which run under it. A product started by hand with
`docker compose` takes no lock; that is what the refusal is still for.

**A `live-up` JVM product keeps the lock until `live-down` (#2121).** A suite
run by hand against it (`cargo test … --test live_confluence_seeded`) takes no
lock, so when `live-up` let the lock go after seeding, another worktree's
`just confluence-live` took it, found Confluence up and edited the same pages
mid-run. Now `live-up teamcity|jira|confluence` hands the lock to a detached
keeper that holds it as `live-up-<product>-kept` in its worktree while the
product is up. That worktree's own recipes and hand-run suites go on under
it (`… runs under the JVM lock this worktree's live-up keeps`); every other
worktree's run waits as for any holder, its waiting line ending in `'just
live-down <product>' from any worktree frees it`. `live-down` ends the keeper
and frees the lock, from any worktree; a product gone some other way (removed
by hand, a VM restart) frees it at the keeper's next look
(`KNOBAS_LIVE_KEEPER_POLL`, 30 s); a killed keeper is a dead holder, taken
over. So run `just live-down <product>` when the session is over: until then
the other worktrees' JVM runs queue behind it. A forgotten one is bounded:
the keeper also lets go once its worktree is removed, or after
`KNOBAS_LIVE_KEEPER_MAX` (3 hours, the Atlassian licence's life), leaving
the product up for the next run.

**Gitea runs take turns on a lock of their own (#724).** Two `just
gitea-live` runs from two worktrees broke each other: one run's cleanup
deleted its own `knobas-edit-…` branch while the other run's full sync was
walking the repository, and the suites that sweep the whole fixture assert on
counts and states another run is changing. So `gitea-live`,
`gitea-live-capped` and `start-work-live` hold `$TMPDIR/knobas-live-gitea.lock`
for their whole run, through `live_session gitea`: the same mechanism as the
JVM lock (one implementation in `live-lib.sh`, `live_lock_hold <recipe>
gitea`), the same waiting line with "Gitea lock" in it, the same stale-holder
takeover and 45-minute refusal. It is a separate lock on purpose: a Gitea run
needs none of the VM's memory, and queueing a two-minute Gitea suite behind an
Atlassian run would serialise what never collided. A TeamCity run starts and
seeds Gitea without it (the seed writes nothing to a seeded Gitea), and so
does `live-up gitea`.

**Forgejo, as a Gitea source (#1195, spec #1192 story 18).** Forgejo is the hard
fork of Gitea and is expected to work through the same adapter. `just
gitea-live forgejo` is the one switch: it starts the pinned Forgejo image
(`FORGEJO_IMAGE` in `images.env`, version 17.0.0 and its digest; profile `forgejo`, port
3002, volume `forgejo-data`), seeds it with `seed-forgejo.sh` (the Gitea
seed, `KNOBAS_SEED_SERVICE=forgejo`, its state in the git-ignored
`seed-state-forgejo.json` so Gitea's token and id map are never touched), prints
the `KNOBAS_GITEA_*` variables from `./seed --env-forgejo` (plus
`KNOBAS_GITEA_SEED_STATE`), runs the same four suites as `just gitea-live`, and
stops Forgejo again if the run started it. It takes the Gitea lock. By hand:
`just live-up forgejo`, `just live-down forgejo`. Settings differ from Gitea's in
one place: `FORGEJO__migrations__ALLOW_UNENCRYPTED`, because Forgejo refuses the
seed's http pull mirror otherwise. `gitea-live-capped` and `start-work-live`
have no Forgejo form.

**Teams and languages (#1045).** The seed puts each repository of
`fixtures/tidewater/work.json` in its `team` (an organization team with no
members, beside *Owners*, which knobas leaves out) and, for a `lang` that is a
programming language, commits one source file to `main` (`src/main.rs` for
Rust, `src/main/kotlin/Main.kt` for Kotlin) so Gitea's language statistics
find it; Markdown counts for nothing and gets no file. Both run before the
seed cuts any branch, and both are skipped when already there.

**Gitea outlives the run that started it (#698).** No recipe stops Gitea at
the end of its run; only `just live-down gitea` does. A run that found Gitea
down — a `just gitea-live` (or `gitea-live-capped`, `start-work-live`), or a
TeamCity run, whose VCS roots clone it — starts it and leaves it up, and says
so in one line (`live: gitea was started for this run and stays up after it
…`). A TeamCity run takes no Gitea lock, so by the time that run ends a
TeamCity run or another Gitea run from another worktree may be using it, and
stopping "what the run started" would pull Gitea out from under that run
mid-suite. The other fix — counting Gitea's users with a marker per run and
stopping it when the last one leaves — was passed over for the simpler rule,
because nothing needs Gitea down: it is light (~130 MB, no JVM), the one-JVM
rule does not count it, and its stopped volume would come back seeded anyway.
Held to it by `testenv/live-test.sh`, which runs a whole `live_session
teamcity` against a stub `docker` while a Gitea session starts beside it, and
a whole `live_session gitea` from an empty engine.

**Why `down` differs by product.** TeamCity and Gitea are *stopped*: their
volumes hold the seeded builds and corpus, and the next start finds them
seeded in seconds. Jira and Confluence run on 3-hour timebomb licences that a
restart does not renew, so a kept volume is an expired product; they are
removed with their volumes, and their block in `seed-state.json` goes too, so
`./seed --env` stops printing a URL nothing answers. The removal is `docker
compose rm` plus the two named volumes, **not** `down -v`: a `down` with
services named still removes the project network when no *running* container
is on it, and a stopped Gitea, TeamCity or Kuma is still attached to it by id
— the next `up` of any of them then fails with *"network … not found"* (seen
on #633's first runs; the cure for a container in that state is `docker
compose up -d --force-recreate <service>`, then `stop` and `up` once more for
OrbStack's port forward).

**Never a stray remote engine.** `live-up` refuses a docker context named
`knobas-*`: until #634 those reached the three Hetzner servers over ssh, and a
context left behind on a machine would send these calls somewhere else. The
one remote engine allowed is a declared one, in `testenv/live.env` (next
section). The
three product monitors check the products at their published ports, so they
are red whenever their product is stopped. See "Monitors" below.

**The one real system that is not a container is Hetzner Cloud**, which the
hcloud importer reads. Its project is empty since #634, so `just estate-live`
runs at `ESTATE_LIVE_HCLOUD_SERVERS=0`; a PR that touches the hcloud producer
owes a run at `=1`, and that server is spun up the same way a product is — on
demand, for the one run — except that Björn creates and deletes it himself: no
agent and no recipe does (the deputy's ruling on #634).

`testenv/live-test.sh` holds the refusals and the put-back rule to account
against a stub `docker`, and `testenv/live-lock-test.sh` the lock — a second
run waits and goes on, a dead holder's lock is taken over, the wait ends in
a refusal naming the holder, `live_session` takes the lock before the guard
and lets it go after the put-back, a `live-up` product's keeper holds it
against other worktrees until `live-down`, and a Gitea session queues on
the Gitea lock and never on the JVM one — both in `just witness-unit` and so
in `just check`.

## Live products on a remote machine (WSL)

**Björn, 2026-11-01:** the live products may run **24/7 on a WSL box** instead
of on demand in the notebook's Docker, so the whole live suite is open around
the clock and no JVM run waits for another product to be removed first. It is
a switch, not a replacement: one file says where the products are, and with no
file everything above holds unchanged.

**The switch is `testenv/live.env`** (git-ignored; `live.env.example` is the
template), in the **main checkout**: every worktree reads that one file
(its own `testenv/live.env` first, if it has one). It holds `KNOBAS_LIVE_*`
lines only and is read, never sourced. A variable set in the environment wins
over the file, per variable, so `KNOBAS_LIVE_TARGET=local just jira-live` runs
one suite here while the file names the WSL box, and the unit tests pin
`local` whatever the file says.

| Variable | Default | Means |
|---|---|---|
| `KNOBAS_LIVE_TARGET` | `local` | `local`: this machine's Docker, on demand, as above. `remote`: the machine below. |
| `KNOBAS_LIVE_SSH` | — | The remote machine's ssh host. Required for `remote`. |
| `KNOBAS_LIVE_TUNNEL_PORTS` | `3000 3002 8080 8090 8111` | Ports forwarded from 127.0.0.1 here to 127.0.0.1 there. |
| `KNOBAS_LIVE_ONE_JVM` | `1` local, `0` remote | The one-JVM refusal. |
| `KNOBAS_LIVE_LICENCE_MARGIN_S` | `3600` | Jira and Confluence are set up again when their licence has less left than this (both targets). |
| `KNOBAS_LIVE_MEM_BUDGET_MB` | engine memory − `KNOBAS_LIVE_MEM_RESERVE_MB` (1536) | Remote only: what the products' caps may add up to (below). |
| `KNOBAS_LIVE_{GITEA,TEAMCITY,TEAMCITY_AGENT,JIRA,CONFLUENCE,DB}_MB` | 512, 2048, 1024, 4608, 3072, 512 | Remote only: each container's memory cap (`docker-compose.remote.yml`), a ceiling against a runaway. |
| `KNOBAS_LIVE_USE_{GITEA,TEAMCITY,JIRA,CONFLUENCE}_MB` | 512, 2048, 4096, 2560 | Remote only: what the budget counts per product (database and agent included), measured with headroom. |

**How a remote target works.** Nothing in the suites changes, and no checkout
lives on the remote machine:

- `live-lib.sh` exports `DOCKER_HOST=ssh://$KNOBAS_LIVE_SSH`, so every
  `docker` and `docker compose` call (the recipes, the seeds, and the one
  suite that runs `docker exec`, `live_jira_seeded`'s tied-`created` stamp)
  goes to the remote engine. Compose still reads `docker-compose.yml` and
  `images.env` from the worktree, so the remote runs the pins of the tree
  under test.
- One ssh master (`testenv/live tunnel up`, opened by any live run that
  finds it down, left running) forwards the product ports from this
  machine's loopback to the remote's. The products stay bound to the remote's
  127.0.0.1, and every URL the seeds, `./seed --env` and the suites use is the
  one they use locally. A local product still holding one of those ports
  stops the tunnel from opening, and the refusal names the port.
- The seeds run here, as always, so each worktree's `seed-state.json` is its
  own and is written by the seed that just ran against the remote product.
- **No run stops a product when it ends** (`live_to_put_back` answers
  nothing), and the one-JVM refusal is off: products stay up until the
  memory budget (below) needs the room for another.
  `just live-down <product>` still works, through the same engine.
- The locks stay on this machine (`$TMPDIR`): the worktrees that take turns
  are all here. **On a remote target each JVM product has its own lock**
  (`knobas-live-jira.lock`, `-confluence`, `-teamcity`; Björn, 2026-11-01).
  A Jira run, a Confluence run and a TeamCity run from three worktrees go at
  once, and two runs on one product take turns, with the same waiting line,
  45-minute refusal and dead-holder takeover. The machine-wide JVM lock existed
  for the 8 GB VM's memory, which the budget below now guards. Its other job,
  two runs in one product's data, is per product. `atlassian-live` takes no
  lock there, because each half takes its own. A `live-up` keeper keeps its
  product's lock. The Gitea lock is unchanged. The local target keeps the
  one JVM lock.
- **The tunnel holds this machine's product ports.** A local run beside it
  would fail to publish them, or talk to the remote product through it. So a
  local run refuses while a tunnel is open (`testenv/live tunnel down` frees
  the ports, from either target). A worktree whose code predates the remote
  target has no such guard. Until every worktree runs this code, a remote run
  can collide with a local one, as it did on the first day.
- **Uptime Kuma follows the target too** (Björn, 2026-11-01: *"move it to
  wsl. i want to completely turn off docker … on this mac"*). `just kuma-live`
  sources `live-lib.sh`, so Kuma, its Docker API proxy and the seed's node
  container run on the remote engine, and the tunnel brings 3001 here. The
  seed's scripts reach their container as a tar on stdin, not through a bind
  mount, which would name a path on the engine's machine. Two host sockets go
  there as well, because a remote Kuma resolves `host.docker.internal` to its
  own machine's Docker bridge (172.17.0.1), and a container cannot reach a
  socket bound to that machine's 127.0.0.1:
  - **the canary** (`canary.sh`), which runs itself on that machine over ssh,
    fed on stdin, and binds the bridge address, still off the network. Only
    `up`, `down` and `status` cross to the remote shell.
  - **the stall** (`stall.sh`, port 8298): a socket that accepts and never
    answers. `live_kuma`'s refresh-waits witness needs one. Locally the suite
    holds its own; on a remote target `kuma-live` binds this one and names it
    in `KNOBAS_KUMA_STALLED_URL`.

  With a remote target this machine runs **no container at all**. The gates
  need none either: `check-ports.sh` only resolves the compose file, and the
  shellcheck recipe uses the local binary when its version is the pin. Both
  were run with OrbStack stopped.

  The monitors work from a remote Kuma (since 2026-11-01): the three JVM
  checks name their compose services (`http://teamcity:8111/login.html`,
  `http://jira:8080/status`, `http://confluence:8090/status`), as `gitea`
  always did, so they reach the products on the compose network rather than
  through `host.docker.internal`, where the products (bound to 127.0.0.1) do
  not answer. `notebook` pings the Mac's Tailscale address, `100.92.199.117`.
  On a freshly seeded Kuma that ping takes long enough to fail that a full
  sync in the first minute may not see `notebook` yet. A second run does.

  **`just estate-live` reads the WSL box's engine.** `testenv/estate.json`
  describes the box (*The estate file*, below), and its Docker half reads
  `docker --context wsl`, the ssh context onto `knobas-wsl` that
  `testenv/wsl/connect.sh` creates, and requires every running container
  there to be in the file.

**The box has 16 GB, so nothing may freeze it.** A VM pushed into swap stops
answering ssh. There are three layers, each closing a different way to that:

1. **Windows keeps its share.** `setup-windows.ps1` gives WSL physical memory
   less 6 GB (10 of 16), 4 GB of swap and `autoMemoryReclaim=gradual`. WSL
   cannot take more than its cap, whatever runs inside.
2. **Every container has a hard cap** (`docker-compose.remote.yml`, layered on
   through `COMPOSE_FILE` for every compose call a remote run makes) and a heap
   pinned under it: Jira 4.5 GB with `-Xmx2048m`, Confluence 3 GB with
   `-Xmx1024m`, TeamCity 2 GB with `-Xmx1536m` plus 1 GB for its agent, each
   PostgreSQL and Gitea 512 MB. The caps are a third above measured use. A JVM
   that grows takes out its own container, not the VM. A first, tighter 3.5 GB
   held Jira at its ceiling and pushed it into swap. `check-ports.sh` asserts
   the overlay changes no port.
3. **A memory budget decides what runs at once.** Before a product starts, what
   each product up uses is added to what this one will use. Those are
   **measured** figures with headroom, not the caps: Jira with its database
   4096 MB, Confluence 2560, TeamCity with its agent 2048, Gitea 512. While
   the sum is over the budget (the engine's memory less 1536 MB), the other
   JVM products are **stopped**, each only under its own lock taken for
   the stop (one another run holds is in use and is skipped),
   the one started longest ago first, until it fits. They are stopped rather
   than removed: TeamCity keeps its builds, and Jira and Confluence their seed
   until their licence is due. A product that cannot fit is refused in a line
   naming the numbers. Deciding what fits and starting the containers (not
   seeding them) happens in one short **memory section**
   (`$TMPDIR/knobas-live-mem.lock`). Two runs starting two products at once
   would otherwise each count the other as down and both find room. On the real box's 12 GB WSL (budget 10425 MB) all three fit
   (9216). With a 10 GB WSL any two fit and a third stops one. `testenv/live
   remote-check` prints the budget and what fits.

   **Measured with all three up**, a Jira suite running, on the real box
   (2026-11-01): Jira 3.48 GB, Confluence 2.27, TeamCity 1.22 and its agent
   0.40, each PostgreSQL under 0.2. The VM peaked at 8.5 of 12 GB used, with
   3.2 GB available, and the 49 Jira tests passed.

**The 3-hour licence is renewed when it is due.** A product kept up meets the
timebomb licence's end, which a run that removed it afterwards never did. So
before Jira or Confluence is started or re-seeded, by a run or by `live-up`,
the age of its **data volume** is read. The volume is created before the
wizard applies the key, so the estimate errs early, and only a reset removes
it. The container's own age is wrong for this: compose recreates a container
whenever its configuration changes (a moved pin, a raised cap), and a
recreated Jira would look new on an old licence. With less than
`KNOBAS_LIVE_LICENCE_MARGIN_S` left it is removed with its volumes and set up
from empty, about five minutes for Jira and two for Confluence; otherwise it is
re-seeded, which writes nothing to a seeded instance. A stopped container a
reboot left behind is the same case. The caller holds the JVM lock, so no
other run is inside the product while it is replaced. This holds on the local
target too, where a `live-up` session past two hours used to end in an
expired product.

**Before the content seed, the REST API must answer.** `/status` says RUNNING
while the plugins behind the API are still warming. A Jira restarted from a
stopped container answered `/rest/api/2/myself` with 503 for a while, and the
seed that hit it was retried from empty for nothing. So the start waits until
the admin gets a 200 from it (`KNOBAS_LIVE_API_WAIT_S`, 300).

**A seed that fails on an instance that was already there is retried once,
from empty.** That covers data a killed run left half-changed, an account Jira
locked after wrong passwords (*Never send this Jira a wrong password*, above),
and a licence that ran out earlier than its age said. A seed that fails from
empty is reported as the failure it is. Jira and Confluence log in with the
admin password, not a token, so a fresh instance needs no new credential. The
Gitea token is checked by `./seed --env` and minted again by the seed (#1377).

### Setting it up

Three scripts in `testenv/wsl/`, one per machine:

1. **Windows**, in an elevated PowerShell:
   `powershell -ExecutionPolicy Bypass -File setup-windows.ps1 -Distro Ubuntu`.
   It sets `memory` (physical less 6 GB), `swap=4GB`,
   `networkingMode=mirrored` (so WSL's sshd answers on the Windows box's own
   address), `vmIdleTimeout=-1` and `autoMemoryReclaim=gradual` in
   `%UserProfile%\.wslconfig`. An existing file is backed up and keeps its
   other keys. It refuses a `-MemoryGB` that leaves Windows under 4 GB. It adds a firewall rule for port 2222 and a scheduled task
   that starts the distribution at boot and keeps it open. Then `wsl
   --shutdown`. Set the box never to sleep.
2. **WSL**: `bash setup-wsl.sh --key '<public key>' [--port 2222]`. It turns
   systemd on (first run: `wsl --shutdown` and run it again), installs
   `docker.io` and `openssh-server`, puts the user in `docker`, and runs sshd
   on 2222 with keys only. Docker Desktop's engine is refused as a 24/7 host,
   because it runs only while Docker Desktop does.
3. **Mac**: `testenv/wsl/connect.sh <address> [port] [user]`. It makes
   `~/.ssh/knobas-wsl` (no passphrase: the runs are unattended, and a security
   key asks for a touch per login) and prints the public key for step 2. It
   writes a `Host knobas-wsl` block with a ControlMaster to `~/.ssh/config`,
   because every docker call is an ssh login. It writes `testenv/live.env` in
   the main checkout only once the login works, then runs `testenv/live
   remote-check`: ssh, the engine, the memory budget, the tunnel.
   `connect.sh --local` removes `live.env` and the tunnel, and the products
   run here again.

If mirrored networking is not available, Tailscale inside WSL does the same
job: use its address in step 3. Before switching, stop any product still up
locally (`just live-down <product>`). Its port would keep the tunnel from
opening.

**Measured, 2026-11-01**, against an OrbStack Ubuntu machine (arm64, 7 GB)
standing in for WSL and set up by `setup-wsl.sh`. `just gitea-live` (70
tests) and `just gitea-live-capped` were green. `just confluence-live` was
green: 36 tests, seeded and verified at 118 s, Confluence left up. A second
run with `KNOBAS_LIVE_LICENCE_MARGIN_S=10800` removed the running instance as
due, set it up from empty and was green again. On a second stand-in, the caps
landed (Gitea and each PostgreSQL 512 MB, Confluence 2560 MB running
`-Xmx1024m`). Under a forced 3600 MB budget, starting Forgejo was refused while
Confluence's `live-up` kept it, and stopped Confluence once nothing did. A
`just confluence-live` then brought it back from its stopped container,
re-seeded rather than rebuilt, and was green.

**On the real box, 2026-11-01** (Ubuntu 26.04 in WSL 2, 12 cores, 12 GB,
docker.io 29.1.3, reached over Tailscale; budget 10425 MB): `just gitea-live`
70 passed (265 s); `just jira-live` 49 passed (559 s, seeded from empty in
299 s); `just confluence-live` 36 passed (242 s, seeded in 147 s); `just
teamcity-live-seeded` 39 passed (559 s). TeamCity, the third JVM product,
printed `teamcity needs 3584 MB; 11264 MB is over the 10425 MB budget --
stopping jira` and ran beside Confluence. `just kuma-live`, on the Mac as
always, 21 passed. With per-product locks, `jira-live`, `confluence-live` and
`teamcity-live-seeded` started together from three worktrees and none waited
on another. They finished in 179, 259 and 332 s, all green (49 + 36 + 39
tests), about 5.5 minutes for what took about 23 in turn. `setup-windows.ps1` has not run: this box's WSL already had
12 GB and systemd.

## Live products as sources (`just live-sources`, #2371)

`just live-sources` adds one source per live product to the demo profile:
`live-gitea`, `live-forgejo`, `live-jira`, `live-confluence`, `live-teamcity`
and `live-kuma`. Addresses and credentials come from `./seed --env`,
`--env-forgejo` and `--env-kuma`, so the target (`live.env`: local, or the WSL
box through the tunnel) is whatever seeded them. Gitea and Forgejo sync the
seeded owner's repositories (`tidewater`), Jira the seeded projects, Confluence
the seeded space. A product with no seed, or with nothing answering at its
address, is skipped with one line; the rest are still added. Each line says
whether the source was added or updated and what its credential test said.

```sh
just live-sources                       # target/debug/knobas-app, then the demo profile
just live-demo                          # the same, then `just demo`
just live-demo-bundle path/to/knobas.app    # the bundle's own binary, then opens it --demo
just live-sources-remove [knobas.app]   # the six sources off again, with what they synced
```

It runs `knobas-app live-sources apply --demo` (a debug build only), which goes
through the app's own `crud::add` or `crud::update`, then `crud::set_secret`:
the credential is stored under the demo profile's keychain service and tested,
as the Settings form does. Running it again updates every source in place, so
a token `just live-up gitea` minted again is picked up by running it again.
The sources go in on stdin, never in argv.

- **Quit any demo knobas first.** The subcommand starts the demo profile's own
  database, which one process holds at a time.
- **Run it with the binary that will read the credentials.** macOS lets the
  binary that wrote a keychain item read it without asking, and asks for any
  other. `just demo` and a bundle share the demo profile's keychain service, so
  pass the bundle when that is what you will open. A `just demo` that rebuilds
  the binary asks once, as it does for any source.
- **A stale credential ends the run non-zero, naming it.** A token the
  product refuses still saves the source (a re-run updates it), but the line
  says `credential unauthorized` and the run exits 1 listing the sources not
  connected, so `just live-demo` stops before the app opens. A recorded Gitea
  token Gitea refuses makes `./seed --env` print nothing, so the run says the
  seed refused and why, skips Gitea, Jira, Confluence and TeamCity, adds the
  rest, and exits 1. Either way: `just live-up <product>` in the same checkout
  mints fresh credentials (seed state is per worktree), then run it again.
- `--data-dir <dir>` on `testenv/live-sources.sh` puts the profile in a scratch
  directory; the keychain service stays the demo profile's.

## The live recipes

Every live suite has a `just` recipe, and the recipe is how it is run — a suite
with no recipe is one nothing runs, which is how
`crates/knobas-app/tests/start_work_live.rs` came to be carrying an assertion no
implementation could fail (issues #347, #350). Each recipe's own header in the
`justfile` says what it certifies and what it writes; this table says which
variables gate it and where those come from.

| Recipe | Suite | Gated on | From |
|---|---|---|---|
| `just gitea-live` | `knobas-source-gitea` / `live_gitea` — the adapter against the shapes interfaces §4.2 fixes | `KNOBAS_GITEA_URL`, `KNOBAS_GITEA_TOKEN` | `live_session gitea`: Gitea started if down, `./seed-gitea.sh`, `eval "$(./seed --env)"`; Gitea stays up afterwards (only `just live-down gitea` stops it) |
| `just gitea-live-capped` | `live_gitea_capped` — paged walks against a server capped to one record | as above | as above |
| `just start-work-live` | `knobas-app` / `start_work_live` — the start-work flow over this Gitea: ticket → branch → PR → link → status and back, and then that pull request on the standup digest (M3.3's digest witness for Gitea). Half of it is a mock, so it is **not** M2 exit criterion 1's certificate; the recipe's header says which half | as above | as above |
| `just teamcity-live` | `knobas-source-teamcity` / `live_teamcity` — the adapter against the **public JetBrains** instance, read-only | `KNOBAS_TEAMCITY_URL` | the repo-root `.env`: `cp .env.example .env` |
| `just teamcity-live-seeded` | two suites: `knobas-source-teamcity` / `live_teamcity_seeded` — the adapter against **our** seeded TeamCity — and then `knobas-app` / `teamcity_seeded_live`, read-only, which is M3.3's digest witness for this source (a seeded build the mirror attributes to the reader is on the digest for the day it ran) | `KNOBAS_TEAMCITY_URL`, `KNOBAS_TEAMCITY_TOKEN` | `live_session teamcity`: Gitea and TeamCity started, seeded, and TeamCity stopped again; Gitea stays up (the recipe does it) |
| `just kuma-live` | three suites: `knobas-source-kuma` / `live_kuma` — the Uptime Kuma adapter against **this** container: the estate mirrored, an idle poll that emits nothing, a monitor really deleted and reported gone, a wrong API key's 401, and the write half (#452) — a scratch monitor paused through `pause_monitor`, gone from the next poll, resumed back — then `knobas-app` / `kuma_write_live`, the same write half one seam higher: the pause queued through `submit_write`, settled, and read back out of `sync.item` — and then `knobas-app` / `alert_chain_live`, **M4.1's exit witness**: the canary released, the alert in the inbox because a context holds its asset, acked, the port rebound, the alert closed. The recipe binds the canary first and rebinds it from a trap | `KNOBAS_KUMA_URL`, `KNOBAS_KUMA_API_KEY`, `KNOBAS_KUMA_USER`, `KNOBAS_KUMA_PASSWORD` | `./seed-kuma.sh` then `eval "$(./seed --env-kuma)"` (the recipe does both) |
| `just jira-live` | `knobas-source-jira` / `live_jira_seeded`, the Jira half of `knobas-app` / `atlassian_live`, and `knobas-app` / `share_exit` | `KNOBAS_JIRA_URL`/`USER`/`PASSWORD` | `live_session jira`: the recipe starts, seeds and removes Jira itself |
| `just confluence-live` | `knobas-source-confluence` / `live_confluence_seeded`, the Confluence half of `atlassian_live` (by name: the `justfile`'s `atlassian_live_confluence_tests`), and `knobas-app` / `confluence_live` | `KNOBAS_CONFLUENCE_URL`/`USER`/`PASSWORD` | `live_session confluence`, likewise |
| `just atlassian-live` | `jira-live`, then `confluence-live` — one product at a time | as those two | as those two |
| `just estate-live` | `knobas-app` / `estate_live` — both importers against the real systems (#509, #510, #634): the **hcloud importer's round trip** over whatever the knobas Hetzner project holds (produce, land, apply, produce again — all known, nothing changed), and the **Docker importer** against the WSL box's engine through the `wsl` context (`testenv/wsl/connect.sh` makes it), whose running containers must all be in `testenv/estate.json`. No ssh, no container started, no seed. The project is empty since #634, so an ordinary run is `ESTATE_LIVE_HCLOUD_SERVERS=0`; a PR touching the hcloud producer owes a run at `=1`, with a server Björn stands up and deletes himself (the `justfile` recipe's header) | `HETZNER_API_TOKEN`, `ESTATE_LIVE_HCLOUD_SERVERS` | the repo-root `.env` (a read-only token for the knobas project), and the count on the command line |

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
it at the same time will break each other. The first entry below was one of
the ways until #684 and is the rule that ended it; the four after it still are:

- **The Gitea token is per worktree** (since #684; before it, the seed
  re-minted one shared token and was the first of these collisions).
  `seed-state.json` is git-ignored and therefore lives *inside one worktree*,
  while the Gitea container is everyone's. So the rule is:
  - `seed-gitea.sh` keeps the token recorded in *this* tree's
    `seed-state.json` while Gitea accepts it — one call to `/user`, whatever
    the token is named.
  - When Gitea refuses it (401), or the tree has none, the seed mints one
    named `knobas-seed-<worktree directory>-<checksum of its path>` and first
    deletes only tokens of **that** name — its own earlier token, whose value
    it has lost. Another worktree's token is never revoked, nor is the shared
    `knobas-seed` token the seed minted before #684, which a tree still
    holding it keeps while it works.
  - Any other answer to the probe, or none, is a refusal that changes
    nothing.
  - `./seed --env` runs the same probe before it prints a token (#1377): a
    token Gitea does not answer 200 to ends it, non-zero, naming the status
    and `./seed`, so `just responsiveness-probe` and `just gitea-live` stop
    before cargo starts. A print never re-mints; `./seed` does.

  So two worktrees seeding one after the other, or at the same time, both
  keep a working token. A 401 from `eval "$(./seed --env)"` now means this
  tree's own token is gone (a reset Gitea volume, a hand-deleted token), and
  the next `./seed-gitea.sh` here replaces it. One token per worktree that
  ever seeded accumulates on the admin account; a `down -v` clears them.
  The rule is `testenv/gitea-token.sh`, held to it by
  `testenv/gitea-token-test.sh` in `just witness-unit`. Before #684 it went
  wrong four times in the #660 wave (PRs #674, #675, #681, #682) and once
  during PR #130's review.
- **The Gitea live suite sweeps.** `tests/live_gitea.rs` removes any leftover
  `knobas-` branch in `payout-service` before it starts, which is how a killed
  run heals (issue #143). It cannot tell a sibling's live branch from a corpse.
- **`just gitea-live-capped` reconfigures the shared container.** It recreates
  Gitea with `MAX_RESPONSE_ITEMS: 1` and uncaps it again from a trap, so for
  the length of that run every other reader of this environment is talking to a
  server that answers one record per page. A concurrent `just gitea-live` fails
  on missing records, which reads as an adapter defect and is not one.
- **Kuma has one Docker host, `testenv-docker-proxy`** (#978), which
  `./seed-kuma.sh` creates and repairs. It points at `tcp://docker-proxy:2375`,
  the `docker-proxy` service (tecnativa/docker-socket-proxy: container reads
  only, no POST, no published port, on an internal network shared with Kuma
  alone). A plain `docker compose --env-file images.env up` starts it, as it starts Kuma, and Kuma
  depends on it; container reads include every container's environment, which
  is why nothing else reaches it. Kuma deliberately does **not** mount
  `/var/run/docker.sock`: that would give the container control of the Docker
  daemon. It is what lets a Docker check created through knobas have a host
  (`kuma-live`'s `a_docker_check_is_published_with_the_seeded_docker_host`).
- **`./seed-kuma.sh` deletes every monitor `monitors.json` does not name**
  (since 2026-09-06, #441). That is what makes the seed idempotent, and it is
  also a sweep: a monitor a sibling added by hand, or that knobas itself
  created through the Kuma source's *Create monitor for this asset* (#453,
  landed 2026-09-07), is gone on anyone's next `./seed`, silently and with no
  way back. Add a monitor that has to survive to `monitors.json`, not to the
  UI.

- **`just kuma-live` adds and deletes four monitors of its own** --
  `knobas-live-scratch` and `knobas-write-scratch` through `kuma-monitor.sh`,
  `knobas-live-created` and `knobas-write-created` through knobas' own create
  (#453) -- and pauses and resumes them, which is why the sweep above matters:
  a killed run's leftovers are removed by the next `./seed-kuma.sh`, and a
  sibling counting monitors mid-run counts them.

  Its **API key no longer collides** (since #1484, the rule #684 made for the
  Gitea token). The run re-mints the key when the worktree it runs from has no
  `kuma-api-key`, or has one the instance no longer answers to: Kuma hands a
  key's clear text out once, so `seed-kuma.sh` verifies the recorded key
  against `/metrics` and keeps it **while it still authenticates**, exactly as
  `seed-gitea.sh` does with its Gitea token, and replaces it when it does not
  (since 2026-09-11, #552), and only on an answer the instance actually gave: a
  probe that gets no answer at all refuses rather than guessing. The key is
  named `knobas-seed-<worktree directory>-<checksum of its path>`, and a
  re-mint deletes only keys of **that** name, so a sibling's seed never 401s
  your copy. Another worktree's key, and the shared `knobas-seed` key the seed
  minted before #1484, are never deleted; one key per worktree that ever
  seeded accumulates on the instance, and a `down -v` clears them. A 401 from
  `/metrics` now means this tree's own key is gone (a reset volume, a
  hand-deleted key), and the next `./seed-kuma.sh` here replaces it.

So: **claim the environment before running `./seed` or `just kuma-live`, and
say when you release it.** `start-work-live` seeds too, and its branches carry
the same `knobas-` prefix `live_gitea.rs` sweeps, so a concurrent `just
gitea-live` would delete the branch out from under it. The failure mode is a
vanished branch, a vanished monitor or a count that is one off, none of which
reads as "somebody else is in here". For the three Gitea recipes the tooling enforces it since #724: they
queue on the Gitea lock (*Live products, on demand*). Kuma and a bare `./seed`
take no lock, and neither does a TeamCity run -- so **`just gitea-live-capped`
still wants the environment claimed** while a TeamCity run may be cloning from
the Gitea it caps.

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

**Issues (#1206).** The seed also files three issues in `ledger-api`, after every pull
request so no pull request number moves: one with `Priority/High`, `Kind/Bug`, an assignee and
a comment, one with `Priority/Medium` and nobody assigned, and one closed. Knobas mirrors them
as tickets (`gitea:tidewater/ledger-api/issues/<n>`). The closed `(reserved)` index
placeholders above are issues too, so they come back as closed tickets.

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

Off by default because they are expensive. Approximate costs are in `images.env`.

```sh
docker compose --env-file images.env --profile real-teamcity  up -d teamcity teamcity-agent   # ~3.5 GB pull, ~10 GB on disk
docker compose --env-file images.env --profile real-atlassian up -d jira-db jira confluence-db confluence
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
docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent
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
just live-up teamcity        # Gitea first (the VCS roots point at it), then the
                             # wizard, token, agent and the Tidewater builds
./seed --teamcity --running  # ... and hold Payout_Build 1188 at step 3/5
```

A TeamCity that was only *stopped* answers its REST API with a 503 while the
application starts, so `seed-teamcity.sh` reads it as a server in
maintenance, waits out `APPLICATION_STARTING`, finds no create-admin form
(the administrator exists), and then asks the administrator's credentials
again — which is every on-demand start since #633.

**What it creates**, derived from `fixtures/tidewater/work.json`'s `builds`
by the rules of `crates/knobas-mockd/src/tc_state.rs`, so the ids are the ones
the adapter's tests and the project census already expect:

| Fixture `cfg` | Project (id = name) | Build configuration name | Build | Outcome |
|---|---|---|---|---|
| `Ledger_Deploy_Staging` | `Ledger` | `Deploy Staging` | `412` on `main` | SUCCESS |
| `Payout_IntegrationTests` | `Payout` | `IntegrationTests` | `1187` on `feature/PAY-231-sepa-retry` | FAILURE, the fixture's `log` in the build log, its failed test reported as one, the log again as the artifact `sepa-report.txt` |
| `Payout_Build` | `Payout` | `Build` | `1188` on `feature/PAY-231-sepa-retry`, `--running` only | running, held at step 3/5 `cargo test` |

No configuration has a description: the fixture gives none, and the seed
invents none, exactly as mockd does.

**A build chain, for the chain views (#1010).** The Builds view's chain line
and the run's *Chain on <branch>* panel (#857) need configurations joined by
snapshot dependencies, which the fixture has none of. The seed adds project
`SeededChain` under `_Root` with `SeededChain_Compile`, `SeededChain_Package`
and `SeededChain_Release`, each building payout-service through a VCS root of
its own and each waiting on the one before, and queues one `Release` build on
`feature/PAY-231-sepa-retry`, which runs all three. It is **not** in
`fixtures/tidewater/work.json` (a cross-stream contract), no fixture
configuration is in the chain (a build queues the ones it depends on, so a
fixture configuration in it would move that configuration's build numbers),
and it is not the live suite's own `KnobasChain857`, which that test creates
and deletes. `live_teamcity_seeded.rs` counts the fixture's three
configurations apart from the chain's, keeps the chain's builds in the exact
sets (`seed-state.json`'s `teamcity.chain` records them), and witnesses the
chain itself. Like the change witness, it waits while `--running` holds the
agent.

**Parameters and a trigger, for the source view (#664).** The fixture says
nothing about either, so the seed decides, all on `Payout_IntegrationTests`:
`tidewater.region = eu-central-1` on project `Payout` (so the configuration
shows it as *inherited*), `env.SEPA_RETRIES = 5` of its own, the password
parameter `env.SEPA_SANDBOX_KEY`, the select `env.LOG_LEVEL` (`info` or
`debug`, labelled *Debug*, default `info`) for the Run… dialog's typed fields
(#852), and one VCS trigger on `+:feature/*` that is
**disabled** -- enabled, it would queue a build on every push the Gitea live
suite makes. `seed-state.json`'s `teamcity.settings` records the configuration
and the password's made-up value, which `live_teamcity_seeded.rs` proves never
reaches a view (and which TeamCity itself never serves back).

**A failed test and an artifact, for the build view (#665).** The step of
`Payout_IntegrationTests` prints the fixture's `log`, then reports the test
its `test <name> ... FAILED` line names as a failed test (TeamCity service
messages; the failure message is the log's next line), publishes the log as
`sepa-report.txt`, and exits 1. TeamCity then composes 1187's `statusText`
from the test counts, `Tests failed: 1 (1 new), passed: 0`, not from the exit
code. A server seeded before #665 is brought up to date by the next `./seed
--teamcity`: the step's script is rewritten in place, and a finished 1187
that reports no failed test is deleted and run again under the same number
(its id changes; `seed-state.json` records the new one). TeamCity lists no
change for 1187, because it is its configuration's first build; its build
view's *Revision* row is what leads to its commit (#683).

**A build that lists a change (#683).** TeamCity lists a change only for a
commit it has never seen on any branch, and never for a configuration's first
build on a branch -- re-pointing a branch at a commit another branch already
holds lists nothing (measured). So the seed keeps a branch of its own,
`seed/change-witness` in `ledger-api`: cut at `main`'s head, built by
`Ledger_Deploy_Staging`, given one commit (the file
`seed-change-witness.txt`, through Gitea's contents API) and built again. The
second build lists that commit as its change. This is the one write the
TeamCity seed makes to Gitea; `ledger-api`'s default branch is untouched and
no capped suite budgets that repository. `seed-state.json`'s
`teamcity.changes` records the configuration, the commit and both build ids,
and the seeded TeamCity suite counts both builds as seeded. A re-run that
finds both builds, the change and the branch head as recorded writes
nothing; otherwise it deletes the branch's builds, cuts the branch again and
makes a new commit.

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
specification naming every branch the Gitea seed created in that repository,
and the change witness's. A mirrored commit's page carries Gitea's own
`ROOT_URL`, `http://127.0.0.1:3000`, so a change named by the root's URL
opens it only through a Gitea source whose `clone_urls` lists
`http://gitea:3000` (#683) -- what `teamcity_seeded_live.rs` configures, and
what a real deployment whose CI clones over an internal hostname would. A
finished fixture build that ran from any other root URL is run again under its
number.
A project whose finished build ran on a commit the Gitea now up has never held
is deleted whole and made again (#964): a build keeps its commit, TeamCity's
data outlives Gitea's, and a recreated, reseeded Gitea has other shas (the
repository's first commit carries the seed's own date), so the build's
*Revision* row would name a commit no sync can mirror. Re-running only the
build does not do, since the VCS root keeps the old modifications and the new
build lists the whole branch as its changes.
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
running. Since #690 the recipe enforces it: two worktrees starting `just
teamcity-live-seeded` together run one after the other, on the JVM lock
(*Live products, on demand*). `./seed --teamcity --running` and this suite are therefore mutually
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
docker compose --env-file images.env --profile real-atlassian up -d --wait jira-db jira
./seed-atlassian.sh jira
docker compose --env-file images.env --profile real-atlassian up -d --wait confluence-db confluence
./seed-atlassian.sh confluence

./seed-atlassian-content.sh # the Tidewater content (next section)
eval "$(./seed --env)"      # adds KNOBAS_JIRA_* and KNOBAS_CONFLUENCE_*
```

`just live-up jira` and `just live-up confluence` are each half of that, and
since #633 the two are never up at once (*Live products, on demand*); by hand,
`./seed-atlassian-content.sh jira` or `confluence` seeds one of them.
`./seed --atlassian` still walks both wizards and seeds the content against a
pair brought up together, for a machine with the memory for it.

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

The keys live in your **shell**, not in `images.env` — that file is tracked
(`./fetch-timebomb-keys.sh --write` drops them in the git-ignored
`.env.licences` if you want them to survive a new terminal).
`CONFLUENCE_LICENSE_KEY` must be set when the container *first starts*, since
Confluence reads it at first-time setup, and `seed-atlassian.sh` checks the
container's own environment for it; Jira's goes in through the wizard, and the
seed fetches it itself when it is unset.

**Three hours is the shape of this environment.** The licence expires three
hours after it is applied, and restarting the container does not reset it. So
this is stand up → run what needs a real instance → `just live-down jira` (or
`confluence`), which removes the product with its volumes, not a long-lived
environment like Gitea's. **Not a bare `down -v`.** A profile-scoped `down -v`
with no service named takes the default profile's containers and volumes with
it too — Gitea and its seeded corpus included (checked with `--dry-run` on
Compose v5.1.2) — and even with the services named it removes the project
network when nothing running is on it, stranding every stopped container
attached to it (*Live products, on demand*).

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
just atlassian-live      # from the repo root: jira-live, then confluence-live
```

which is, per product: `fetch-timebomb-keys.sh` (before the `up`, since
Confluence reads its key at first start) → `up -d --wait <product>-db
<product>` → `seed-atlassian.sh <product>` → `seed-atlassian-content.sh
<product>`, twice → `seed-atlassian-content.sh <product> --verify` → every
crate test gated on that product's URL → the product removed with its
volumes, from a trap, so the teardown runs when a step fails and on Ctrl-C.
**Jira is gone before Confluence starts.** Measured on #633's runs (12 cores,
the 8 GB VM, images pulled, empty volumes): `jira-live` 299 s in all, seeded
and verified at 244–263 s, its three suites ~15 s once compiled;
`confluence-live` 91–104 s, seeded and verified at 56–58 s. Each three-hour
window holds with hours of margin. The pair's 2026-09-03 numbers, both
products up together, are in the `justfile`'s `atlassian-live` header.

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

**One product at a time, since #633.** `just atlassian-live` is now
`jira-live` then `confluence-live`: each starts its product, seeds it,
runs its suites and removes it before the other starts, and each refuses while
any other JVM product is up (*Live products, on demand*). The app crate's
`atlassian_live.rs` holds tests for both products, so it runs twice — with
Jira, its Confluence tests skipped by name; with Confluence, exactly those —
and the list is the `justfile`'s `atlassian_live_confluence_tests`. A new
Confluence test missing from it fails loudly in the Jira run, and a listed name
the file no longer has stops both recipes before they start a product.
`--verify confluence` also waits until every seeded page comes back from a CQL
search of the space: the full sync is a search, and on #633's first run the
suites, now started seconds after the seed, saw two pages of seven. It waits
the other way too: a page in `ENG` that `seed-state.json` does not record must
have left the search, since the adapter suite bounds a full sync by the seeded
pages and the home page (#2111). The content seed itself deletes every such
page, trashed and purged, after it has found or created its own: a suite's
guard deletes the page it made, a run that was killed reaches no guard, and a
Confluence left up between runs kept that page for the next one.

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

**A third project, `NARROW`, whose workflow narrows** (issue #522). Every
software template this container offers gives that same all-to-all Simplified
workflow, so on `PAY` and `OPS` alone "what the workflow offers from here" and
"every status this project has" are the same list — and #498's
reachable-transition read, whose whole point is that it *narrows*, had no live
witness for the narrowing. So the seed also creates *Narrowing workflow
fixture* from Jira **Core**'s process-management template
(`com.atlassian.jira-core-project-templates:jira-core-process-management`),
which gives seven statuses and a proper subset from each of them: *Open* →
*In Progress*; *In Progress* → *Under Review*, *Cancelled*; *Under Review* →
*Approved*, *Rejected*; *Approved* → *Done*; *Done* → nothing; *Cancelled* →
*Open*; *Rejected* → *In Progress*. It is **not** fixture content: no ticket
of `work.json` belongs to it, it is recorded as `jira.narrowing` rather than in
`jira.projects`, and it **holds no issues between runs** — the live witness in
`atlassian_live.rs` files its own ticket, walks it and deletes it, and a killed
run's leftover is cleared by the adapter suite's leftover clearing like any
other stray. Adding it takes this Jira's whole status list
(`GET /rest/api/2/status`) from four to nine, which is what lets `PAY`'s own
reads witness that the answer is a *subset* rather than a copy.

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
empty. `jira.narrowing` records the third project — its key, name, template,
the issue type the live witness files as, and the statuses that project's
workflow for that type has — and `crates/knobas-app/tests/atlassian_live.rs`
refuses to start without it.

**Idempotent, and it says what it skipped.** Every create is preceded by a
read — issues by key, comments and worklogs by text, pages by title in the
space, page comments by text — and the run ends with `done: created N,
skipped M`. Re-running against a half-seeded instance finishes it; against a
seeded one it creates nothing.

## A seeded pull request that is no longer open (#884)

`seed-gitea.sh` refuses when a pull request the fixture says is open ("SEPA
retry with exponential backoff", "Add payout CSV export") is merged on the
server, and prints the remedy: Gitea on a fresh volume (`docker compose
rm -sfv gitea`, remove the `gitea-data` volume, delete `seed-state.json`, seed
again; `./reset` does the same for every product). A merged pull request
cannot be reopened, and a replacement would invent history on `main`, so the
seed does not repair it. A **closed** one it reopens (#2110): closing merges
nothing, so the reopen restores the fixture exactly. An open pull request on
the same head and base is closed first: Gitea would reopen beside it, leaving
the fixture one open pull request over. When Gitea refuses the reopen (the
head branch is gone) the seed refuses as for a merged one, with Gitea's
answer in the message. On 2026-10-25 Gitea's timeline points to a person:
payout-service#142 closed through the app's *Close* action and a pull request
with the body `asdf` opened from its branch 15 s later, not a suite. The live
suites only merge pull requests they opened on `knobas-` branches (audited in
#884: every `MergePullRequest` site in `live_gitea.rs` and `start_work_live.rs`
merges its own); what merged the original
one on 2026-10-14 was not a suite. `testenv/gitea-pr-state-test.sh` (in `just
witness-unit`) holds the rule to account.

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
  the variable does not remove the line, so a plain `docker compose --env-file images.env up` after a
  capped run comes back on a container with no such variable and a still-capped
  server. That is why `docker-compose.yml` pins
  `GITEA__api__MAX_RESPONSE_ITEMS: "50"` — Gitea's own default, and
  `client::PAGE_SIZE` — explicitly rather than leaving it implicit.

## Image pinning

No `:latest` anywhere in the compose file. `images.env` holds one
`NAME=repo@sha256:…` line per image and is **committed**: it contains public
image digests and no secrets. The compose file interpolates them with a failing
default, so a missing pin stops `docker compose config`, not `docker compose --env-file images.env up`
three minutes later.

```sh
./pin-images.sh   # re-resolve the tags in the script to digests, rewrite images.env
git diff images.env     # review it: a digest change is an environment change
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
same change. A held pin says so on its own comment line in `images.env`. `--out PATH`
writes elsewhere, to compare.

## Network access

The environment's **containers and seeds** run offline once the images are
pulled, with **one exception**: `kuma-seed` runs `npm i socket.io-client@4`
into a scratch prefix at seed time, because Uptime Kuma v2 has no REST API for
configuration and the client has to come from somewhere.

Uptime Kuma itself does not leave the laptop either, since #634: every one of
its six monitors watches something on this machine. (From 2026-09-06 six of
the eight it then held left it — three pings on the Hetzner servers and three
checks through the SSH tunnel — and read down off the network.)

## The responsiveness probe's saved database

`just responsiveness-probe` (#753) times every read the shell makes against a
seeded mirror of 22,000 items. Building that mirror takes minutes and reading
it seconds, so the probe keeps the built database (#781): the whole server root,
`data/` and `.pgpass`, under `$TMPDIR/knobas-probe-snapshots/`, in two copies,
one never analyzed (autovacuum off on every table) and one analyzed. A run
clones the copy for its `RP_ANALYZED` state with `cp -c` (an APFS clone, seconds
whatever the size), starts its own postmaster on the clone, and leaves the
snapshot untouched. It prints one `RP-SNAPSHOT` line: built or restored, the
snapshot's name, its size and its age.

- **Key.** A snapshot is named `m<migrations>-p<seed>-<never|analyzed>`:
  `<migrations>` hashes every file in `crates/knobas-db/migrations`, `<seed>`
  the sizes (`RP_TICKETS`, `RP_PAGES`, `RP_BUILDS`), `RP_SYNTHETIC` and whether
  the seeded Gitea was synced. Change a migration, a size or `RP_SYNTHETIC`
  and the next run builds a new snapshot on its own.
- **`RP_FRESH=1`** rebuilds both copies. Use it when the seeded Gitea has
  changed (the snapshot holds what it held when it was built) or when the seed
  code in `responsiveness_probe.rs` has.
- **`RP_ITEMS`** (default 22000) scales the three sizes; `RP_ITEMS=2500` is the
  size to iterate on and has a snapshot of its own. An explicit `RP_TICKETS`,
  `RP_PAGES` or `RP_BUILDS` still wins.
- **`RP_PASS`** (#793). `run` (the default) runs and times the suggestion pass
  (`RP-PASS ms=...`). `skip` restores a third snapshot, taken after the pass,
  and skips it; the line then reads `RP-PASS skipped (snapshot ...)`, so a
  reading is never mistaken for a pass measurement. The snapshot is built once
  (restore the base copy, run the pass, save) when missing, and its key adds a
  hash of `crates/knobas-core/src/suggest.rs` and the migrations, so a change to
  the pass builds a new one. `RP_FRESH=1` rebuilds it too. Snapshots of an
  older pass are removed by the 7-day sweep or a migrations change.
- **Disk.** Each run first deletes snapshots older than 7 days or whose
  migrations hash is not the current one, and unfinished copies older than an
  hour. The 22k copies are about 0.2 GB each as files; the clone costs little
  until the run writes.

## The estate file

`estate.json` is the estate knobas models, written down: this notebook, the
Windows desktop that runs WSL 2, the WSL VM and the Docker engine in it, every
container `docker-compose.yml` runs there — Gitea, Uptime Kuma and its Docker
API proxy, and TeamCity, Jira and Confluence with their databases — and the
ports the notebook reaches through the ssh tunnel as routes, with the asset
each one lands on. The notebook is typed `custom`: since it runs no container
it holds nothing in the tree, and the type table has no name for a
workstation. Environment (`dev`) and owner are
set once at the root; each asset names the Uptime Kuma monitors that watch it.
From M4 the estate knobas models **is** this infrastructure and not a
fictional one (#427), so the file is a fixture with readers, not
documentation: the Import, `--demo` (`knobas_app::sources::demo`),
`crates/knobas-app/tests/it/estate_exit.rs` (M4.0's exit checklist) and the
`?fake-ipc` harness (`app/src/lib/shell/dev/fake/estate.ts`) all read these
bytes. `crates/knobas-core/tests/it/estate_file.rs` keeps it honest in `just
check`: unique ids, parents and route ends that resolve, one tree, types from
the table, no unknown keys, every compose service recorded on the WSL box's
engine, every container carrying the `docker_context` and `container_name` it
is matched on, and the monitor names agreeing with `monitors.json`.

A container recorded here and not running is not wrong: TeamCity, Jira and
Confluence stay up on the box, but the memory budget may stop one, and the
file describes them whether or not they are up. What `just estate-live`'s
Docker half does catch is the other direction — a container **running** on
the engine and not written down here. That is why the Docker API proxy is
recorded although nothing routes to it, and why a Forgejo left running after
a `gitea-live forgejo` run would turn that half red.

Until 2026-11-01 the engine was OrbStack's, in a Linux VM on this notebook,
and the products ran there on demand; the file also held the mockd container
and its three ports. The box's values in the file (Windows host
`DESKTOP-QUUAHUB`, Ubuntu 26.04 in WSL 2 with 12 cores and 12 GB, docker.io
29.1.3, the bridge at `172.17.0.1`) are the real box's, as of 2026-11-01.

Until 2026-10-12 the file also held three Hetzner servers, their engines and
the SSH tunnel's forwards, and lived in `testenv/hetzner/` with the
provisioning. Björn decommissioned the servers (#634); `git log --
testenv/hetzner/` has the directory, and
`crates/knobas-app/tests/it/support/hcloud-estate.json` keeps the three server
entries as the counterpart of the hcloud recording the importer's shape
battery runs on.

## Monitors

`monitors.json` is **the estate**, not fixture content. Since 2026-09-06
(issue #441, M4 spec #427) the list is the real infrastructure this repository
runs on: since #634, a ping on the notebook, one HTTP check per JVM
product, one on Gitea, and a canary that exists to be knocked over.
`fixtures/tidewater/work.json` still contains no monitors — a monitor is a
*mirrored item of the Kuma source*, never fixture work (M4 spec, "Assets are
knobas-owned; monitors are mirrored").

| Monitor | Type | Checks | Goes red when |
|---|---|---|---|
| `notebook` | ping | `100.92.199.117` | the notebook's Tailscale address stops answering the Kuma on the WSL box |
| `teamcity` | http | `http://teamcity:8111/login.html` | TeamCity is stopped |
| `jira` | http | `http://jira:8080/status` | ditto |
| `confluence` | http | `http://confluence:8090/status` | ditto |
| `gitea` | http | `http://gitea:3000/api/healthz` | the Gitea container is down |
| `canary` | http | `http://host.docker.internal:8299/` | `./canary.sh down` released the port |

**The three product checks are red whenever their product is stopped.** On
the WSL box they stay up, so a red there is a product down, or one the memory
budget stopped, or one being set up again for its licence. On the local
target, where they run only while a live recipe needs them (#633), red is the
ordinary state and not an outage. (Until #634 they were named `(tunnel)`,
because they fell with the SSH tunnel to the Hetzner servers, and three pings
watched the servers themselves.)

The endpoints are the unauthenticated ones each product answers `200` on
(`/login.html`, `/status`, `/status`); the obvious REST paths answer `401`,
which `accepted_statuscodes: ["200-299"]` reads as down.

**The `notebook` ping watches `100.92.199.117`**, the notebook's Tailscale
address, which the Kuma on the WSL box reaches it at, and
`testenv/estate.json` states it as the notebook's `ip`. Literal rather than a
name because a ping carries a bare address and the estate states one, which
is what lets `monitor_url_host` (the suggestion rule that reads a ping's
`hostname` against an asset's `ip`, #479) reach a ping on the real estate at
all. Until 2026-11-01 it watched `0.250.250.254`, what `host.docker.internal`
resolved to inside OrbStack's VM. If the address moves, change it here, in
`monitors.json` and in the estate file together (`estate_file.rs` holds the
two equal).

**Can the Kuma container send ICMP?** Yes, measured on the pinned image (2.5.3)
under OrbStack on 2026-09-06: the container runs as `uid=0(root)`,
`/proc/sys/net/ipv4/ping_group_range` is `0 2147483647`, and `/usr/bin/ping` is
present. Were that ever to stop being true, switching the ping to Kuma's `port`
type is not only a `monitors.json` edit: `kuma-seed.mjs`'s `payload()` branches
on `ping` against everything-else-is-http, and `OWNED` (what counts as drift)
has no `port`, so both need a third case first.

**Only the canary checks the host**, through `host.docker.internal`. The
three products and `gitea` are reached by their compose service names on the
compose network, which works on any engine; the `notebook` ping is ICMP, with
no socket at all. Docker Desktop and OrbStack define `host.docker.internal`
themselves; the `extra_hosts: host.docker.internal:host-gateway` line on the
`uptime-kuma` service is what makes the name *resolve* on a plain Linux
engine, such as the WSL box's, which defines nothing: to the bridge gateway,
`172.17.0.1`. A bridge-gateway client cannot reach a socket bound to
`127.0.0.1`, so on a remote target `canary.sh` binds the canary on the bridge
address. Until 2026-11-01 the three product checks went through the alias to
the products' published ports too, which worked only under OrbStack, where
the alias reaches a host socket bound to `127.0.0.1`.

Adding it did cost one thing, once: a compose service whose definition changes
is **recreated** on the next `docker compose --env-file images.env up -d`, so the shared Kuma was
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
runs, the notebook is the estate. Stopping one of those to manufacture a red
is how one stream's live run becomes another's mystery failure, so the estate
carries one check whose entire blast radius is a port nobody else binds. **No
live run stops a shared container** (M4 spec #427).

Port 8299 is deliberately **outside** the §5 map above: it is a host port, not
a published container port, so `check-ports.sh` neither knows nor should know
about it. The pid lives in `~/.knobas-canary-8299.pid` — in `$HOME`, because
a process that outlives a shell does not belong in the repository directory, and `./reset` therefore does not
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

## sccache on a dev machine

Björn's Mac builds through sccache as a machine-wide `rustc-wrapper` (sccache 0.18, installed 2026-10-13, 30 GiB cache). It is expected, not required by the repo: nothing in `.cargo/` or the `justfile` mentions it. The setup:

```toml
# ~/.cargo/config.toml
[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"
```

The cache's own config is `~/Library/Application Support/Mozilla.sccache/config` (size, location). `sccache --show-stats` shows hits.

**Failure mode:** the wrapper path is absolute. If `/opt/homebrew/bin/sccache` goes missing (a `brew uninstall`, a new machine with the config copied over), **every** cargo call fails, including `rustc -vV` probes, and the message names cargo rather than the wrapper. `just pre-gate` prints one warning line (it never fails) when the configured wrapper is missing or does not run.

**What it buys (measured 2026-10-14, #743, quiet 12-core machine, `main` at 749985ff, `just check`):** dependencies are cacheable, so a fresh worktree's first gate reuses them. Workspace crates mostly are not: sccache hashes absolute source paths, so a workspace crate hits only in the worktree that built it, and with incremental compilation on (the default for dev and test) rustc output is not cached at all.

| gate | default | `CARGO_INCREMENTAL=0` |
|---|---|---|
| fresh worktree, first gate | 291 s | 293 s |
| repeat gate after a one-line edit to `knobas-core` | 160 s | 193 s |

`CARGO_INCREMENTAL=0` was **not** adopted in the recipes: it is no faster for a fresh worktree (the workspace crates miss on the new path either way) and slower for the repeat, where incremental compilation beats a full rebuild of the changed crate and everything above it. One trap when measuring it: sccache folds `CARGO_*` variables into its key, so the first run with `CARGO_INCREMENTAL=0` re-misses every dependency (3.5% Rust hits against 60% warm); compare only a second run.

### Target size per worktree (#905)

Each ticket worktree grew to 8–26 GB, and the loop's disk guard made that the limit on parallel tickets. Measured 2026-10-15 (#905) the same way as the table above: `main` at b7279c27, a fresh worktree's first `just check`, then a repeat after a one-line edit to `knobas-core` (a new `pub const`), `du -sh target` after each, cargo's default jobs (no `CARGO_BUILD_JOBS`), sccache warm for each option's flags (option 2's profile got one throwaway fresh gate first, since a new profile re-misses every dependency). 1-minute load before each run in brackets; the repeat starts straight after the fresh gate, whose own load is still decaying.

| option | fresh: wall | fresh: `target` | repeat: wall | repeat: `target` |
|---|---|---|---|---|
| 1. today (`[profile.test] debug = 0`, no `[profile.dev]`) | 303 s (3.6) | 7.9 GB (incremental 3.4) | 174 s (13.5) | 9.9 GB (incremental 5.4) |
| 2. + `[profile.dev] debug = "line-tables-only"` | 292 s (3.5) | 7.9 GB (incremental 3.4) | 172 s (10.0) | 9.9 GB (incremental 5.4) |
| 3. 2 + `[profile.test] incremental = false` | 306 s (2.8) | 5.7 GB (incremental 1.1) | 186 s (13.4) | 6.8 GB (incremental 2.2) |

`deps` (3.9 GB) and `build` (525 MB) were the same in every run. **Option 3 is adopted** (`Cargo.toml`): about 2.2 GB less on a fresh worktree and 3.1 GB less after one repeat, for about 14 s on a repeat gate. The saving grows with every repeat, since the test profile's incremental state was what piled up (7.1 GB of `issue-816`'s 13 GB). What is left in `incremental/` is `cargo clippy`'s, which runs on the dev profile.

Option 2 changes nothing in the gate: clippy only checks, so no dev-profile code is generated, and its sccache hits matched option 1's exactly. It is kept because it shrinks what does build on the dev profile (`just dev`, `cargo build -p knobas-app`, the desktop witness), and a dev backtrace still names file and line for each frame. Test binaries stay at `debug = 0`: a failing test's panic message names file and line (from the panic location), and its backtrace frames name functions only, as before.

### Gate stages (#920)

A passing `just check` now prints each recipe's wall time (`step <recipe> N s`), `test`'s build and run times, and its ten slowest test binaries by wall time. Measured 2026-10-15 (#920) the same way as #905's repeat: a warm worktree (one throwaway gate first), a one-line edit to `knobas-core` (a new `pub const`) before each run, cargo's default jobs, normal priority, nothing else running, the 1-minute load under 4 at each start (in brackets). Times in seconds.

| run | `fmt` | `shell` | `test-layout` | `witness-unit` | `clippy` | `clippy-libs` | `inventory` | `test` (build + run) | cargo half | `front` | `just check` |
|---|---|---|---|---|---|---|---|---|---|---|---|
| before, 1 (3.76) | 1 | 6 | 0 | 16 | 14 | 7 | 69 | 82 (0 + 79) | 195 | 45 | 196 |
| before, 2 (3.93) | 2 | 6 | 0 | 17 | 13 | 5 | 64 | 80 (1 + 77) | 187 | 43 | 187 |
| before, 3 (3.73) | | | | | 13 | 6 | 62 | 81 (+ 78) | 185 | | 186 |
| scripts beside, 1 (1.45) | 2 | 7* | 0* | 17* | 12 | 6 | 70 | 82 (1 + 78) | 172 | 53 | 172 |
| scripts beside, 2 (3.90) | 1 | 6* | 0* | 17* | 12 | 6 | 62 | 82 (+ 79) | 163 | 52 | 164 |

\* in their own half, beside the cargo chain (23-24 s), off the critical path.

Slowest binaries (before, 2): `knobas-app` `test/it` 77 s, `knobas-core` `test/it` 60 s, `knobas-db` `test/embedded` 26 s, `knobas-sync` `test/it` 23 s, `knobas-sync` `test/scheduler_loop` 20 s, `knobas-db` `test/schema` 18 s, `knobas-search` `test/it` 15 s, `knobas-db` lib 9 s, `knobas-source-gitea` `test/it` 6 s, `knobas-app` `test/share_exit` 5 s. `test`'s run is `knobas-app`'s `test/it`'s length.

What the numbers decided:

- **The script checks run as a half of their own** (adopted): `shell`, `test-layout` and `witness-unit` run no cargo and read no `target/`, and their 23 s sat ahead of `clippy`. `just check` 186-196 s before, 164-172 s after.
- **`inventory` is the test build** (renamed `test-build` on 2026-10-29, when the committed test inventory was retired). It runs `_test-executables`, which builds the test binaries; `test` then runs the same build as a no-op (0-1 s). Nothing compiles twice, so nothing changed.
- **`clippy-libs` stays**: 5-7 s, a second pass whose point is a different feature set (`knobas-db` without `test-util`). One cargo invocation cannot lint both, since unifying the features is what it avoids.
- **`xargs -P` stays 6.** `test`'s run, interleaved: N=6 79, 77, 78 s (load 3.76, 3.93, 3.73); N=8 82, 82 s (3.93, 3.48); N=10 82, 85 s (3.54, 3.91). The pool is paced by one binary, and more at once only slow it. SysV segments peaked at 14 of 32 at every N.
- **`check-front` runs the Vitest files a change can reach** (adopted, `just front-changed`): `svelte-check` (about 6 s) and `vite build` (1 s) stay whole; Vitest (32-34 s whole) runs `vitest related` over the changed `app/src` files plus every test that reads sources off disk (11 today, found by grep each run), and the whole suite when config, the setup file, or a deleted or renamed file is in the change. Measured against an edit shaped like a real frontend PR (a comment appended to each `app/src` file it touched), load under 4: shaped like #915 (15 files), old 39, 39 s, new 16, 16 s; like #899 (12 files), old 39, 38 s, new 17, 16 s. 18 of 169 test files ran each time. It stops covering a test that depends on a changed file neither by import nor by reading files; `just check` still runs every test.

### Scratch databases from one template (#925)

`knobas-app`'s `test/it` binary set the length of `test`'s run (#920). Run serially, its tests took 0.14-0.19 s each with the machine 75% idle: most of them ask for a `scratch_database` of their own, and each one was a `create database` plus all 36 migrations, mostly waiting on the disk. Under the gate's contention the same migrations were CPU inside Postgres, repeated several hundred times per run.

`test_util` now migrates once per process, into `knobas_template_<nonce>`, and every scratch and per-file database is `create database ... template` of it, about 35 ms on the test server (`wal_log` and `file_copy` alike). Measured 2026-10-16 the same way as #920: warm worktree on 02cd1406, `just test` alone, cargo's default jobs, nothing else running, the 1-minute load under 4 at each start (in brackets). Run is `test`'s pool, in seconds.

| | run 1 | run 2 | run 3 |
|---|---|---|---|
| before (per-test migrations) | 82 (n/a) | 80 (4.9) | 92 (13.6, not quiet) |
| durability off on the gate server, no template | 76 (4.0) | 78 (3.4) | 81 (3.9) |
| template (adopted) | 77 (3.6) | 75 (4.0) | 76 (3.8) |

About 5 s off every `test` run. A serial `assets_ipc` test went from a 0.18 s to a 0.15 s median. Turning `fsync`, `synchronous_commit` and `full_page_writes` off on the gate server was within the noise, so it is not adopted. The order of the pool's start was ruled out by #920 already (heavy binaries first was slower). What is left is the binary itself: it still runs for most of the pool, 58-76 s, because its several hundred tests each do real work under twelve test threads.

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
first. Run 3 of the #339 prototype (verdict table on the issue)
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
the app with ⌘Q. When ⌘Q cannot be sent or leaves the app running, it sends
SIGTERM, and then stops the embedded PostgreSQL the killed app left on the demo
profile (`stop_left_postgres`, #1967). A server that was already up before the
launch is left alone. Drivers live in `testenv/desktop-witness/drivers/`, and there
are three.

`launcher-hotkey` (#500) presses ⌘K, asserts through the accessibility tree
that the launcher's query box has focus (`AXComboBox` or `AXTextField`,
labelled *Search or act*, which is the `aria-label` in `QueryBox.svelte`), and
presses Escape --
after which the box must be gone from the tree, because `Launcher.svelte`
renders the overlay under `{#if open}` and a closed launcher takes its input
out of the DOM.

`open-in-editor` (#501, ported to the ticket-first shell by #1084) is the
spawn: it writes a stub executable and a fake clone into a scratch directory,
and in Settings › *Checkouts and Open in…* sets the **Clones root** to that
directory (*Change…*, *Save*) and adds an application whose command is
`<stub> {path}` (*Edit applications…*, *Add an application…*). It opens the
repository payout-service through the launcher, waits for its *Cloned at* line
to name the clone, picks the new application from *Open in…* (the menu's last
application, three steps up from the focused default), and asserts that the stub ran with
**exactly one** argument and that it is the checkout path. Exactly one is the
assertion ADR-0016 asks for: what the program received is what the disk
answered, and nothing the mirror holds. Each run leaves its application in the
demo profile's list. The repository's page and not the job's Code section
(which draws the same `TfOpenIn`): measured 2026-10-18, the demo's PAY-231
reaches its branch only through a *proposed* link, so the job's Code section
reads *Nothing connected* until a reader accepts it. And Tickets rather than ⌘K
for the capture driver's job: the same day the demo profile's launcher
answered `PAY-231` with pages, commits and a branch, and `#PAY-231` with no
match at all.

`pull-request-open-in` (#2316) is a pull request's *Open in…*: the worktree
(#2285, ADR-0035), *Diff tool* (#2275) and *Browser* (#2274). The demo's mock
cannot carry it (no source view, repositories with no owner, short shas), so it
uses the testenv Gitea, over **https at OrbStack's container domain**
(`https://knobas-gitea.orb.local`, a certificate from OrbStack's root, which
OrbStack installs as trusted for SSL; `KNOBAS_WITNESS_GITEA` names another https
front). Needs `just live-up gitea` from the same tree, for the token in
`./seed --env`. It clones the repository into a scratch clones root and names a
stub as the clone's `diff.guitool`, adds the Gitea in Settings › *Sources*
(*+ Add a source*, *Test connection*, *Add source*) and an application with a
pull-request command, opens `SEPA retry with exponential backoff` from ⌘K once
the source has synced it, and asserts: the application ran with the worktree
as its only argument, the worktree is at the head and its own `FETCH_HEAD`
records `refs/pull/<n>/head` from the https address, the clone's branch, `HEAD`
and status are unchanged; the diff tool's right-hand folder holds every file
the pull request changes; the *Browser* item names the page and the default
browser comes to the front. It then removes the source with what it synced.
The page draws two *Open in…*: the header's, for the pull request, and the
*Local checkout* card's, for the repository. The header's comes first and is
drawn last, so the driver waits for both and opens the first; the first run
(2026-11-01) opened the card's, whose application ran at the checkout. To
bring knobas back from the browser it focuses the window through AeroSpace
and opens the running bundle: `ax activate` alone is refused while the browser
holds the front.

`capture` (#503, ported by #1084) is the **global** shortcut, which is the one
keystroke in knobas that has to arrive while knobas is not the application
being typed at. In Settings › *Notes and capture* it turns notes on if they
hide (the shortcut registers nothing while they do), sets the combination in
the *Change…* dialog (#1086, clearing a stored one first) and waits for the row
to say *Opens a small window over whatever you are doing.*, and counts the
notes. It opens the job PAY-231 from Tickets (filter by key, the preview's
job link) so the capture has a foreground, brings
**Finder** to the front and asserts it is there, sends the combination,
asserts the capture window is up with the caret in its box, types a line,
presses *Open in knobas*, and asserts that knobas is frontmost again, the
capture window is gone, and the note's own page is on screen (#1504: notes are
shown, so the page does not send the reader to Settings) with its title and
its *Captured from* row. Then it opens Settings › *Notes and capture* again
and asserts one note more than it counted.

**The window gets a known size before the driver runs** (#2056). A tiling
window manager gives a new window whatever share of the screen is left, and
below 1240 px Tickets hides the preview `capture` opens its job from: under
AeroSpace, beside a terminal, the driver waited for a preview that was never
drawn. So the harness runs `witness_window_size` (`desktop-witness-lib.sh`)
between the first draw and the driver, and `just live-shot` runs it before its
steps file. When AeroSpace is running (its CLI answers), the window is
fullscreened by its window id, whatever its width, and the fullscreen ends
with the window. Elsewhere a window narrower than `WITNESS_MIN_WIDTH` (1241
points, pinned against `tickets.css` by `desktop-witness-test.sh`) is widened
through `ax resize`. The run log then prints the size the driver runs at,
or, below the minimum, names the size and the minimum and fails before the
driver's first step.

**The capture window is counted as a window, on both sides of the keystroke**
(#548). `ax windows` lists what knobas has open, one line per window with its
title, and the driver counts the ones titled `Capture` just before the shortcut
and again after it: none, then exactly one. Counting *named elements* is the
question this driver used to ask and it is a representation of the window --
the word `Capture` is on the window's title, its web area and its text area at
once, all three correct, so `exactly_one` counted 3 and refused a window that
was open. The closing assertion is the same count going back to zero, and an
answer that could not be read is never zero.

**Every page a step lands on is held to the app in the gate** (#1968). A pin
says a name is still spelled in some file, not that a step still lands where
the driver waits: `capture`'s pin of `TfNote.svelte`'s `#/settings/notes` was
green for months after *Open in knobas* moved to the note's own page, and the
first run after that went red on a correct app. So a driver waits on a landing
only through `lands <landing> "$READING"...` or `lands_named <landing>
"$NAME"...` (`desktop-witness-lib.sh`), its arguments the driver's own
`readonly` constants. `app/src/App.test.svelte.ts` replays each landing in the
DOM, in the state the driver has made by then, and asserts what the driver's
call names (`app/src/lib/shell/test-witness.ts` reads it out of the driver);
`witness-unit` refuses a driver with no landing and a landing with no replay.
What the replay cannot say is how WebKit's accessibility tree reads that DOM,
which is still the on-screen run's.

Every accessible name any driver acts on is pinned against the file that
carries it by `just witness-unit`, so an edit to one reads as a stale driver
at the gate rather than as a failed run minutes into somebody's screen. **A
name is rendered text**: WebKit names an element by what is drawn, so the rooms
UI's uppercased `.lab` labels arrived as `SHORTCUT` where the markup said
`Shortcut`. The ticket-first shell transforms no name a driver uses, and
`witness-unit` pins that `tf.css` has no `text-transform` (#1084).

**What a pin cannot say, and #547 is the bill for it.** A pin measures that a
string is still in a file. It says nothing about which *attribute* the
accessibility API answers with, and the two are different questions: an
`aria-label` and a button's own words arrive as a **name** (`AXDescription` /
`AXTitle`, which `ax find` counts and `ax press` acts on), while a heading, a
status line and a rendered paragraph arrive as a **value** (`AXValue`, which
only `ax values` reads). `open-in-editor` waited for a heading as a name for as
long as it existed, and its pin was green the whole time. Only a run can tell
the two apart, which is what this harness is for.

**And a pin says nothing about how many elements answer** — which is #548's
bill for the same belief. Three files give the capture window the word
`Capture`: `capture.rs`'s `.title`, `capture.html`'s `<title>` and
`CaptureWindow.svelte`'s `aria-label`. Each pin was green, each name is right,
and `ax find` answered `3` where the driver asked for one. The third question a
driver can ask is therefore about **windows** and not about names at all: `ax
windows` prints one line per window with its title, and `window_count` reads
it.

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
five pieces `open-in-editor` is built out of -- the stub it writes, the git
config it writes, its wait for the stub's record to be *finished* rather than
merely begun, its reading of what the record says, and whether the stub's path
can go into a command template without the quotes it may not type (#547) --
and the four `capture` is: the title a typed paragraph gives its note, the
whole-line match its readings are found by, the reading of `ax frontmost`'s
answer, and `notes_total`, its reading of the section's note count (#1084). Four more
belong to neither: `reading_verdict` and `waypoint_verdict`, which say whether
a wait witnessed the step before it or was merely true on both sides of it, and
`window_count` and `count_verdict` (#548), which read a list of windows and
turn a count of them into the same one-word verdict. A driver is not exempt
from the gate because its *run* is.

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
   harness refuses rather than launching a copy LS does not name. That is the
   runner's job, not the harness's, and #525 carries it as a precondition. The
   harness never moves it for you, and it re-registers whatever was registered
   before on its way out.

   **Measured again on 2026-09-08 by #503, and it is worse than one copy.**
   `lsregister -dump` listed **ten** paths carrying `dev.knobas.desktop`:
   `/Applications/knobas.app`, three old worktrees, the root checkout's debug
   *and* release bundles, and five stale DMG mount points. Moving
   `/Applications/knobas.app` aside was not enough — LS then named the root
   checkout's debug bundle. What worked, and what every run recorded below was
   made with, is unregistering every other path and putting back afterwards the
   ones that still exist on disk:

   ```sh
   LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
   "$LSREG" -dump | sed -n 's/^ *path: *\(.*knobas\.app\) (0x[0-9a-f]*)$/\1/p' | sort -u
   # unregister every path but the bundle this worktree is about to build:
   "$LSREG" -u /Applications/knobas.app          # …and each of the others
   just desktop-witness <driver>
   "$LSREG" -f /Applications/knobas.app          # …and each one still on disk
   ```

   Nothing is moved and nothing is deleted: the only thing changed is the
   Launch Services database, and a path whose bundle is gone needs no putting
   back.

   **Opt-in, since #1176: `KNOBAS_WITNESS_UNREGISTER_OTHERS=1 just
   desktop-witness <driver>` does that by hand-step for you.** After the build
   and before registering the new bundle, the harness reads `lsregister
   -dump`, unregisters every other copy of `dev.knobas.desktop` that lives
   *inside this repo's checkouts* (the root checkout and its `.worktrees/`),
   and on exit -- success, failure, Ctrl-C or SIGTERM, from the same `trap` as
   the rest of the cleanup -- registers again each of those that is still on
   disk. A copy anywhere else (`/Applications/knobas.app`, a DMG mount, a
   scratch directory) is **never touched**: the run lists it as *left
   registered*, and the refusal above still names it if it wins. The default,
   with the variable unset, is unchanged: the harness refuses and moves
   nothing. The selection is `copies_to_unregister` in
   `desktop-witness-lib.sh`, unit-tested over a fake dump.

5. **`swiftc`**, from the Command Line Tools. The accessibility helper is one
   Swift file compiled into a scratch directory on each run (about 2 s);
   nothing is committed as a binary.

Every refusal names the permission by the name it has in System Settings and
points back at this section, and exits non-zero.

### What is not witnessed yet

**Rewritten on 2026-09-08 by #503, because everything it said had stopped being
true.** Until that morning no driver had ever run: the dev Mac's screen was
locked, the harness refused at its first probe, and this section listed four
open questions about a `WKWebView` nobody had been able to look at. The screen
was unlocked, the harness ran, and the answers are below — together with the
three faults the first run found, which are fixed, and the two gaps that remain.

**Two drivers are green**, run from `.worktrees/issue-503` at 04:55 CEST on
2026-09-08 against `dev.knobas.desktop` built and signed by `knobas-dev`:

* `just desktop-witness launcher-hotkey` — ⌘K focuses the launcher's query box,
  Escape takes it out of the tree.
* `just desktop-witness capture` — a shortcut typed into Settings and reported
  as **registered**; a detail opened; **Finder brought to the front and asserted
  there**; the combination pressed; the capture window up with the caret in its
  box; a line typed; *Open in knobas* pressed; knobas frontmost again, the
  capture window gone, and the note open with `CAPTURED FROM` in its links
  panel.

**Both are runs of the head they were made on, not of what merged.** They
predate #503's review commits: `b4dc71d7` moved the capture window's `close`
port from a component prop into `createCapture`'s defaults, so the last line of
the `capture` transcript above — *knobas is frontmost again and the capture
window has gone* — had not been asserted against the wiring that merged. The
merge-manager's re-run on the merge head refused with `screen-locked` at 06:20
CEST on 2026-09-08, and the re-run was **owed to #525**, which carries all three
runs again. The deputy's second ruling of 2026-09-08 on #503, parts 1 and 2.

**And what turned that green into #548's red is unmeasurable.** #525's runner
drove the same driver on the merge head at 16:57 and it refused the capture
window it had just opened. **What is measured is that nothing the assertion
depends on differed**: across every commit of the #503 branch and the squash
`2b8460bf` that merged it, `capture.sh`'s `exactly_one "$CAPTURE_BOX"` line,
`ax.swift` and all three of the app's `Capture` names are byte-identical — so
`b4dc71d7` is **not** the cause and nothing in the app regressed. Why one run
of that code was green and the other red cannot be recovered: neither
transcript records a count, and the worktree the 04:55 run was made in is gone.
What the code says, and it is a reading rather than a second measurement, is
that the old assertion had only one state it could ever be true in — `0` before
the shortcut, `3` with the window's webview up (measured on #548's dump), and
`1` only while the `AXWindow` exists and its document has not loaded, which
`capture::open_window` rebuilds on every press. That would make the green a
race the run happened to win. **Nothing here rests on it**: the assertion is
wrong under either reading, and counting windows has no such state.

**One driver has still never been driven, and the corpus is no longer why.**
`open-in-editor` used to stop where it always said it would — *the demo profile
carries no repo entity* — because `knobas_source_mock::items` walked past the
repos and branches `fixtures/tidewater/work.json` has always held. **#537
closed that**: the corpus now carries three repositories and three branches,
`mock:payout-service` answers the entity read under the name the driver types,
and `crates/knobas-app/tests/it/demo.rs`'s
`the_demo_corpus_answers_the_checkout_the_desktop_driver_opens` holds the whole
chain below the window — the demo load, the entity read, and `checkout::view`
matching the very remote the driver writes into the `.git/config` it plants.

What #537 could **not** do is run it: on #537's head at 09:32 CEST on
2026-09-08 the readiness check answered `screen-locked 1` and refused before the
build. #525's runner got the screen at 16:55 and the driver ran for the first
time — and refused a branch detail that was open in its own dump, which is
**#547**.

**`open-in-editor` is green since #547**, run from `.worktrees/issue-547` at
17:32 CEST on 2026-09-08 against `dev.knobas.desktop` built and signed by
`knobas-dev`, with `/Applications/knobas.app` moved aside and every other path
unregistered so Launch Services named the bundle the run had just built:

```
open-in-editor: the launcher has answered; 'CHECKOUT' on screen: no
open-in-editor: after Return; 'CHECKOUT' on screen: yes
open-in-editor: a repo or branch detail is open, and it has a checkout panel
open-in-editor: pressing 'Open in VS Code'
open-in-editor: the stub ran with one argument, and it is the checkout: …/clones/payout-service
```

That last line is **ADR-0016's witness, and the first time it has ever been
observed**: a program a person chose in Settings, started by the button on a
branch detail, holding the path the disk answered with and nothing the mirror
holds. It is a run of the head it was made on and not of what merged — the
run of the merge head is the merge-manager's, and it is **#525**'s second
criterion (the deputy's ruling of 2026-09-08 on #525, condition (d)).

**`capture` is green again since #548**, run from `.worktrees/issue-548` at
08:49 CEST on 2026-09-11 against `dev.knobas.desktop` built and signed by
`knobas-dev`, with every other registered path unregistered for the length of
the run and put back afterwards. The probe answered `trusted 1 / post-events 1
/ screen-locked 0` before the build and again after it:

```
capture: bringing Finder to the front
capture: Finder has the screen; knobas is behind it
capture: pressing CmdOrCtrl+Alt+Shift+K
capture: before the keystroke, windows titled 'Capture': no
capture: after the keystroke, windows titled 'Capture': yes (1)
capture: the capture window is up, with the caret in it
capture: typing a capture whose title will be 'Retry storm from the capture witness'
capture: pressing 'Open in knobas'
capture: knobas is frontmost again and the capture window has gone
capture: the note is open in the main window, titled 'Retry storm from the capture witness'
capture: the links panel reads 'CAPTURED FROM': the capture attached what was in front of the reader
```

Those middle two lines are the **waypoint in both directions**: knobas had no
window of that name while Finder held the screen, and one the moment the
shortcut was pressed, counted the same way on the same tree — so the wait says
the keystroke did it. The line after the button is the mirror, the same count
back at zero. It is a run of the head it was made on and not of what merged;
the run of the merge head is the merge-manager's, and it is **#525**'s third
criterion (the deputy's ruling of 2026-09-08 on #525, condition (d)).

#### What the first runs answered

* **A Tauri window's `WKWebView` exposes its accessibility tree.** Roles,
  `AXDOMIdentifier`s and all. The question this section opened with is closed.
* **An `aria-label` arrives on `AXDescription` *and* `AXTitle`**, identically.
  The drivers accept either, so this cost nothing, but it had not been seen.
* **A `<label for=…>` names its field on `AXTitle`, and the label element
  itself is not a second match** — `ax find` answers `1`, which is what
  `exactly_one` needs.
* **`AXFocused` and `AXPress` reach a WebKit element.** Both drivers depend on
  them and both work.
* **An accessible name is the *rendered* text.** A `<label class="lab">` is
  named `SHORTCUT`, not `Shortcut`, because `app/src/app.css` gives that class
  `text-transform: uppercase`; the same for the links panel's `.row.hd`
  headings, which read `CAPTURED FROM`. An `aria-label` is **not** transformed:
  it is not rendered text. `desktop-witness-lib.sh`'s `rendered_label` states
  the rule once, both drivers use it, and `witness-unit` pins both stylesheet
  rules.

#### What the first runs broke, and what was fixed

* **A synthetic modifier stayed down.** `ax type` built its events from
  `CGEventSource(.hidSystemState)`, which inherits the session's modifier
  flags, and a `key` posted just before it leaves ⌘ held as far as that state
  is concerned — there is no `flagsChanged` to release it. A driver that
  pressed ⌘A to select a settings field and then typed
  `CmdOrCtrl+Alt+Shift+K` into it sent ⌘C, ⌘A, ⌘S … ⌘K, and the last of those
  opened the launcher over the pane it was typing in. Fixed: `type` clears the
  flags on every event.
* **`open-in-editor` looked for names nothing carries.** `Directory`,
  `Open in VS Code` (the field, not the button) and `Checkout` are all `.lab`
  text, so on screen they are upper case; the driver could not have got past
  its first `fill`. Its `witness-unit` pin was green throughout, because it
  pinned the *markup*. Fixed with `rendered_label`, and the run now reaches the
  demo-corpus gap above.
* **Two races.** The top strip's Settings button is not in the tree the instant
  the app is frontmost — the harness waits for a *window*, and the boot screen
  is one — and Return on a launcher that has not answered selects nothing. Both
  drivers now wait for what they are about to act on.

#### What the first unlocked run broke, and what #547 fixed

* **A wait on a name nothing carries.** `open-in-editor` asked `ax find` for
  `CHECKOUT`, and `<span class="lab">Checkout</span>` is not a *name*: it names
  nothing, and its words are an `AXValue`. The panel was on screen with its
  three buttons and the count was 0, so the driver reported a working feature
  as a detail that never opened — and `witness-unit`'s pin of the heading was
  green throughout, because **pinning what the markup says measures nothing
  about what the accessibility API answers**. The heading is read with `ax
  values` now, the way `capture` has read its headings since #503.
* **And read twice.** A wait that was already true one keystroke earlier
  witnesses nothing, so the reading is taken with the launcher's hit list on
  screen *and* after Return, and `waypoint_verdict` refuses the pair unless it
  went from absent to present. An empty answer from `ax values` is
  `unreadable`, never *absent*: the helper prints nothing when it can see no
  window, and read as *absent* that would make the before-half of every
  waypoint pass on a helper that had stopped working.
* **A driver may not type a quote.** The template it typed was `"<stub>"
  {path}`, quoted against a `TMPDIR` with a space in it — and macOS' *Smart
  Quotes* substitution rewrote the straight quotes on their way into the WebKit
  field, so knobas stored `“/var/…/stub” {path}` and refused to run a program
  of that name, correctly and with the reason in the toast. **`ax type` posts
  characters the way a person's keyboard does, and the input stack rewrites
  some of them** — straight quotes, and a double hyphen, which the same
  preference pane turns into a dash. The template is one unquoted word now, and
  `path_is_one_word` refuses to type a path that would have needed the quotes.

#### What the first unlocked run broke, and what #548 fixed

* **A wait on a name three elements carry.** `capture.sh` asked `ax find` for
  `exactly_one` element called `Capture`, and the capture window is named that
  three times over — `AXWindow` title from `crates/knobas-app/src/capture.rs`'s
  `.title("Capture")`, `AXWebArea` description from `app/capture.html`'s
  `<title>`, `AXTextArea` title and description from `CaptureWindow.svelte`'s
  `aria-label`. Every one of the three is correct and every `witness-unit` pin
  over them was green. The count was 3 while the window was up, so the driver
  reported the window whose text area held the caret as *never appeared*.
* **And widening the count would not have fixed it.** `present` in place of
  `exactly_one` goes green here and goes green just as well on a main window
  that ever comes to carry that name with no capture window anywhere: the
  question *did the capture window appear?* is about a **window**, and a count
  of named elements is a representation of it. `ax windows` grew for that, and
  the driver counts windows titled `Capture` **before** the keystroke and after
  it — none, then exactly one — so the wait means the keystroke. The closing
  assertion is the same count back at zero, and `ax windows` refusing (no
  window at all: a quit app, a screen locked mid-run) reads as `x` rather than
  zero, so a dead process cannot satisfy *the capture window has gone*.

#### What is still not witnessed

* **`captured-in`, and it is not a debt.** A capture's second born link needs a
  **stored** room, and the `--demo` profile carries no context: a reader makes
  one. Making one inside `capture.sh` means driving the tab strip — another
  feature's UI — to arrange this feature's fixture, and the first attempt at it
  lost the field to a blur before its Return. **Spec #491's stream map puts the
  capture links' witness elsewhere on purpose**: row 5 reads *"app IPC suite for
  the links; desktop automation for the shortcut and window"*, and its *Primary
  seam* paragraph names *"the capture links (stored room, derived room,
  foreground present and absent)"* at *"the app's commands over a scratch
  database"*. That witness is `crates/knobas-app/tests/it/capture_ipc.rs` and
  `app/src/lib/capture/capture.test.ts`, and this driver asserting
  `CAPTURED FROM` is already one link more than that row asks of it. A driver
  step that makes a stored room is welcome later; it is owed to nothing (the
  deputy's ruling of 2026-09-08 on #503, part 5).
* **A shortcut the operating system refuses.** That needs another application
  holding a combination, which no driver can arrange on a machine it does not
  own. The refusal path is pinned at the IPC seam with a stand-in registrar.
* **Which entity a born link points at, from a driver.** Deliberately not
  asserted: the ticket's key is on the room behind the slide-over and the
  context's label is on the tab strip, so a driver reading the whole window for
  either would pass with no link drawn at all.
* **Launch Services with several copies of `dev.knobas.desktop` registered.**
  Not a gap in a driver — a prerequisite, and on this Mac it is real. The
  harness refuses unless the copy Launch Services names is the one it just
  built, and there were **ten** registered paths on 2026-09-08: `/Applications`,
  three old worktrees, the root checkout's debug and release bundles, and five
  stale DMG mount points. Three were left on 2026-09-11 (`/Applications` and
  the root checkout's two), and every run recorded above was made with the
  others unregistered (`lsregister -u <path>`) and every one that still existed
  on disk put back afterwards; nothing was moved or deleted. **A worktree's own
  bundle is left registered by the harness** — it registers what it built and
  puts back only what was registered before — so a merged-and-deleted worktree
  is where the stale entries come from. **A merge-manager
  re-running these recipes has to do the same**, or the harness will refuse with
  *Launch Services still resolves `dev.knobas.desktop` to another copy* and name
  the winner. **By default the harness is not going to do it for you, and that is ruled
  rather than unfinished** (#1176 added an opt-in,
  `KNOBAS_WITNESS_UNREGISTER_OTHERS=1`, that does it for this repo's own
  checkouts only; see precondition 4): ADR-0016's dated consequence of 2026-09-08 makes a
  clear registration one of three preconditions *"a person meets once and the
  harness probes on every run"*, and says in as many words that *"the harness
  never moves an installed knobas aside to win the registration; that is the
  runner's job, for the length of the run."* Ten registrations instead of one
  changes the **recipe**, which prerequisite 4 above now carries; it does not
  change the rule.

### Screenshots of the real app (`just live-shot`, #1483)

`just shot` photographs the frontend under `?fake-ipc`. What it cannot show is
the app against a real product, so `just live-shot <out dir> <steps file>`
starts `just demo` with `KNOBAS_SECRET_STORE=memory`, waits for the window and
sources the steps file. The file is bash with seven commands: `shot <name>`
(the knobas window only, `screencapture -l` of its window id, scaled from
Retina to points), `press <label> [n]`, `focus <label> [n]`, `click-at
<label> [n]`, `type <text>`, `key <key> [modifier...]` and `settle [s]`.
`testenv/live-shot.sh`'s header has each.

- **It drives the witness's `ax.swift`**, compiled per run like the witness
  does. A label is an accessible description, title or placeholder. Only
  *visible* matches count, because WebKit's tree reaches a table cell twice
  and keeps filtered-out rows. With several visible, `n` picks one in reading
  order; without `n` it refuses. With none in the window, a match below the
  fold counts (a dialog field under its scroller's edge), and the one picked
  is scrolled into view first. `click-at` is a pointer click at the
  element's centre, for a control only a hover reveals, and also takes words
  on screen (a radio's label, a list row's name).
- **It takes the screen and the witness's lock**
  (`$TMPDIR/knobas-desktop-witness.lock`): one witness or live-shot at a time.
  It refuses on a locked screen before every command, and `type`, `key` and
  `click-at` refuse unless knobas is frontmost.
- **`LIVE_SHOT_PRODUCT=gitea`** (or forgejo, teamcity, jira, confluence) brings
  that product up through `live_session` first. That takes the Gitea lock for
  Gitea and the JVM lock for a JVM product, and exports `./seed --env` for the
  steps file (`type "$KNOBAS_GITEA_TOKEN"`). Unset, it starts no product and
  takes no product lock.
- **The window gets the desktop witness's known size** before the steps run
  (#2056): fullscreen when AeroSpace is running, so a tiling window manager
  does not resize it between shots, and refused below 1241 points wide. See
  *The window gets a known size* under *The desktop witness (macOS)*.
- **On a failed command** it writes `failed.png` and `failed-ax.txt` (the
  window's accessibility tree) beside the shots, then quits the app with ⌘Q.
  If the app is still running after that, it is killed, and the embedded
  PostgreSQL it left on the demo profile is stopped, as the witness does (#1967).
- **The demo profile is shared.** A source a steps file adds stays in it, with
  its credentials gone at exit, so a steps file that adds one removes it again.

## Scripts

| Script | Does |
|---|---|
| `./seed` | Gitea and Kuma, in order. Idempotent — re-running is a no-op that exits 0. |
| `./seed-gitea.sh` | Org, users, repos, branches, commits, PRs, comments, reviews. |
| `./seed-kuma.sh` | Kuma admin account, monitors, API key. Owns the monitor list: what `monitors.json` no longer names is deleted. |
| `./kuma-monitor.sh` | `add <name> <url>` / `delete <name>` on one throwaway monitor, through the same socket.io channel the seed uses. Idempotent both ways. What `just kuma-live` witnesses a deletion with; not part of the baseline, so `./seed-kuma.sh` sweeps whatever it leaves. |
| `./canary.sh` | `up` / `down` / `status` on the canary's host socket, `127.0.0.1:8299`. The one thing a live run may knock over. On a remote target it runs on that machine and binds its Docker bridge. |
| `./stall.sh` | Remote target only: `up` / `down` / `url` on a socket that accepts and never answers (bridge address, port 8298), for `live_kuma`'s refresh-waits witness. `just kuma-live` binds and releases it. |
| `./fetch-timebomb-keys.sh` | Pulls the two 10-user, 3-hour Data Center timebomb keys off Atlassian's public page, checks each decodes to the right product, prints `export` lines (`--write` also drops them in the git-ignored `.env.licences`). `seed-atlassian.sh` calls it when a key is unset. |
| `./seed-atlassian.sh` | The real Jira and Confluence containers' setup wizards, unattended (`--profile real-atlassian`); `./seed --atlassian` runs the script below after it. Takes `jira` or `confluence` to walk one wizard, which is the shape a loaded machine wants — see *Jira and Confluence, end to end*. |
| `./seed-atlassian-content.sh` | The Tidewater people, projects, issues, comments, worklogs and links in the real Jira; the ENG space, pages and comments in the real Confluence. Takes `jira` or `confluence` to seed one. `--verify` reads PAY-231 and one page back, walks one throwaway ticket through `NARROW`'s workflow to prove it still narrows (the only write in that path; it deletes the ticket), and waits until a CQL search returns exactly the seeded pages and the home page; the Confluence seed deletes the `ENG` pages it did not create. `just jira-live` / `just confluence-live` run the whole window. |
| `./live` | `up` / `down <gitea\|teamcity\|jira\|confluence>` and `status`: the on-demand products (`live-lib.sh`), one JVM product at a time. `just live-up` / `live-down` / `live-status`. See *Live products, on demand*. With a remote target, also `tunnel up\|down\|status` and `remote-check` (*Live products on a remote machine (WSL)*). |
| `./live.env.example` | The template for `live.env`, which says where the live products run: here, or a remote machine. |
| `./wsl/setup-windows.ps1`, `./wsl/setup-wsl.sh`, `./wsl/connect.sh` | Setting up a WSL box as the 24/7 host of the live products, one script per machine. See *Live products on a remote machine (WSL)*. |
| `./live-test.sh` | Its unit tests against a stub `docker`: the one-JVM refusal, the remote-context refusal, what a run puts back, and that no run stops a Gitea it started, TeamCity's or Gitea's own (#698); `live.env`'s reading and a remote target's rules; the licence renewal and the one retry from empty. `just witness-unit`, and part of `just check`. |
| `./live-lock-test.sh` | The JVM lock's unit tests (#690), against a stub `docker` and a scratch lock: a second run waits for the first, a stale lock is taken over, the wait times out naming the holder. On a remote target: runs on two products go at once, two on one product take turns, a lock with no product is none, and the memory section serialises its takers. `just witness-unit`, and part of `just check`. |
| `./seed-teamcity.sh` | The real TeamCity container's first start, an access token and one authorised agent (`--profile real-teamcity`); then runs the script below. |
| `./seed-teamcity-builds.sh` | The Tidewater projects, build configurations, VCS roots and builds in the real TeamCity; `--running` for the fixture's running build. |
| `./pin-images.sh` | Re-resolve image tags to digests into `images.env`; guarded pins are held, `--move VAR` takes a new one. |
| `./check-ports.sh` | Assert the compose file against the §5 port table, default profile, opt-in profiles and the capped overlay. Starts nothing. |
| `./desktop-witness.sh` | The desktop witness (#500): builds and signs the debug bundle, launches the copy Launch Services registered, runs one driver from `desktop-witness/drivers/` against the real accessibility tree, quits. Takes the screen; one at a time; not part of `just check`. See *The desktop witness (macOS)*. |
| `./live-shot.sh` | Screenshots of the real app against live products (#1483): `just demo` with in-memory secrets, a steps file of `shot`/`press`/`focus`/`click-at`/`type`/`key`, the knobas window only. Takes the screen and the witness's lock; not part of `just check`. See *Screenshots of the real app*. |
| `./desktop-witness-test.sh` | The witness's own unit tests, over `desktop-witness-lib.sh` -- the Launch Services path comparison, the accessibility probe's reading, and the pure halves of the `open-in-editor` and `capture` drivers, and that every driver's landing has its DOM replay in `App.test.svelte.ts` (#1968). No screen, no bundle, no macOS. `just witness-unit`, and part of `just check`. |
| `./reset` | `down -v` every profile, and delete the seed's outputs. |

One script outside this folder belongs to the same family, since it measures the app rather than a product:

| Script | Does |
|---|---|
| `scripts/keep-alive-budget.sh <slot>` | Keep-alive's budget and memory witness (#2250): `just keep-alive-budget <slot>`. Headless Chrome against the `?fake-ipc` dev server on `5300 + slot` (reused when one is up), debug port `9400 + slot`. Times the return to the repository page's Files tab, a 10,000-line file open (`?fake-file-lines=10000`), after Commits: 7 runs, exit 1 when the median is over 50 ms (`KEEP_ALIVE_RETURNS`, `KEEP_ALIVE_BUDGET_MS`). Then the JS heap after a forced garbage collection and the DOM node count with five pages held (that page, Inbox, Builds, Docs, Time), at *Pages kept open* 0 and 5 (`?fake-pages-kept-open=<n>`), twice each. It prints `scripts/machine-load.sh`'s line first: a timing on a machine that line calls over is quoted beside it. Starts no product; not part of `just check`. |

## mockd's documented deviations

mockd is deliberately **stricter** than the real products in several places, so
that an adapter which passes here passes there — never the reverse. The list is
maintained in one place and must not be copied:

> **`crates/knobas-mockd/src/lib.rs`**, the *Documented deviations* section of
> the module docs.

Read it before concluding that mockd is wrong.
