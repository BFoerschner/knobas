# The estate file, the monitors and the canary

`estate.json` is the estate knobas models, written down. `monitors.json` is the list of monitors the seed keeps in Uptime Kuma. The canary is the one thing a live run may knock over. All three describe the real test infrastructure, so they have to stay true.

Read this page before you add a compose service, add or move a monitor, need a monitor to go red on purpose, or want to see what Kuma reports.

`just` recipes run from anywhere in the checkout. Every other command here runs in `testenv/`. An agent's shell starts at the repository root, where a hook refuses `cd testenv && ...`, so an agent prefixes each such command with `env -C testenv`.

## The estate file

[`estate.json`](../estate.json) holds the machines this repository is developed and tested on. From M4 the estate knobas models is this infrastructure and not a fictional one (#427), so nothing Tidewater-shaped belongs in it. It records:

- the root, where environment (`dev`) and owner are set once,
- the notebook, typed `custom` because it runs no container and the type table has no name for a workstation,
- the Windows desktop `DESKTOP-QUUAHUB`, its WSL 2 VM (Ubuntu 26.04, 12 cores, 12 GB, the bridge at `172.17.0.1`) and the Docker engine in it (docker.io 29.1.3, docker context `wsl`),
- every container `docker-compose.yml` runs there: Gitea, Forgejo, the https front, Uptime Kuma and its Docker API proxy, TeamCity and its agent, Jira and Confluence with their databases, and `docs-assets`,
- as routes, the ports the notebook reaches through the ssh tunnel, each with the asset it lands on,
- on each asset, the Uptime Kuma monitors that watch it.

The box's values are the real box's, as of 2026-11-01.

The file is a fixture with readers, not documentation. The Import, `--demo` (`knobas_app::sources::demo`), M4.0's exit checklist (`crates/knobas-app/tests/it/estate_exit.rs`), the `?fake-ipc` fixture (`app/src/lib/shell/dev/fake/estate.ts`) and `just estate-live` all read these bytes.

`crates/knobas-app/tests/it/support/hcloud-estate.json` is not a second estate file. It keeps the three Hetzner servers Björn decommissioned (#634) as the counterpart of the hcloud recording that the importer's shape tests run on.

### What `just check` holds the file to

`crates/knobas-core/tests/it/estate_file.rs` runs in `just check` with no container. It requires:

- unique ids,
- parents and route ends that resolve, and assets that form one tree,
- types from the type table, and no unknown keys,
- every compose service recorded on the WSL box's engine, except the one-shot `kuma-seed`,
- every container carrying the `docker_context` and `container_name` the Docker importer matches it on,
- every monitor name an asset uses to be one `monitors.json` holds,
- the notebook's `ip` to equal the address the `notebook` ping watches.

So a new compose service needs its container in `estate.json` in the same change, and a renamed monitor needs both files changed together.

### What `just estate-live` holds the file to

```sh
ESTATE_LIVE_HCLOUD_SERVERS=0 just estate-live
```

Its Docker half reads `docker --context wsl ps` and requires every running container on that engine to be in the file. [`../README.md`](../README.md#the-live-recipes), *The live recipes*, has what the recipe needs.

- A container recorded and not running is not wrong. TeamCity, Jira and Confluence stay up on the box, but the memory budget may stop one, and the file describes them whether or not they are up.
- A container running and not recorded is red. That is why the file records the Docker API proxy although nothing routes to it, and Forgejo although it runs only once a Forgejo run has started it.
- Read a red run that names a container first as `estate.json` being behind the engine, and only then as a bug in the importer.

## Monitors

[`monitors.json`](../monitors.json) is the estate's monitor list, not fixture content (#441, M4 spec #427). It names the monitors on the real infrastructure this repository runs on. The two monitors in `fixtures/tidewater/work.json` belong to the demo mock source and never reach Kuma.

| Monitor | Type | Checks | Goes red when |
|---|---|---|---|
| `notebook` | ping | `100.92.199.117` | the notebook's Tailscale address stops answering the Kuma on the WSL box |
| `teamcity` | http | `http://teamcity:8111/login.html` | TeamCity is stopped |
| `jira` | http | `http://jira:8080/status` | Jira is stopped |
| `confluence` | http | `http://confluence:8090/status` | Confluence is stopped |
| `gitea` | http | `http://gitea:3000/api/healthz` | the Gitea container is down |
| `canary` | http | `http://host.docker.internal:8299/` | `./canary.sh down` released the port |

The canary's interval is 20 s. The others have 60 s.

### Read a red product monitor

The three product monitors are red whenever their product is stopped.

- On the WSL box the products stay up. A red there is a product down, one the memory budget stopped, or one being set up again for its licence.
- On the local target the products run only while a live recipe needs them (#633). Red is the ordinary state there, not an outage.

They use the unauthenticated endpoint each product answers `200` on. The obvious REST paths answer `401`, which `accepted_statuscodes: ["200-299"]` reads as down.

### How Kuma reaches each target

- **The three products and `gitea`** are reached by their compose service names on the compose network. That works on any engine.
- **`notebook`** is an ICMP ping of `100.92.199.117`, the address the Kuma on the WSL box reaches the notebook at. `estate.json` states the same address as the notebook's `ip`. It is a literal because a ping carries a bare address, and the match is what lets `monitor_url_host`, the suggestion rule that reads a ping's `hostname` against an asset's `ip` (#479), reach a ping on the real estate. If the address moves, change it in the table above, in `monitors.json` and in `estate.json` together.
- **`canary`** is the only monitor that checks the host, through `host.docker.internal`. Docker Desktop and OrbStack define that name themselves. On a plain Linux engine, such as the WSL box's, the `extra_hosts: host.docker.internal:host-gateway` line on the `uptime-kuma` service makes it resolve to the bridge gateway, `172.17.0.1`. A bridge-gateway client cannot reach a socket bound to `127.0.0.1`, so on a remote target `canary.sh` binds the bridge address.

The Kuma container can send ICMP. Measured on the pinned image (2.5.3) under OrbStack on 2026-09-06: it runs as root, `/proc/sys/net/ipv4/ping_group_range` is `0 2147483647`, and `/usr/bin/ping` is present.

A change to a compose service's definition recreates its container on the next `docker compose --env-file images.env up -d`. For Kuma that means seconds of downtime, and any live run reading Kuma across them fails. Its data is in the `kuma-data` volume and survives. Treat a compose edit as a change to the shared environment ([`../README.md`](../README.md#one-environment-one-owner-at-a-time), *One environment, one owner at a time*).

### Change the monitor list

The seed owns the list. Edit `monitors.json`, then run:

```sh
./seed-kuma.sh
```

On each run the seed:

- deletes any monitor Kuma holds that `monitors.json` no longer names,
- deletes a second monitor that shares a name with a listed one, which Kuma's UI allows,
- deletes and adds again a monitor whose type, URL, hostname or interval drifted,
- leaves a matching monitor alone, so a second run changes nothing.

Everything else about a monitor is the instance's business and stays as it is: notifications, tags, a paused state a live run left behind. A monitor that has to survive goes into `monitors.json`, not into Kuma's UI.

A monitor named `scale-...` is not the seed's. `just seed-scale kuma` adds those beside the list ([`../scale/README.md`](../scale/README.md)), the seed never reads them, and `monitors.json` may not use the prefix.

The seed knows two shapes: `ping`, with a `hostname`, and everything else, which it sends as http with a `url`. Another type, such as Kuma's `port`, takes more than a `monitors.json` edit. `kuma-seed.mjs`'s `payload()` and `OWNED`, the fields that count as drift, both need a third case first.

`KUMA_WAIT_S` sets the guard on each of the seed's socket waits, in whole seconds, 120 by default.

### Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| An `add` is refused with `Cannot read properties of undefined (reading 'every')` | Kuma's `server.js` runs `accepted_statuscodes.every(...)` on every monitor with no type check, so a ping sent without the field fails. The error names neither the field nor the monitor. | Send `accepted_statuscodes` for every type, as `kuma-seed.mjs` does. |
| `kuma-seed: FAILED: ... within 120 s (KUMA_WAIT_S)` | Kuma answered slowly, or the machine is loaded. | Run again with a larger `KUMA_WAIT_S`. |

## The canary

`canary.sh` binds and releases a host socket on port 8299 that answers `200` with the word `canary`.

```sh
./canary.sh up       # bind the port, wait until it answers
./canary.sh down     # release it
./canary.sh status   # one line, exit 0 when it is bound
```

`up` and `down` are idempotent.

| Target | Where the script runs | Where it binds |
|---|---|---|
| Local | this machine | `127.0.0.1:8299` |
| Remote (`live.env`) | that machine: the script runs itself there over ssh | the Docker bridge address, port 8299, still off the network |

- The pid is in `~/.knobas-canary-8299.pid`, in `$HOME` because a process that outlives a shell does not belong in the repository directory. `./reset` therefore does not have to learn to kill it.
- `KNOBAS_CANARY_WAIT_S` sets how long `up` waits for the responder, in whole seconds, 120 by default.
- Port 8299 is outside the port map in [`../README.md`](../README.md#ports), *Ports*, on purpose. It is a host port, not a published container port, so `check-ports.sh` knows nothing about it.

### Why only the canary may fall

No live run stops a shared container (M4 spec #427). Everything else Kuma watches here is shared: Gitea serves the live suites, the three products serve the Atlassian and TeamCity runs, and the notebook is the estate. Stopping one of those to make a monitor red turns one stream's live run into another's unexplained failure. The canary's whole blast radius is a port nobody else binds. To make a red on purpose, release the canary.

Two callers knock it over (#450, M4.1's exit), and both go through this script and touch nothing else:

- `just kuma-live` binds the port before its three suites and binds it again from a trap however the run ends, a killed run included.
- `crates/knobas-app/tests/alert_chain_live.rs`, the exit witness, releases it, waits for Kuma to publish `down`, syncs, acks the alert, binds it again and waits for the recovery. It also binds the port again in `Drop`, so a panicking run leaves the estate green too.

### How long a fall takes

A fall and a recovery each land inside one 20 s interval. Measured 2026-09-06 over two cycles, polling `/metrics` every second: `monitor_status` read `0` after 14 s and 17 s, and `1` again after 17 s and 17 s. An earlier pair of cycles saw 5 s. Where in the interval the release falls is what varies, so wait for the state and not for a duration.

Through the suite on 2026-09-07: `down` after 14.9 s, `up` again after 21.2 s, and the whole chain of import, fall, inbox, ack and recovery in 38.7 s.

### The stall, on a remote target

`live_kuma` needs a socket that accepts every connection and never answers. Locally the suite holds one itself. On a remote target `just kuma-live` binds one on that machine with `./stall.sh up` (bridge address, port 8298), names it in `KNOBAS_KUMA_STALLED_URL` and releases it from its traps. [`../wsl/README.md`](../wsl/README.md) covers the remote target.

### Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `up` fails with `already answers and it is not ours` | Another process holds port 8299. | Find it with `lsof -nP -iTCP:8299 -sTCP:LISTEN`. Do not kill it blind. |
| `up` fails with `did not answer ... within 120 s (KNOBAS_CANARY_WAIT_S)` | A loaded machine starts `python3` slowly. | Run again with a larger `KNOBAS_CANARY_WAIT_S`. |
| The `canary` monitor stays red after a run | Nobody bound the port again. | `./canary.sh status` says who holds it. Then `./canary.sh up`. |

## Reading the state back

Verify through `/metrics`, the channel the adapter uses, and not through the UI:

```sh
curl -fsS -u ":$(cat kuma-api-key)" http://127.0.0.1:3001/metrics | grep monitor_status
```

| You see | It means |
|---|---|
| `monitor_status` 1, 0, 2 or 3 | up, down, pending or maintenance, as the metric's own `# HELP` line says |
| no series for a monitor | unknown, not down |
| a response time of `-1` | the check did not answer |
| the literal string `"null"` in a label | a field a monitor of that type has not got |
| `monitor_id` | the monitor's identity |

`crates/knobas-source-kuma/src/model.rs` states what the adapter reads from this document. Uptime Kuma v2 prunes raw heartbeats to about 24 h, so a test of the seeded state asserts presence, never a particular value, and never waits for heartbeat history.

A paused monitor is not in `/metrics` at all. Measured on the pinned image on 2026-09-06 (#442): pausing a monitor over socket.io (`pauseMonitor`) removed every one of its eight series, and `resumeMonitor` put all eight back within one interval. `/metrics` is the only channel an API key opens and the whole of `knobas-source-kuma`'s read path, so there paused and deleted are one observation. knobas tombstones a paused monitor and mirrors it again on resume. Telling the two apart needs the socket.io channel. `crates/knobas-source-kuma/src/cursor.rs` states this where the tombstone is written.

A `401` from this command means this worktree's API key is gone from the instance, after a reset volume or a key deleted by hand. `./seed-kuma.sh` replaces it.
