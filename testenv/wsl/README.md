# Live products on a remote machine (WSL)

The live products can run around the clock on a Windows desktop's WSL 2 VM, the WSL box, in place of this Mac's Docker. This page covers how a Mac uses the box, what keeps the box from freezing and its licences from expiring, what to do when it stops answering, and how to set one up. Read it when `testenv/live.env` names a remote target, or when you want it to.

Commands on this page run in `testenv/` on the Mac unless the text names another machine. An agent's shell starts at the repository root, where a hook refuses `cd testenv && ...`, so an agent prefixes each command with `env -C testenv`, as in `env -C testenv ./live remote-check`.

## What the box is and why the products live there

Björn ruled on 2026-11-01 (#2366) that the live products may run 24/7 on a WSL box. The ruling is [docs/decisions/2026-11-01-live-products-on-a-wsl-box.md](../../docs/decisions/2026-11-01-live-products-on-a-wsl-box.md). The notebook's 8 GB Docker VM holds one JVM product at a time, so every JVM run waited for another product's start and removal. The box has the memory for all of them, so the whole live suite stays open.

It is a switch, not a replacement. One file, `testenv/live.env`, says where the products run. With no file, everything in [testenv/docs/live-products.md](../docs/live-products.md) holds unchanged.

The box in use, as measured on 2026-11-01:

| | |
| --- | --- |
| Machine | A Windows desktop with 16 GB of physical memory |
| VM | Ubuntu 26.04 in WSL 2, 12 cores, 12 GB |
| Engine | docker.io 29.1.3 |
| Reached | Over Tailscale, as the ssh host `knobas-wsl` on port 2222 |
| Memory budget | 10425 MB |

The box runs Docker and sshd. It holds no checkout. The Mac drives compose and the seeds, so the box always runs the image pins and the seeds of the worktree under test. A checkout on the box would drift from that tree, which is why there is none.

What changes with a remote target:

| | Local target | Remote target |
| --- | --- | --- |
| Where the containers run | This Mac's Docker | The box's engine, over ssh |
| A product at the end of a run | Put back as the run found it | Left up |
| One-JVM refusal | On | Off. A memory budget decides what runs at once |
| JVM lock | One for the machine | One per product |
| Memory caps | None | `docker-compose.remote.yml` caps every container |
| Licence renewal between runs | None | A launchd agent on the Mac, every 15 minutes |
| Uptime Kuma, the canary, the stall | On the Mac | On the box |

With a remote target the Mac runs no container at all.

## Use the box day to day

### Point the checkouts at the box, or back at this Mac

`testenv/live.env` is the switch. It is gitignored, `live.env.example` is its template, and `./wsl/connect.sh` writes it:

```sh
KNOBAS_LIVE_TARGET=remote
KNOBAS_LIVE_SSH=knobas-wsl
```

- The file lives in the main checkout. A worktree reads its own `testenv/live.env` if it has one, and the main checkout's otherwise.
- It holds `KNOBAS_LIVE_*=value` lines and comments. `live-lib.sh` reads it and never sources it, and refuses anything else in it.
- A variable set in the environment wins over the file, per variable. `KNOBAS_LIVE_TARGET=local just jira-live` runs one suite on this Mac while the file names the box. The unit tests pin `local` whatever the file says.

| Variable | Default | Means |
| --- | --- | --- |
| `KNOBAS_LIVE_TARGET` | `local` | `local` is this Mac's Docker, on demand. `remote` is the machine `KNOBAS_LIVE_SSH` names. Any other value is refused. |
| `KNOBAS_LIVE_SSH` | none | The remote machine's ssh host, an alias in `~/.ssh/config`. Required for `remote`. |
| `KNOBAS_LIVE_TUNNEL_PORTS` | `3000 3001 3002 3443 3445 8080 8090 8095 8111` | The ports forwarded from 127.0.0.1 on the Mac to 127.0.0.1 on the box. |
| `KNOBAS_LIVE_ONE_JVM` | `1` local, `0` remote | `0` turns the one-JVM refusal off. |

To go back to this Mac:

```sh
./wsl/connect.sh --local
```

It removes the main checkout's `live.env`, closes the tunnel and removes the refresh agent. It leaves the ssh key, the `Host knobas-wsl` block in `~/.ssh/config` and the `wsl` docker context. `./wsl/connect.sh <address> [port] [user]` points the checkouts at the box again.

Before you switch to the box, stop any product still up locally with `just live-down <product>`. Its port would keep the tunnel from opening.

### What a run does on the box

Nothing in the suites changes.

- Docker calls go to the box. `live-lib.sh` exports `DOCKER_HOST=ssh://$KNOBAS_LIVE_SSH`, so every `docker` and `docker compose` call reaches the box's engine. That covers the recipes, the seeds and the one suite that runs `docker exec`, `live_jira_seeded`. Compose still reads `docker-compose.yml` and `images.env` from the worktree.
- One ssh tunnel carries the ports. A single ssh master forwards the product ports from the Mac's loopback to the box's. Any live run that finds it down opens it and leaves it running. The products stay bound to the box's 127.0.0.1, so every URL the seeds, `./seed --env` and the suites use is the one they use locally.
- The seeds run on the Mac. Each worktree's `seed-state.json` is its own, written by the seed that just ran against the product on the box.
- No run stops a product when it ends. Products stay up until the memory budget needs their room. `just live-down <product>` still works, through the same engine.
- A local run refuses while the tunnel is open. The tunnel holds this Mac's product ports, so a local product could not publish them, or its suite would talk to the box through the tunnel. `./live tunnel down` frees the ports.

| Port | Forwards |
| --- | --- |
| 3000 | Gitea |
| 3001 | Uptime Kuma |
| 3002 | Forgejo |
| 3443, 3445 | The https front for Gitea and for Forgejo |
| 8080 | Jira |
| 8090 | Confluence |
| 8095 | The docs-assets server that comes up with Confluence |
| 8111 | TeamCity |

```sh
just live-status            # the target, the tunnel's state, one line per product
./live remote-check         # ssh, the engine, the budget and what fits, then opens the tunnel
./live tunnel status        # exits 1 when the tunnel is down
./live tunnel up
./live tunnel down          # works from either target
just live-down <product>    # stops or removes a product on the box
```

### Locks on a remote target

The locks stay on the Mac, in `$TMPDIR`, because the worktrees that take turns are all there. The lock mechanics are in [How runs take turns](../docs/live-products.md#how-runs-take-turns). On a remote target these points differ (Björn, 2026-11-01):

- Each JVM product has its own lock: `knobas-live-jira.lock`, `knobas-live-confluence.lock` and `knobas-live-teamcity.lock`. A Jira run, a Confluence run and a TeamCity run from three worktrees go at once.
- Two runs on one product take turns, with the same waiting line, 45-minute refusal and dead-holder takeover as the local JVM lock.
- `just atlassian-live` takes no lock of its own, because each half takes its product's.
- A `live-up` keeper keeps its product's lock.
- The Gitea lock is unchanged. The local target keeps the one JVM lock.

### How long runs take on the box

Measured on the box, 2026-11-01:

| Recipe | Alone | All three JVM recipes at once, from three worktrees |
| --- | --- | --- |
| `just gitea-live` | 265 s | not measured |
| `just jira-live` | 559 s, of which 299 s seeding from empty | 179 s |
| `just confluence-live` | 242 s, of which 147 s seeding | 259 s |
| `just teamcity-live-seeded` | 559 s | 332 s |

Started together, the three JVM recipes finished in about 5.5 minutes. In turn they took about 23.

### Files the box setup keeps on the Mac

| Path | Holds | Moved by |
| --- | --- | --- |
| `testenv/live.env` in the main checkout | The switch | |
| `~/.ssh/knobas-wsl`, `~/.ssh/knobas-wsl.pub` | The ed25519 key, no passphrase | |
| `~/.ssh/config` | The `Host knobas-wsl` block, between marker lines | |
| `$TMPDIR/knobas-live-tunnel.sock` | The tunnel's ssh control socket | `KNOBAS_LIVE_TUNNEL_SOCK` |
| `$TMPDIR/knobas-live-<product>.lock` | A JVM product's lock, a symlink that names its holder | `KNOBAS_LIVE_LOCK_DIR` |
| `$TMPDIR/knobas-live-gitea.lock` | The Gitea lock | `KNOBAS_LIVE_GITEA_LOCK` |
| `$TMPDIR/knobas-live-mem.lock` | The memory section, a directory with its holder's pid inside | `KNOBAS_LIVE_MEM_LOCK` |
| `~/Library/LaunchAgents/com.knobas.live-refresh.plist` | The refresh agent | |
| `~/Library/Logs/knobas-live-refresh.log` | The refresh agent's log | |

## How the box stays within its memory

The box has 16 GB. A VM pushed into swap stops answering ssh, which is a frozen box and not a failed run. Three layers prevent it (Björn, 2026-11-01).

### Windows keeps its share

`setup-windows.ps1` gives WSL the physical memory less 6 GB, which is 10 of 16, with 4 GB of swap and `autoMemoryReclaim=gradual`. WSL cannot take more than its cap, whatever runs inside. The box in use has 12 GB. The script has not run there, because its WSL already had 12 GB.

### Every container has a hard cap

`docker-compose.remote.yml` sets a `mem_limit` per container and pins each JVM heap under it. `live-lib.sh` layers it over `docker-compose.yml` through `COMPOSE_FILE` for every compose call a remote run makes. It never applies on the local target. `check-ports.sh` asserts that the overlay changes no port.

| Container | Measured use | Cap | Heap | Variable for the cap |
| --- | --- | --- | --- | --- |
| Jira | 3.48 GB | 4608 MB | 2048 MB | `KNOBAS_LIVE_JIRA_MB` |
| Confluence | 2.27 GB | 3072 MB | 1024 MB | `KNOBAS_LIVE_CONFLUENCE_MB` |
| TeamCity server | 1.22 GB | 2048 MB | 1536 MB | `KNOBAS_LIVE_TEAMCITY_MB` |
| TeamCity agent | 0.40 GB | 1024 MB | | `KNOBAS_LIVE_TEAMCITY_AGENT_MB` |
| Each PostgreSQL | under 0.2 GB | 512 MB | | `KNOBAS_LIVE_DB_MB` |
| Gitea, Forgejo | | 512 MB each | | `KNOBAS_LIVE_GITEA_MB` |
| Uptime Kuma | | 768 MB | | `KNOBAS_LIVE_KUMA_MB` |
| `docker-proxy`, `https-front`, `docs-assets` | | 128 MB each | | fixed |

The overlay sets the Jira and Confluence heaps through `JVM_MAXIMUM_MEMORY` and TeamCity's through `TEAMCITY_SERVER_MEM_OPTS`.

The measured figures are from the box on 2026-11-01, with all three JVM products up and a Jira suite running. The VM peaked at 8.5 of 12 GB used, with 3.2 GB available. Jira's and Confluence's caps sit about a third above measured use, and the others higher, so a JVM that grows takes out its own container and not the VM. Do not tighten Jira's cap. At 3.5 GB Jira sat at its ceiling and went into swap.

### A memory budget decides what runs at once

Before a product starts, `live-lib.sh` adds what this product will use to what each product already up uses. These are measured figures with headroom, not the caps:

| Product | Counts as | Variable |
| --- | --- | --- |
| Jira with its database | 4096 MB | `KNOBAS_LIVE_USE_JIRA_MB` |
| Confluence with its database | 2560 MB | `KNOBAS_LIVE_USE_CONFLUENCE_MB` |
| TeamCity with its agent | 2048 MB | `KNOBAS_LIVE_USE_TEAMCITY_MB` |
| Gitea, Forgejo | 512 MB each | `KNOBAS_LIVE_USE_GITEA_MB` |
| Uptime Kuma, when it runs | 512 MB | `KNOBAS_LIVE_USE_KUMA_MB` |

The budget is the engine's memory less `KNOBAS_LIVE_MEM_RESERVE_MB` (1536), or `KNOBAS_LIVE_MEM_BUDGET_MB` outright.

While the sum is over the budget, the start stops other JVM products until the new one fits:

- The one started longest ago goes first.
- A product is stopped only under its own lock, taken for the stop without waiting. A product whose lock another run or a `live-up` keeper holds is in use and is skipped.
- Products are stopped, not removed. TeamCity keeps its runs, and Jira and Confluence keep their seed until their licence is due.
- Gitea, Forgejo and Uptime Kuma are never stopped to make room.
- A product that still does not fit is refused in one line that names the numbers.

```text
live: <product> needs <n> MB; <sum> MB is over the <budget> MB budget on <ssh host> -- stopping <other> (started longest ago; its data stays)
live: <product>: <other> would make room but another run holds it; not stopping it
live: <product> needs <n> MB and <used> MB of the <budget> MB budget on <ssh host> is held by what may not be stopped; not starting it (KNOBAS_LIVE_MEM_BUDGET_MB, KNOBAS_LIVE_USE_*_MB)
```

Deciding what fits and starting the containers happens in one short memory section, `$TMPDIR/knobas-live-mem.lock`. Seeding happens after it. Without the section, two runs starting two products at once would each count the other as down and both find room. A run waits up to 900 s for it (`KNOBAS_LIVE_MEM_LOCK_WAIT`).

On the box's 12 GB WSL the budget is 10425 MB, and all three JVM products fit beside Gitea at 9216 MB. With a 10 GB WSL any two fit and a third stops one. `./live remote-check` prints the budget and what fits.

Memory problems:

- **`remote-check: WARNING Jira alone does not fit the budget`.** WSL has too little memory. Raise `memory=` in `%UserProfile%\.wslconfig` on the box and restart WSL as in [setup step 1](#1-windows-memory-network-and-keeping-wsl-running).
- **`live: could not read the memory of the engine on <ssh host>`.** `docker info` gave no memory total. Set `KNOBAS_LIVE_MEM_BUDGET_MB`.
- **`live: the memory section (...) is still held by pid <pid>; giving up`.** Another run held the memory section for the whole wait. Its holder is alive, since the next run takes a dead holder's section over. The line names the pid to look at.
- **`setup-wsl: WARNING under 8 GB`.** The script warns that only one JVM product at a time will fit. Raise `memory=` as above.

## How Jira and Confluence stay licensed

A timebomb licence lasts 3 hours. The key's expiry is a duration, `LicenseExpiryDate=P3H` in the decoded key, which the product resolves each time its JVM starts. So a restart renews the licence and keeps the data (#2373). Measured on the box on 2026-11-01, a `docker restart knobas-confluence` gave three hours from the restart, and the API was back in under a minute. Posting the same key again through Jira's licence API answers 200 and leaves the expiry where it was, and Jira refuses to delete its only licence, so the restart is the renewal.

### At every start, on both targets

Before a run or `live-up` re-seeds Jira or Confluence:

1. It reads the expiry from the product. Jira answers at `/rest/plugins/applications/1.0/installed/jira-software/license`, Confluence at `/rest/license/1.0/license/details`.
2. With less than `KNOBAS_LIVE_LICENCE_MARGIN_S` (3600) left, it restarts the product.
3. If the restart does not move the expiry, it removes the product with its volumes and sets it up from empty.
4. Otherwise it re-seeds, which writes nothing to a seeded instance.

A product that is down renews when it starts. The caller holds the product's lock, so no other run is inside it during the restart.

Two more rules apply to every start.

The REST API must answer before the content seed. `/status` says RUNNING while the plugins behind the API still warm up. A Jira restarted from a stopped container answered `/rest/api/2/myself` with 503 for a while, and the seed that hit it was retried from empty for nothing. So the start waits until the admin gets a 200 from `/rest/api/2/myself` on Jira or `/rest/api/space?limit=1` on Confluence. The wait is `KNOBAS_LIVE_API_WAIT_S`, 300 by default and 600 after a renewal restart.

A seed that fails on an instance that was already there is retried once, from empty. That covers data a killed run left half-changed, an account Jira locked after wrong passwords ("Never send this Jira a wrong password" in [testenv/docs/real-products.md](../docs/real-products.md)), and a licence that ran out earlier than its age said. A seed that fails from empty is reported as the failure it is.

Jira and Confluence log in with the admin password, not a token, so a fresh instance needs no new credential. `./seed --env` checks the Gitea token and the seed mints it again (#1377).

### Between runs, on a remote target

`./live refresh` restarts each of Jira and Confluence that is up and has less than the margin left. It restarts a product only under that product's lock, taken without waiting. A product whose lock a suite or a `live-up` keeper holds, from any worktree, is left alone with a line naming the holder and tried again at the next tick. No run has its product restarted under it. On the local target `./live refresh` refuses.

A launchd agent on the Mac runs it every 15 minutes and when it loads. It runs on the Mac because the per-product locks are in the Mac's `$TMPDIR`. The box cannot see them, so a timer there could not keep the lock rule.

```sh
./wsl/refresh-agent.sh install     # write the plist and load it
./wsl/refresh-agent.sh uninstall   # unload it and remove the plist
./wsl/refresh-agent.sh status      # loaded or not, and the log's last 12 lines
KNOBAS_LIVE_LICENCE_MARGIN_S=10500 ./live refresh   # a forced tick
```

- `install` writes `~/Library/LaunchAgents/com.knobas.live-refresh.plist` for the main checkout's `testenv/live refresh`, with the installing shell's `PATH`, `TMPDIR` and `HOME`. `connect.sh` installs it and `connect.sh --local` removes it.
- The log is `~/Library/Logs/knobas-live-refresh.log`.
- `KNOBAS_LIVE_REFRESH_EVERY_S` (900) sets the interval at install. `KNOBAS_LIVE_REFRESH_TREE` makes the agent run another checkout's `testenv/live`.
- Fifteen minutes and an hour's margin give four tries before expiry.

Licence problems:

- **A product expires while the Mac sleeps.** The agent only runs on a Mac that is awake. Keep the Mac from sleeping (`pmset -g` shows `sleep 0`). On a Mac asleep no suite runs either, and the next run's own check renews what is due.
- **A run meets an expired product.** The agent never restarts a product under a run, so a run that holds the lock past the expiry meets an expired product. Run it again. Its start renews the licence first.
- **`live: <product>: the restart did not renew its licence`.** At a start, the product is then removed and set up from empty. From `./live refresh` the tick exits non-zero, and the next tick or the next start tries again.
- **`live: <product>: up but does not tell its licence (still starting?); next tick`.** The product did not answer the licence request. The agent tries again at the next tick.

## Uptime Kuma, the canary and the estate file on the box

Uptime Kuma follows the target too (Björn, 2026-11-01). `just kuma-live` sources `live-lib.sh`, so Kuma, its Docker API proxy and the seed's node container run on the box's engine, and the tunnel brings port 3001 to the Mac.

The seed's files travel on stdin. `seed-kuma.sh` sends `kuma-seed.mjs` and `monitors.json` to the container as a tar on stdin, because a bind mount would name a path on the engine's machine.

Two host sockets run on the box as well. A Kuma on the box resolves `host.docker.internal` to the box's Docker bridge, 172.17.0.1, and a container cannot reach a socket bound to the box's 127.0.0.1.

| Socket | Script | Port | On a remote target |
| --- | --- | --- | --- |
| The canary | `canary.sh` | 8299 | The script runs itself on the box over ssh, fed on stdin, and binds the bridge address. The bridge is not a LAN interface, so the canary is still off the network. Only `up`, `down` and `status` cross to the box. |
| The stall | `stall.sh` | 8298 | A socket that accepts and never answers, which `live_kuma`'s refresh-waits witness needs. Locally the suite holds its own. On a remote target `just kuma-live` binds it, names it in `KNOBAS_KUMA_STALLED_URL` and releases it when the run ends. |

What else to know:

- The product monitors name compose services. `teamcity`, `jira` and `confluence` watch `http://teamcity:8111/login.html`, `http://jira:8080/status` and `http://confluence:8090/status`, as `gitea` watches its service. They reach the products on the compose network. Through `host.docker.internal` the products, bound to 127.0.0.1, would not answer.
- `notebook` pings the Mac's Tailscale address, `100.92.199.117`. On a freshly seeded Kuma that ping takes long enough to fail that a full sync in the first minute may not see `notebook` yet. A second run does.
- The gates need no engine. `check-ports.sh` only resolves the compose files, and `just shell` uses the local `shellcheck` when its version is the pin.
- `just estate-live` reads the box's engine. `testenv/estate.json` describes the box. The recipe's Docker half runs `docker --context wsl ps`, through the ssh context onto `knobas-wsl` that `connect.sh` creates, and requires every running container there to be in the file. The estate file and the monitors are in [testenv/docs/estate-and-monitors.md](../docs/estate-and-monitors.md).

## When the connection is down

Start with the check. It tests ssh, the engine and the memory budget in that order, then opens the tunnel, and a failure names its step:

```sh
./live remote-check
./wsl/refresh-agent.sh status    # the log shows whether the box answered at the agent's last ticks
```

- **`remote-check: ssh knobas-wsl fails (keys, host, port?)`.** The Mac cannot log in. Check that the box is on and awake, since a sleeping box answers nothing. Check that the distribution runs. The scheduled task `knobas-wsl-keepalive` starts it at boot, and `Start-ScheduledTask knobas-wsl-keepalive` on Windows starts it now. If the box's address changed, run `./wsl/connect.sh <address> [port] [user]` again, which replaces the `Host` block.
- **`connect: cannot log in to knobas-wsl yet`.** The box does not hold the Mac's key. Run the `bash setup-wsl.sh --port <port> --key '<key>'` line that `connect.sh` prints inside WSL, then run `connect.sh` again.
- **`live: the docker engine on knobas-wsl (ssh://knobas-wsl) does not answer`, or `remote-check: no docker engine answers`.** ssh may work while the engine does not. Run `ssh knobas-wsl docker ps`. The ssh user must be in the `docker` group, and dockerd must be running. `setup-wsl.sh` sets both up, so running it again on the box, with the same `--port`, puts them back.
- **`live: could not open the tunnel to knobas-wsl`, then `127.0.0.1:<port> is already bound here`.** Something on the Mac holds a product port, most often a local product still up. Run `docker --context orbstack ps`, stop it, and run again. A worktree whose code predates the remote target has no guard against starting a local product beside the tunnel.
- **`live: the open tunnel could not add 127.0.0.1:<port>`.** The tunnel was opened before that port joined the list and cannot add it now. Run `./live tunnel down`, then run again.
- **`live: an ssh tunnel to a remote target holds this machine's product ports`.** A local-target run found the tunnel open. Run `./live tunnel down` to free the ports, or run with `KNOBAS_LIVE_TARGET=remote`.
- **`live: KNOBAS_LIVE_TARGET=remote names no machine`.** `live.env` sets the target without `KNOBAS_LIVE_SSH`. Add it, or run `connect.sh` again.
- **`live: <file>: '<line>' is not a KNOBAS_LIVE_*=value line`.** `live.env` holds a line that is neither a comment nor a `KNOBAS_LIVE_*` assignment. Remove it.
- **A product is up on the box but its `127.0.0.1` address answers nothing.** The tunnel is down. Live runs open it themselves. `just live-sources` and `just live-demo` do not, and skip a product whose address answers nothing. Run `./live tunnel up`, then run them again.

## Set the box up from scratch

There are three scripts in `testenv/wsl/`, one per machine. All three can be run again.

Before you start:

- Windows 11 22H2 or later with WSL 2.0 or later, which mirrored networking needs.
- An Ubuntu or Debian distribution in WSL.
- The box has no checkout, so bring `setup-windows.ps1` and `setup-wsl.sh` over to it yourself.
- On the Mac, stop any product still up locally with `just live-down <product>`.

### 1. Windows: memory, network and keeping WSL running

In an elevated PowerShell on the box:

```powershell
powershell -ExecutionPolicy Bypass -File setup-windows.ps1 -Distro Ubuntu [-Port 2222] [-MemoryGB 10]
```

It does three things. First, it sets five keys in `%UserProfile%\.wslconfig`. An existing file is backed up and keeps its other keys.

| Key | Value | Why |
| --- | --- | --- |
| `memory` | Physical memory less 6 GB and at least 6 GB, or `-MemoryGB` | Windows keeps enough to stay usable. The script refuses a `-MemoryGB` that leaves Windows under 4 GB. |
| `swap` | `4GB` | Catches a spike. |
| `networkingMode` | `mirrored` | WSL's sshd answers on the Windows box's own address. |
| `vmIdleTimeout` | `-1` | No idle shutdown of the VM. |
| `autoMemoryReclaim` | `gradual` | Hands freed cache back to Windows. |

Second, it adds two inbound firewall rules for the port, `knobas-wsl-ssh` on the Hyper-V firewall and `knobas-wsl-ssh-host` on the host.

Third, it registers the scheduled task `knobas-wsl-keepalive`, which starts the distribution at boot and keeps it open, so dockerd and sshd run with nobody logged in.

Then run `wsl --shutdown` and `Start-ScheduledTask knobas-wsl-keepalive`. In Settings, System, Power, set sleep to "Never".

If mirrored networking is not available, Tailscale inside WSL does the same job. Use its address in steps 2 and 4.

### 2. Mac: the key and the ssh host

```sh
./wsl/connect.sh <address> [port] [user]     # port 2222, user $USER
```

On its first run it:

1. makes `~/.ssh/knobas-wsl`, an ed25519 key with no passphrase. The runs are unattended, and a security key asks for a touch per login.
2. writes a `Host knobas-wsl` block with a ControlMaster to `~/.ssh/config`, because every docker call is an ssh login. A later run replaces the block.
3. fails to log in, prints the `setup-wsl.sh` line for step 3 with the public key in it, and exits 1.

### 3. WSL: the engine and sshd

Inside the distribution, as the user the Mac logs in as and not as root, run the line step 2 printed:

```sh
bash setup-wsl.sh --key '<public key>' [--port 2222]
```

- It turns systemd on in `/etc/wsl.conf`. A first run that had to do so stops there. Run `wsl --shutdown` from Windows, open the distribution again and run the script again.
- It installs `docker.io` and `openssh-server` and puts the user in the `docker` group.
- It runs sshd on the port with keys only, and adds the key to `~/.ssh/authorized_keys`. Port 2222 stays clear of a Windows OpenSSH on 22.
- It warns when the engine is Docker Desktop's, which runs only while Docker Desktop does. For a 24/7 box, turn off Docker Desktop's WSL integration for the distribution and run the script again.
- It warns when the VM has under 8 GB.

### 4. Mac: connect

```sh
./wsl/connect.sh <address> [port] [user]
```

Now the login works, and it:

1. writes `testenv/live.env` in the main checkout.
2. creates the docker context `wsl` (`ssh://knobas-wsl`) if it is missing.
3. runs `./live remote-check`: ssh, the engine, the memory budget, the tunnel.
4. installs the refresh agent.

`setup-windows.ps1` has not run on the box in use. That box's WSL already had 12 GB and systemd.
