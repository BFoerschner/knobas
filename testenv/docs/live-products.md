# Live products, on demand

The live suites run against real Gitea, Forgejo, TeamCity, Jira and Confluence in Docker. On this machine the products run on demand. A live recipe starts the product it needs, seeds it, runs its suites and puts the product back as it found it (Björn, 2026-10-12, #633). Read this page when you start or stop a product by hand, when a run waits or is refused, or when the app must clone from a live forge over https.

The page describes the local target, which is the default. With a remote target in `testenv/live.env` the products stay up around the clock and some rules here change. [What changes on a remote target](#what-changes-on-a-remote-target) lists them.

Commands on this page run in `testenv/`. An agent's shell starts at the repository root, where a hook refuses `cd testenv && ...`, so an agent prefixes each command with `env -C testenv`. A `just` recipe needs no prefix at the repository root.

## Run one live suite

Each live recipe brings its product up itself, so one line is usually the whole job.

```sh
just teamcity-live-seeded   # Gitea and TeamCity up and seeded, two suites, TeamCity stopped again
just jira-live              # Jira and its PostgreSQL, setup wizard, content, three suites, removed again
just confluence-live        # the same for Confluence
just atlassian-live         # jira-live, then confluence-live, never the two at once
just gitea-live             # Gitea up and seeded, four suites
just gitea-live forgejo     # the same four suites against Forgejo
```

`just gitea-live-capped` and `just start-work-live` start Gitea the same way. [The live recipes](../README.md#the-live-recipes) lists every recipe.

A run puts its product back as it found it. If the run started the product, it stops it at the end. If the product was already up, it stays up. The put-back runs from a trap on exit, Ctrl-C and SIGTERM, so a failed or interrupted run still does it. Gitea is the one exception. See [Gitea stays up after a run](#gitea-stays-up-after-a-run-698).

In `atlassian-live` a failure in `jira-live` stops the run before `confluence-live`.

## Keep a product up for several runs

Start the product once and the recipes leave it up.

```sh
just live-up jira       # runs ./live up jira: start, wait, seed
just jira-live          # about 60 s instead of about 5 min, because only the suites run
just live-down jira     # runs ./live down jira
just live-status        # one line per product, up or down
```

The product is `gitea`, `forgejo`, `teamcity`, `jira` or `confluence`. [The products table](../README.md#live-products-on-demand) says what `live-up` starts for each one, what seeds it and what `live-down` does to it. Three things the table leaves out:

- `live-up gitea` and `live-up forgejo` also start [the https front](#the-https-front).
- `live-up confluence` also starts `knobas-docs-assets` on port 8095, the static server the Confluence showcase's embeds load (#2456). `live-down confluence` removes it.
- Every seed is idempotent. `live-up` on a product that is already up re-checks it and writes nothing.

`live-up` of a JVM product keeps the JVM lock until `live-down`, so run `just live-down <product>` when the session is over. [A product started with live-up keeps the lock](#a-product-started-with-live-up-keeps-the-lock-2121) explains why.

Jira and Confluence run on 3-hour timebomb licences. A restart renews the licence and keeps the data (#2373). When a run or `live-up` finds one of them up with less than `KNOBAS_LIVE_LICENCE_MARGIN_S` seconds of licence left, 3600 by default, it restarts the product before it re-seeds. If the restart does not move the expiry, it removes the product with its volumes and sets it up from empty. If a seed fails against an instance that was already there, the run retries it once from empty. [How Jira and Confluence stay licensed](../wsl/README.md#how-jira-and-confluence-stay-licensed) has the detail.

## Stop a product

`live-down` stops Gitea, Forgejo and TeamCity and keeps their volumes. The volumes hold the seeded content and the seeded runs, so the next start finds them seeded in seconds. `live-down teamcity` stops the server and its agent and leaves Gitea up.

`live-down` removes Jira and Confluence with their volumes, so their next start is from empty. A kept instance would come back to whatever a run left in it. Their block in `seed-state.json` goes too, so `./seed --env` stops printing a URL nothing answers.

Do not remove them by hand with `docker compose down -v`. A `down` with services named still removes the project network when no running container is on it. A stopped Gitea, TeamCity or Kuma is still attached to that network by id, and its next `up` fails with "network ... not found". `live-down` uses `docker compose rm` and removes the two named volumes instead.

Uptime Kuma's `teamcity`, `jira` and `confluence` monitors reach the products by compose service name. Each one is red while its product is down, which is expected. The [estate and monitors page](estate-and-monitors.md) describes the monitors.

## One JVM product at a time

TeamCity, Jira and Confluence are JVM products, and only one of them may be up on this machine. The Docker VM here has 8 GB. Jira wants about 4 GB, Confluence about 2 GB, and TeamCity's server and agent about 2.2 GB. Two JVMs that start together and claim their heaps died twice under load (#313).

`live-up` and every recipe that starts a JVM product refuse it while another of the three is up. The refusal is one line that names the product that is up:

```text
live: knobas-teamcity is running; one JVM product at a time on the 8 GB Docker VM -- 'just live-down teamcity' first
```

The refusal comes before any `docker compose` call, so a refused start leaves nothing half-created. A run never stops the other product itself, because that would end somebody's session under them. Gitea and Forgejo are not JVM products and are never refused.

## How runs take turns

Several worktrees may implement JVM tickets at once. Their live runs queue on a lock, and only the few minutes each one spends in a product are serialised. The refusal above is the backstop behind that queue.

| Run | Lock it holds |
| --- | --- |
| `teamcity-live`, `teamcity-live-seeded`, `jira-live`, `confluence-live`, `atlassian-live` | The JVM lock, for the whole run |
| `live-up teamcity`, `live-up jira`, `live-up confluence` | The JVM lock, kept until `live-down` |
| `gitea-live`, `gitea-live forgejo`, `gitea-live-capped`, `start-work-live` | The Gitea lock, for the whole run |
| `live-shot` with `LIVE_SHOT_PRODUCT` set | The lock of that product, JVM or Gitea |
| `live-up gitea`, `live-up forgejo`, the Gitea a TeamCity run starts | None |
| A product started by hand with `docker compose`, a suite run by hand with `cargo test` | None |

`teamcity-live` starts no product. It takes the lock because a `.env` that points at the seeded TeamCity makes it a TeamCity run like any other.

### The JVM lock (#690)

The JVM lock is `$TMPDIR/knobas-live-jvm.lock`, one for the whole machine. A run takes it first and holds it through start, seed, suites and the closing `live-down`. A run that finds it held waits. It says once who holds the lock and since when, and it says when it gets it:

```text
live: teamcity-live-seeded: the JVM lock is held by 'teamcity-live-seeded' in /Users/dev/knobas/.worktrees/issue-683 (pid 51234, since 14:02:11); waiting up to 2700s
live: teamcity-live-seeded: took the JVM lock after 312s
```

While it waits it reports every 300 s. It goes on when the holder lets go, so it never meets the refusal the first run's product would cause. After 45 minutes it gives up and refuses, naming the holder. `KNOBAS_LIVE_LOCK_WAIT` sets that wait in seconds.

The next run takes over a lock whose holder is dead, such as a run that was SIGKILLed, and says so.

The lock is a symlink whose target names the holder as `<pid> <epoch> <recipe> <worktree>`. To see who holds it:

```sh
readlink "$TMPDIR/knobas-live-jvm.lock"
```

`atlassian-live` takes the lock once and both of its halves run under it, so no other JVM run gets in between Jira's removal and Confluence's start.

### A product started with `live-up` keeps the lock (#2121)

A suite run by hand takes no lock. If `live-up` released the JVM lock after seeding, another worktree's recipe could take it, find the product up and edit the same seeded data mid-run. So `live-up teamcity`, `live-up jira` and `live-up confluence` hand the lock to a detached keeper. The keeper holds it as `live-up-<product>-kept` in the worktree that ran `live-up`, for as long as the product is up. `live-up` ends with this line:

```text
live: jira stays up and holds the JVM lock until 'just live-down jira'; this worktree's runs go on under it, other worktrees' wait
```

- A recipe in the same worktree goes on under the kept lock and prints `... runs under the JVM lock this worktree's live-up keeps`.
- A run from any other worktree waits as it would for any holder. Its waiting line ends in `'just live-down <product>' from any worktree frees it`.
- `just live-down <product>` ends the keeper and frees the lock, from any worktree.

Run `just live-down <product>` when the session is over. Until then every other worktree's JVM run queues behind it.

A forgotten keeper lets go by itself in four cases.

| What happens | When the lock is free |
| --- | --- |
| The product goes away some other way, removed by hand or by a VM restart | Once the keeper has found it down on two looks in a row. It looks every `KNOBAS_LIVE_KEEPER_POLL` seconds, 30 by default. |
| The keeper's worktree is removed | At the keeper's next look. The product stays up. |
| `KNOBAS_LIVE_KEEPER_MAX` seconds pass, 10800 by default, the life of an Atlassian licence | At the keeper's next look. The product stays up for the next run. |
| The keeper is killed | When the next run takes the dead holder's lock over |

### The Gitea lock (#724)

Two Gitea runs from two worktrees break each other. One run deletes its own `knobas-edit-...` branch at its end while the other run's full sync walks the repository, and the suites that read the whole fixture assert on counts and states the other run is changing. So `gitea-live`, `gitea-live-capped` and `start-work-live` hold `$TMPDIR/knobas-live-gitea.lock` for their whole run. `gitea-live forgejo` takes the same lock.

It works like the JVM lock. The waiting line says "Gitea lock", a dead holder's lock is taken over, and the wait ends in the same 45-minute refusal. It is a separate lock because a Gitea run needs none of the VM's memory, and a two-minute Gitea suite should not queue behind an Atlassian run.

A TeamCity run starts and seeds Gitea without the Gitea lock, and so does `live-up gitea`. The seed writes nothing to a seeded Gitea.

## Gitea stays up after a run (#698)

No recipe stops Gitea at the end of its run. Only `just live-down gitea` does. A run that finds Gitea down starts it and leaves it up, whether it is a Gitea run or a TeamCity run whose VCS roots clone it. It says so in one line:

```text
live: gitea was started for this run and stays up after it (#698; other worktrees' runs may share it) -- 'just live-down gitea' stops it
```

A TeamCity run takes no Gitea lock. By the time a run ends, a run from another worktree may be using the same Gitea, and stopping it would pull Gitea out from under that run mid-suite. Nothing needs Gitea down. It is light, about 130 MB with no JVM, and the one-JVM rule does not count it.

## Run the Gitea suites against Forgejo

Forgejo is the hard fork of Gitea, and knobas expects to read it through the same adapter (#1195, spec #1192 story 18).

```sh
just gitea-live forgejo
```

The recipe does five things:

1. It starts the pinned Forgejo image, `FORGEJO_IMAGE` in `images.env`, version 17.0.0 by digest. The service is in profile `forgejo`, on port 3002, with the volume `forgejo-data`.
2. It seeds Forgejo with `seed-forgejo.sh`, which is the Gitea seed run with `KNOBAS_SEED_SERVICE=forgejo`. The seed keeps its state in the git-ignored `seed-state-forgejo.json`, so Gitea's token and id map are never touched.
3. It exports the `KNOBAS_GITEA_*` variables that `./seed --env-forgejo` prints, and `KNOBAS_GITEA_SEED_STATE` with them.
4. It runs the same four suites as `just gitea-live`.
5. It stops Forgejo again if the run started it.

To keep Forgejo up by hand, use `just live-up forgejo` and `just live-down forgejo`.

Forgejo's settings differ from Gitea's in one place. `FORGEJO__migrations__ALLOW_UNENCRYPTED` is on, because Forgejo refuses the seed's http pull mirror without it. `gitea-live-capped` and `start-work-live` have no Forgejo form.

## What the Gitea seed adds for teams and languages (#1045)

For each repository in `fixtures/tidewater/work.json`, `seed-gitea.sh` does two things before it cuts any branch:

- It puts the repository in the organization team its `team` names. The team has no members. It sits beside Owners, which knobas leaves out.
- If the repository's `lang` is Rust or Kotlin, it commits one source file to `main` so Gitea's language statistics find it. The file is `src/main.rs` for Rust and `src/main/kotlin/Main.kt` for Kotlin. Markdown counts for nothing in the statistics and gets no file.

The seed skips both where they are already there.

## A leftover `knobas-*` docker context is refused

`live-up`, `live-down` and every recipe that starts a product refuse a docker context named `knobas-*`. Until #634 such contexts reached three Hetzner servers over ssh, and one left active on a machine would send these calls somewhere else.

```text
live: the docker context is 'knobas-jira', left from the decommissioned Hetzner servers (#634); live products run locally -- docker context use orbstack
```

Switch back with `docker context use orbstack`. The one remote engine allowed is a declared one, in `testenv/live.env`.

## Run `estate-live` against Hetzner Cloud

The hcloud importer reads Hetzner Cloud, the one real system here that is not a container. Its project has been empty since #634, so an ordinary run declares no server:

```sh
ESTATE_LIVE_HCLOUD_SERVERS=0 just estate-live
```

A PR that touches the hcloud producer, `crates/knobas-app/src/assets/hcloud.rs` or the code it calls, owes a run at `ESTATE_LIVE_HCLOUD_SERVERS=1`. That run is Björn's. He creates the smallest server in the knobas project, runs the recipe and deletes the server. No agent and no recipe creates or deletes a server (the deputy's ruling on #634).

## What changes on a remote target

`testenv/live.env` with `KNOBAS_LIVE_TARGET=remote` and `KNOBAS_LIVE_SSH=<ssh host>` sends every `docker` call to that machine's engine. [Live products on a remote machine (WSL)](../wsl/README.md) covers that target in full. Four things on this page change:

- No run stops a product at its end. The products stay up until the memory budget needs their room.
- The one-JVM refusal is off. `KNOBAS_LIVE_ONE_JVM` overrides the default on either target.
- Each JVM product has its own lock, `$TMPDIR/knobas-live-<product>.lock`. `atlassian-live` takes no lock there, because each half takes its own. The Gitea lock is unchanged.
- One ssh tunnel forwards the product ports to this machine's `127.0.0.1`, so every URL stays the same. A local run refuses while that tunnel is open.

## Variables that set the waits

Set these in the environment, or as `KNOBAS_LIVE_*=value` lines in `testenv/live.env`. The environment wins.

| Variable | Default | What it sets |
| --- | --- | --- |
| `KNOBAS_LIVE_LOCK_WAIT` | `2700` | Whole seconds a run waits for a lock before it refuses |
| `KNOBAS_LIVE_LOCK_POLL` | `2` | Seconds between two tries at a held lock |
| `KNOBAS_LIVE_KEEPER_POLL` | `30` | Seconds between two looks of a `live-up` keeper at its product |
| `KNOBAS_LIVE_KEEPER_MAX` | `10800` | Seconds after which a keeper lets go of the lock |
| `KNOBAS_LIVE_GITEA_WAIT_S` | `60` | Whole seconds Gitea has to answer from the host after its container is up |
| `KNOBAS_LIVE_LICENCE_MARGIN_S` | `3600` | A Jira or Confluence with fewer seconds of licence left is restarted before it is re-seeded |
| `KNOBAS_LIVE_API_WAIT_S` | `300` | Seconds the Jira or Confluence REST API has to answer the admin with 200 before the content seed. Unset, the wait after a restart that renews the licence is 600. |

## Tests that hold these rules

Two scripts test `live-lib.sh` against a stub `docker`. Neither starts a container. Both run in `just witness-unit`, which `just check` runs.

| Script | What it holds to account |
| --- | --- |
| `./live-test.sh` | The one-JVM refusal, the `knobas-*` context refusal, what a run puts back, Gitea outliving the run that started it, the licence renewal, the remote target's memory budget, the https front stopping with the last forge, and the name constraints of the front's root |
| `./live-lock-test.sh` | A second run waits and goes on. A dead holder's lock is taken over. The wait ends in a refusal that names the holder. A session takes the lock before the guard and releases it after the put-back. A `live-up` keeper holds the lock against other worktrees until `live-down`. A Gitea session queues on the Gitea lock and never on the JVM lock. |

## When a start, a stop or a lock goes wrong

| Symptom | Cause | Fix |
| --- | --- | --- |
| `live: knobas-<x> is running; one JVM product at a time ...` | Another JVM product is up outside the queue. Somebody started it by hand with `docker compose`, or a `live-up` keeper let go and left it up. | When nobody is using it, `just live-down <product>`, then run again. |
| `... the JVM lock is held by ...; waiting up to 2700s` | Another run or a `live-up` keeper holds the lock. | Wait. If the line ends in `'just live-down <product>' from any worktree frees it` and that session is over, run that command. |
| `... is still held by ... after 2700s; giving up` | The holder kept the lock longer than `KNOBAS_LIVE_LOCK_WAIT`. | Run again when the holder is done, or set a longer `KNOBAS_LIVE_LOCK_WAIT`. |
| A run waits on a lock, and the pid in the line is not a live run | The dead holder's pid was recycled onto an unrelated process. The lock cannot see that. | Remove the link, for example `rm "$TMPDIR/knobas-live-jvm.lock"`. |
| `... is not the JVM lock's symlink; remove it` | A file or a folder sits at the lock's path. | Remove it. |
| `up` fails with "network ... not found" | A `docker compose down` removed the project network while a stopped container was still attached to it. | `docker compose --env-file images.env up -d --force-recreate <service>`, then `stop` and `up` of the same service once more for OrbStack's port forward. |
| `live: the docker context is 'knobas-...' ...` | A docker context left from the Hetzner servers is active. | `docker context use orbstack` |
| `live: gitea: the container is up but http://127.0.0.1:3000 does not answer ...; not holding the Gitea lock, so not stopping it` | A recreate of the Gitea container killed OrbStack's port forward. `live-up gitea` and a TeamCity run hold no Gitea lock, so they do not stop Gitea to heal it. | `just gitea-live`, which heals it with a stop and an up under the lock. If Gitea only answers late under load, set a longer `KNOBAS_LIVE_GITEA_WAIT_S`. |
| `live: forgejo: the container is up but the host does not reach it on port 3002` | The same dead port forward, for Forgejo. | `just live-down forgejo`, then `just live-up forgejo`. |
| `live: an ssh tunnel to a remote target holds this machine's product ports ...` | A remote run left its tunnel open, and this run targets this machine. | `./live tunnel down`, or run with `KNOBAS_LIVE_TARGET=remote`. |

## The https front

knobas clones and fetches over https or ssh only ([ADR-0023](../../docs/adr/0023-knobas-clones-a-repository-into-the-clones-root.md), [ADR-0035](../../docs/adr/0035-a-pull-request-is-opened-in-a-worktree-under-the-clones-root.md)), and Gitea and Forgejo serve http. So a Caddy container, `https-front`, terminates TLS in front of both forges on the compose network (#2377). Its container is `knobas-https-front`, it is in the default profile, and it takes about 20 MB.

| Address | Forge |
| --- | --- |
| `https://127.0.0.1:3443` | Gitea |
| `https://127.0.0.1:3445` | Forgejo. It answers 502 while Forgejo is down. |

- The front starts with either forge: `live-up gitea`, `live-up forgejo` and every run that brings one up. It stops with the last of them.
- The seeds and the suites keep using the http ports, 3000 and 3002.
- `just live-sources` registers Gitea and Forgejo at the https addresses, so *Clone* and a pull request's worktree fetch work from the app.
- On a remote target the tunnel forwards both ports. A tunnel that was opened without them adds them the next time a run finds it up, without going down.

The ruling on #2377 chose a front over two alternatives. Gitea's own HTTPS would move the http address that every seed, every suite and every TeamCity VCS root uses. An ssh clone needs the source's typed address to be ssh, which the adapter's API calls cannot use.

### Trust the front's CA once

The front has no CA of its own. Until the command below has run, the front waits and serves nothing.

Before you start:

- The front must be up. `just live-up gitea` starts it.
- The step runs on the Mac. macOS may ask for the login password.

```sh
just live-trust-ca
```

The command does this:

1. If the front's volume holds no root, it generates one on the Mac with openssl: P-256, ten years, CN `knobas testenv CA, loopback only (<target>)`, where `<target>` is the remote target's ssh host, or `local`. The root carries critical name constraints that permit only `127.0.0.1`, `::1` and `localhost`.
2. It puts the root and its key into the `https-front-data` volume at `/data/knobas-ca`, so a recreate of the container keeps them. The key stays in that volume only.
3. It refuses a root in the volume that is not constrained that way.
4. It takes every knobas testenv root that is not constrained, and this target's earlier roots, out of the login keychain.
5. It trusts the root for SSL with `security add-trusted-cert -r trustRoot -p ssl -k ~/Library/Keychains/login.keychain-db <root>`. knobas (rustls with the platform verifier), Apple's git and `/usr/bin/curl` all read that trust setting.
6. It tries `https://127.0.0.1:3443` up to 30 times, a second apart, until the certificate verifies.

Caddy issues its certificates from an intermediate it makes under that root. Nothing turns verification off.

Run the command once per front volume and once per target, because each target has its own root. A second run on a front that already verifies changes nothing. It generates a new root only after the volume is removed.

### Why the root is name-constrained

Whoever holds a trusted root's key can mint certificates this Mac accepts, and the key lives on the target. On a remote target that is the WSL box's Docker volume. An unconstrained root would let the key holder impersonate any https site to this Mac. The constrained root is good for the loopback names only, and macOS enforces the constraint on the whole chain. A `github.com` leaf signed by the front's own intermediate fails `security verify-cert` (ruling on #2377).

The first front used Caddy's own unconstrained root, `knobas testenv CA - 2026 ECC Root`. `just live-trust-ca` takes it out of the login keychain if it is still there.

### Remove the trust by hand

Use these to remove one root yourself, or to drop the front's trust altogether.

```sh
security find-certificate -a -c "knobas testenv CA" -Z ~/Library/Keychains/login.keychain-db   # SHA-1 of each
security find-certificate -c "<name>" -p ~/Library/Keychains/login.keychain-db > /tmp/ca.pem
security remove-trusted-cert /tmp/ca.pem
security delete-certificate -Z <sha1> ~/Library/Keychains/login.keychain-db
security dump-trust-settings          # lists no knobas testenv root any more
```

### When the front does not verify

| Symptom | Cause | Fix |
| --- | --- | --- |
| Nothing answers at `https://127.0.0.1:3443`, and the front's log says `no root in /data/knobas-ca yet` | The front waits for its root. | `just live-trust-ca` |
| `just live-sources` prints `this Mac does not trust https://127.0.0.1:3443's certificate yet` | The trust step has not run on this Mac for this target. `live-sources` adds the source all the same. | `just live-trust-ca` |
| `live: the https front (knobas-https-front) is down; 'just live-up gitea' starts it` | No forge is up, so the front is stopped. | `just live-up gitea`, then `just live-trust-ca` |
| `live: the front's root (/data/knobas-ca/root.crt) is not name-constrained ...; not trusting it` | The root in the volume does not carry the loopback constraints. | The command generates a root only when the volume holds none, so a constrained root needs a new volume. |
| `live: the root is trusted but https://127.0.0.1:3443 still fails verification` | After 30 tries the front still serves no certificate that verifies. | Read `docker logs knobas-https-front`, as the message says. |
| `https://127.0.0.1:3445` answers 502 | Forgejo is down. | `just live-up forgejo` |
| `live: the open tunnel could not add 127.0.0.1:<port>` | On a remote target, the open tunnel could not take on a port. | `./live tunnel down`, then run again. |
