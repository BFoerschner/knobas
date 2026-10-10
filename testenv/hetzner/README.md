# The real products on Hetzner

Since 2026-09-06 the three opt-in real products -- TeamCity, Jira, Confluence --
run on three Hetzner servers, one product per server, instead of in the
notebook's 8 GB Docker VM: a cx23 (2 Intel vCPU, 4 GB, ~EUR 6.50/month) for
TeamCity and a cpx22 (2 AMD EPYC Genoa vCPU, 4 GB, ~EUR 23/month) each for
Jira and Confluence, all billed hourly, about EUR 53/month together. Gitea, Uptime Kuma and mockd stay on the notebook: they are small,
and nothing on the servers depends on them except TeamCity's VCS roots, which
the tunnel covers (below).

| server | type | runs | compose profile | notebook port |
|---|---|---|---|---|
| `knobas-teamcity` | cx23 | teamcity, teamcity-agent | `real-teamcity` | 127.0.0.1:8111 |
| `knobas-jira` | cpx22 | jira, jira-db | `real-atlassian` | 127.0.0.1:8080 |
| `knobas-confluence` | cpx22 | confluence, confluence-db | `real-atlassian` | 127.0.0.1:8090 |

One product per server because 4 GB holds one JVM and its database, not two
JVMs; the 4 GB swap cloud-init adds is margin, not a plan. The Atlassian pair
went through three types in one day, and the numbers decided: Jira to
FIRST_RUN took 56 s on the notebook, **147 s on a cx23, 174 s on a cx33** (no
faster with twice the cores, zero steal: the start is single-threaded and the
cx line is a 2.1 GHz Skylake), and **72 s on a cpx22**. cpx22 is the cheapest
AMD type still offered; cpx21 and cpx31 answer "unsupported location"
everywhere in the EU.

## The estate file

`estate.json`, next to the host list above, is that estate written down: the
notebook and its OrbStack Docker engine, the three servers with their type,
address and role, every container and database on them, and the tunnel's
forwards as routes with the asset each one lands on. Environment (`dev`) and
owner are set once at the root and inherited from there; each asset also names
the Uptime Kuma monitors that watch it, by their Kuma name, which the import
resolves into `monitored-by` links once the monitors are mirrored (M4.1).

It exists because from M4 the estate knobas models **is** this infrastructure
and not a fictional one (#427): the same file is the Import's input, what
`--demo` loads, and the witness that the asset model fits an estate that
exists. So it is not documentation that may rot quietly -- it is a fixture, and
editing anything here without editing it is a change to the estate that knobas
has not been told about.

Since #440 it has **three** readers, and all three read these bytes rather than
a copy of them: `knobas_app::sources::demo` imports it on every `just demo`,
`crates/knobas-app/tests/estate_exit.rs` runs M4.0's exit checklist over it, and
`app/src/lib/shell/dev/fake-tauri.ts` -- the `?fake-ipc` browser harness --
draws its Tree from it, so a QA screenshot and the demo profile show the same
estate. Editing this file moves all three at once, which is the point; what
holds it to a shape is `crates/knobas-core/tests/it/estate_file.rs`, below.

Ids are stable slugs in the asset namespace (`asset:knobas-jira`), because an
import keeps them and re-importing recognises them by id. Where a server and
the container on it share a name, the server is the `hetzner-` one
(`asset:hetzner-jira` is the cx server, `asset:knobas-jira` the Jira container
on it). The PostgreSQL containers are typed `container` and hold their one
`database`; nothing is typed `database_server`, because here the container *is*
the server and a node for each would be the same fact twice.

`crates/knobas-core/tests/it/estate_file.rs` keeps it honest in `just check` --
with no container and no database. It asserts that every id is unique, that
every parent and every route target resolves, that the assets form one tree
with one root, that every type is in the built-in table, and that no entry
carries a key the file does not define, since a misspelled `monitors` reads as
an absent one. It also parses the host-list table above and the compose file's
service list, so adding a fourth server or a new service and forgetting the
estate file is a red gate rather than a discovery months later, and a server
whose type or profile changes in one place and not the other is too, as is a
service recorded on the wrong host. The two things it cannot check are the
addresses and the `hcloud_id`s: `hosts.env` is gitignored, so the public IPs in
`estate.json` are the only committed copy of them, and the ids have no other
copy at all -- `hcloud server list` is what settles a disagreement about
either. The `hcloud_id` on each server (#508) is that server's **origin key**
(`CONTEXT.md`): what the hcloud importer's produced file matches this server on
when the id it invented is not one the tree holds.

A wrong one is invisible to `just check` -- a missing or a duplicated one is
red there, but no gate can read hcloud -- and it is **not** invisible to the
milestone. The witness is `just estate-live` (#509, built), and it is what the
three ids and every property below rest on: a server whose `hcloud_id` here is
wrong is unknown by id and unmatched by key, so the produced file previews it
as *new* against an estate that must preview all-known, and the recipe goes
red. So a red `estate-live` naming one server is read as a wrong value in this
file before it is read as a bug in the producer.

**What that recipe's hcloud half measures is wider than the three ids.** It
previews the produced file and requires *no changes*, so every property the
hcloud importer writes has to be the key this file carries with the value this
file carries: `hcloud_id`, `server_type`, `os` (hcloud's image name, where the
`vm` type declares it), `location`, `ip`, and one property per Hetzner
**label** -- `knobas: testenv` and `role: <product>` on all three. A server
resized in Hetzner, moved to another location, rebuilt on a newer image,
relabelled, or given a new address turns `estate-live` red until this file
follows. The properties the three **servers** carry that hcloud has never heard
of -- `vcpu`, `memory_gb`, `disk_gb`, `ssh_host`, `docker_context`,
`compose_profile` -- are not in the produced file and are not measured by it: an
import is silent about what its file does not mention.

**Since #510 the recipe has a second half, and it measures the containers.** The
Docker importer reads every `container_engine` asset here that carries a
`docker_context` and spawns `docker --context <that> ps --format json` once per
engine, so four things in this file are now under it:

- **Every `container_engine` needs a `docker_context`.** `asset:orbstack-docker`
  gained one (`orbstack`) with #510; the three Hetzner engines have had theirs
  since M4.0. An engine without one is *skipped* -- named in the answer, its
  containers absent from the file -- and `estate-live` goes red on the skip.
- **Every container needs `docker_context` and `container_name`**, which
  together are its [origin key](../../CONTEXT.md) -- a container's docker id
  changes on every recreate and its name does not. Both are **properties**,
  because that is what the Import's second matching rule reads; the name is
  therefore written twice, once as the entry's name and once as the property,
  and `crates/knobas-core/tests/it/estate_file.rs` holds the two equal and holds
  each container's context equal to its engine's. That check is the one thing
  the `hcloud_id`s do not have: a container's key is implied by the rest of this
  file, so a hand edit that renames one and forgets the other is red in `just
  check` rather than an hour later on a live recipe.
- **A running container that is not written down here is red.** The producer
  reads `docker ps` -- the *running* ones -- so a container started on one of
  these engines and never recorded previews as *new*.
- **A container recorded here and not running is not red**, and that is the
  asymmetry to know: `asset:knobas-mockd` is not running (ADR-0013) -- no
  container by that name is on the notebook's engine at all today, exited or
  otherwise -- and this recipe says nothing about it, the same silence an
  import keeps about anything its file does not mention.
- **Reading `docker ps -a` instead would turn this recipe red**, which is why
  the producer reads the running ones. `docker --context orbstack ps -a` on
  2026-09-08 listed `knobas-teamcity` (*Exited (0) 2 days ago*) and
  `knobas-teamcity-agent` (*Exited (143)*) on the notebook, left behind by the
  local `real-teamcity` profile; this file records those two names only under
  `docker_context: knobas-teamcity`, so under `-a` they would be produced with
  the key (`orbstack`, `knobas-teamcity`), match nothing, and preview as
  *new*. (`kuma-seed` is not the example anyone should reach for:
  `testenv/seed-kuma.sh` runs it with `--rm`, so it leaves nothing behind.)

What the Docker half does **not** write, and therefore does not measure: `image`
and `compose_service`. `docker ps` reports `Image` as the image *reference* on
the notebook (`gitea/gitea`) and as the image **id** on the three ssh contexts
(`e34446c9dbf8` and friends, whose images compose pulled by digest), so writing
it would put six containers into *would change* with a hash on every run;
`compose_service` is one orchestrator's label and this file's own note about how
a container is started, not a fact docker owns.

**No tunnel is needed for either half**, which is a reading of what the two
things carry and not a run with the tunnel down (2026-09-08): `ssh -G
knobas-jira` resolves to `46.224.125.111` port 22 out of the block above, and
the three `ssh -N` processes `./tunnel up` starts carry `-L
127.0.0.1:8111|8080|8090` and TeamCity's one `-R` and nothing else -- so a
`docker --context knobas-jira` call goes to the server's own address and no
forward is in its path. What the recipe does need is the SSH key, the four
contexts, and the docker CLI on the PATH.

Nothing in the file is provisional any more, and the two things that were are
named here because their settling is what the surrounding tests now rest on.
The **type ids** are settled: #428 landed the built-in table as
`knobas_core::asset::TYPES`, took this file's spellings verbatim, and
`estate_file.rs` reads the types off that table instead of keeping a copy of
the list.

The **monitor names** were a guess until #441 and are not one any more. That
ticket replaced `testenv/monitors.json`'s two Tidewater checks with the estate's
own eight, so every name a `monitors` key here lists is a monitor the local
Uptime Kuma actually holds after `./seed-kuma.sh` -- the seven the assets here
name, plus the `canary`, which watches nothing and so appears in no asset. The two files are
each other's only check: `estate.json` names monitors by their Kuma name because
that is what the import resolves them by (#445), so renaming one without the
other silently unresolves a link. `../README.md`, "Monitors", is the list.

## How it fits the existing scripts

Nothing in `../docker-compose.yml`, the seeds or the `justfile` changed. Three
pieces make them run against the servers:

- **Docker contexts** `knobas-teamcity`, `knobas-jira`, `knobas-confluence`,
  each `ssh://` to its server. `docker compose` runs on the notebook, the
  engine is remote, so the compose file, `../.env`'s image pins and the licence
  variable are all read here and only the containers are there.
- **`bin/docker`**, put first on PATH by `source testenv/hetzner/env`. A docker
  call that names one of the products' containers or services runs under that
  product's context; any other call runs on the local engine untouched. So
  `docker inspect knobas-jira` in `seed-atlassian.sh` reaches the Jira server,
  `docker compose --profile real-teamcity up -d teamcity teamcity-agent` reaches
  the TeamCity server (with `teamcity.override.yml` layered on), and `docker
  compose exec gitea ...` and `docker ps` stay local. The one call that names
  two products -- the recipe's `down -v jira jira-db confluence confluence-db`
  -- runs once per server with that server's services. It is a shim, and shims
  guess: it looks only at bare service and container names, never at flags, so
  a hand-typed call mixing a local service with a remote one is not something it
  handles. It refuses a non-compose call that names two servers.
- **`./tunnel up`**: one `ssh -N` per server forwarding the notebook's
  127.0.0.1:8111/8080/8090 to the same port on the server, so the seeds, the
  live suites, `seed-state.json`'s URLs and a browser are all unchanged. The
  TeamCity tunnel also carries a *reverse* forward: the server's docker bridge
  gateway `:3000` to the notebook's Gitea, and `teamcity.override.yml` maps the
  name `gitea` to that gateway in both TeamCity containers. The VCS roots
  `seed-teamcity-builds.sh` writes as `http://gitea:3000/...` therefore resolve,
  and a build clones through the tunnel. **A build queued while the tunnel is
  down fails at checkout**; the server itself is fine.

  Since 2026-09-06 the local Uptime Kuma watches both halves of this: a **ping
  per server** by the IP in `hosts.env` (the firewall opens only 22 and ICMP,
  and the Kuma container can send ICMP — `../README.md`, "Monitors", has the
  measurement), and an **HTTP check per product through the forward**, named
  `(tunnel)` because it falls with the tunnel. So a tunnel that died with the
  Wi-Fi shows up as the three `(tunnel)` checks red and the three pings green,
  which is a different picture from a server being down. `./seed-kuma.sh` reads
  `hosts.env` for those IPs and refuses to run without it.

`source hetzner/env` also exports two flags `just atlassian-live` reads:

- `KNOBAS_TESTENV_HETZNER=1`: with a server per product the recipe starts and
  seeds Jira and Confluence **at once** rather than one after the other (the
  sequence exists for the notebook's 8 GB VM, and the local branch keeps it).
  The two seeds write `seed-state.json` concurrently, which is why
  `seed-atlassian.sh`'s `record` takes a `mkdir` lock.
- `KNOBAS_ATLASSIAN_KEEP=1`: **the pair stays up between runs** and the recipe
  does not `down -v` at the end. The next run finds both RUNNING, both seeds
  say "already set up", the content seed skips everything it finds, and the
  run is the four suites. The licence renewer below is what makes that legal.
  A wipe is explicit, from `testenv/` under the shim:
  `docker compose --profile real-atlassian down -v jira jira-db confluence confluence-db`;
  the next run then walks both wizards again (about four minutes on cpx22).

The recipe's refusal to run while `knobas-teamcity` is up looks at the *local*
engine, where TeamCity no longer is, so it does not fire.

## The licence renewer

The Atlassian keys are 10-user timebomb licences that die three hours after
they take effect. A pair that lives between runs therefore needs renewing, and
`renew-licence.sh` does that on each Atlassian server from a systemd timer
(`install-renewer.sh` puts it there): every 150 minutes and three minutes
after boot. What it does was measured against the real containers on
2026-09-06:

- **Jira's expiry is the JVM start plus three hours.** Restarted at 08:22:23Z,
  the applications REST reported `expiryDate` 11:22:33Z; re-applying the
  identical key through that REST (200 with `{"licenseKey": ...}`) moved the
  date not at all. So for Jira the renewer is `docker restart knobas-jira`
  (RUNNING again after 66 to 68 s on cpx22) and a read-back of the new expiry.
- **Confluence shows its expiry to the day only**, so which rule it follows is
  unmeasured. It gets both: the restart (RUNNING after 80 to 86 s), then the
  key fetched afresh and re-applied through `/admin/doupdatelicense.action`,
  which answers a 302 to `#updateSuccessful`. A failed fetch (Atlassian's page
  reshaped) leaves the restart done and is logged, not fatal.
- **Websudo is off on both** (`jira.override.yml`, `confluence.override.yml`):
  Jira's licence REST answers `upm.applications.websudo.error` to a plain
  basic-auth call and Confluence's licence form sits behind the same
  re-confirm-your-password step; a cookie login plus the websudo form would
  break the moment a live suite's wrong-password test trips CAPTCHA on the
  same account. These products are reachable only through the tunnel and hold
  developer credentials.
- **Not under a live run.** The recipe touches `/opt/knobas/run-in-progress` on
  each server for its duration and removes it from its EXIT trap; the renewer
  waits for the marker (up to 20 minutes, or a 30-minute-stale one) before it
  restarts anything. The recipe, in turn, runs the renewer first when its
  status file is older than 120 minutes, so no run starts with under 30
  minutes of licence.
- **Status**: `/opt/knobas/renew-status` on the server, one line, printed by
  `./tunnel status`; `journalctl -u knobas-renew-licence` for the history.

`fetch-timebomb-keys.sh` exited 1 on its no-`--write` path (a `[ ] && ...` as
the loop's last command); nothing had noticed while every caller was an
`eval "$(...)"`. Fixed the day the renewer, under `pipefail`, saw an empty key.

## Day to day

```sh
cd testenv
source hetzner/env          # once per shell: the routing shim
./hetzner/tunnel up         # idempotent; again after sleep or a Wi-Fi change
./hetzner/tunnel status

just teamcity-live-seeded   # from the repo root, as before
just atlassian-live         # the pair is already up and seeded: seeds and content
                            # seed skip through, four suites run, nothing torn down

docker compose --profile real-teamcity up -d teamcity teamcity-agent  # -> knobas-teamcity
docker --context knobas-jira ps                                        # plain, no shim needed
ssh knobas-jira                                                        # root on the server
```

TeamCity is left running and seeded between sessions, as it was locally; its
volumes live on `knobas-teamcity`. The Atlassian pair is left running too,
licence renewed on the servers' own timers.

Verified 2026-09-06, all green, each a full `just atlassian-live` with four
suites:

| shape | servers | to seeded | total |
|---|---|---|---|
| notebook, one product at a time (repo history) | -- | 265 to 317 s | 292 to 480 s |
| one at a time, torn down | cx23 | 883 s | 986 s |
| parallel start, torn down | cx33 | 754 s | 851 s |
| **kept pair, first run after set-up** (content seed from empty) | cpx22 | 174 s | 226 s |
| **kept pair, every run after** | cpx22 | 10 s | **60 s** |

The kept pair's first run includes 80 s the seed spent waiting for a
Confluence the recipe had recreated (the drift `--no-recreate` now prevents).
The pair itself was set up from empty in 243 s on cpx22. The four suites
themselves ran in 46 s there against 76 s on the Intel types, over the same
tunnel. `./seed --teamcity` from empty volumes and `just teamcity-live-seeded`
were verified on the cx23 the same day.

## Provisioning

`./provision.sh` is idempotent and does all of: a dedicated SSH key
`~/.ssh/knobas-hetzner` (the keys already in `~/.ssh` are FIDO hardware keys,
which want a touch for every one of the many connections a compose call
makes), the hcloud ssh-key, the firewall, the three servers with
`cloud-init.yml` (Docker engine from get.docker.com, 4 GB swap,
`GatewayPorts clientspecified` for the reverse forward), a `Host knobas-*`
block in `~/.ssh/config` between marker comments, `hosts.env` with the
addresses (gitignored), and the three docker contexts. It reads the token as
`HETZNER_API_TOKEN` from the repo-root `.env` and exports it as `HCLOUD_TOKEN`,
which `hcloud` prefers over its active context, so the `hcloud context` for
another project on this machine is never the one it writes to.

`./provision.sh --destroy` deletes the three servers and their data; the key,
the firewall and the ssh config block stay. Re-running `./provision.sh` after
that recreates them empty: `./seed --teamcity` again.

## What is not solved

- The tunnel is three plain `ssh -N` processes with keepalives. They die with
  the network; `./tunnel up` puts them back. There is no supervisor.
- `../reset` (`docker compose --profile '*' down -v`) names no service, so the
  shim leaves it local: it resets Gitea, Kuma and mockd, not the servers. To
  wipe a server's product: `docker compose --profile real-teamcity down -v
  teamcity teamcity-agent` under the shim, or `./provision.sh --destroy`.
- The tunnels do not survive a server resize or reboot; `./tunnel up` again.
- A server that lives for weeks accumulates whatever a suite failed to put
  back. The wipe above is the reset; the seeds and the content seed rebuild it.
- **One named instance of that, on Jira:** an inactive workflow named
  `ZZP: Process Management Workflow`, left on 2026-09-08 by #522's measurement
  of which project template gives a narrowing workflow. The throwaway project
  it belonged to was deleted and Jira reclaimed its statuses, but the workflow
  itself has no REST route out of this version: `GET /rest/api/2/workflow`
  answers no `entityId` to address a `DELETE` with, `/rest/api/2/workflow/search`
  is a `404`, and `DELETE` by name is a `404` or a `405`. It is inert — in no
  scheme, contributing no status, on no endpoint any seed or suite calls — and
  it goes with the next `down -v` of the Jira service. An admin may delete it
  from Jira's own workflow screen at any time and strike this bullet; no agent
  is asked to click, because forcing a REST-less step through a UI is what
  #522's own instruction refused. **The rule it leaves behind: walk a template
  on the project the seed keeps, not on a throwaway** — the measurement would
  have cost nothing to make on `NARROW` itself.
- The renewer depends on Atlassian's public licence page for Confluence's
  re-apply. If Confluence turns out to follow Jira's start-relative rule, that
  dependency is redundant; nobody has measured it.
- Two people cannot share these servers any more than they could share the
  laptop's containers ("One environment, one owner at a time" in
  `../README.md`); the servers are one account's.
