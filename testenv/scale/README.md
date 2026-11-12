# Björn's scale

Björn's real sources hold about 5,000 Confluence pages, 20,000 Jira issues, 4,000 TeamCity objects and 10,000 Gitea items. The seed holds a few dozen of each. This page covers the two ways to measure knobas at his size: scale data filled into the live products and synced into a knobas database of its own, and the synthetic 22,000-item database that `just responsiveness-probe` builds once and keeps.

`just` recipes run from anywhere in the checkout. Every other command here runs in `testenv/`. An agent's shell starts at the repository root, where a hook refuses `cd testenv && ...`, so an agent prefixes each such command with `env -C testenv`.

## What the scale seed adds

`seed-scale.sh` fills a live product beside the seed, never in it. Each product gets scopes of its own, so a source that syncs the seed alone sees no change.

| Product | Scope it adds | Default `--count` |
|---|---|---|
| `jira` | projects `SCLPAY`, `SCLOPS`, `SCLWEB`, `SCLDATA`, `SCLMOB`, `SCLSEC`, named *Scale test: ...* | 20,000 issues |
| `confluence` | spaces `SCLENG`, `SCLOPS`, `SCLPRD` | 5,000 pages |
| `teamcity` | project tree `Scale`, named *Scale test* | 3,800 runs |
| `gitea`, `forgejo` | organizations `scale-platform`, `scale-commerce`, `scale-data` | 10,000 items |
| `kuma` | monitors named `scale-*` | 200 monitors |

At the default counts:

- **Jira.** Epics, one with about 900 children, and stories, tasks, bugs and sub-tasks, in To Do, In Progress, In Review or Done by age. Priorities, labels, components, monthly fix versions, due dates and estimates. Assignees and reporters are the fixture's five people and `knobas`. About 20,000 comments written as those people, about 4,500 worklogs and about 7,000 links (Blocks, Relates, Duplicate, Cloners), some across projects. A Scrum board per project with 55 two-week sprints: 52 closed with their done issues, one active, two future.
- **Confluence.** Trees up to nine deep, yearly meeting-note folders with hundreds of pages each, incident reports, release notes, ADRs and an API reference. Storage bodies from 150 bytes to 90 KB, median about 3 KB, with headings, tables, code, panels, status lozenges, expands, task lists, page links, mentions and Jira macros. Labels, about 4,000 comments by six people and about 360 attachments. The fill creates the fixture's five people in Confluence when they are missing, password `tidewater-dev`.
- **TeamCity.** 40 projects in 8 groups of 3 to 4, and 85 pipelines, which TeamCity calls build configurations. About 78 % of the runs are green. The rest fail by exit code or by failed tests, are cancelled, or are killed for memory. About 150,000 test results, and artifacts on about 30 % of the runs. Each leaf project builds a scale Gitea repository on `main` and its branches.
- **Gitea and Forgejo.** 58 repositories, about 18,700 commits by the fixture's people, about 1,200 branches, about 1,700 pull requests of which two thirds are merged, about 2,600 issues with labels, milestones and assignees, about 4,000 comments and about 1,100 reviews. knobas mirrors about 11,000 items of it at the default budgets.
- **Kuma.** 200 monitors in 12 groups. Each checks something on the compose network: Gitea over http, keyword and tcp, or a ping of the loopback. About one in twenty is down. They stay until the Kuma volume is reset.

## Fill a product

Before you start:

- Claim the environment ([`../README.md`](../README.md#one-environment-one-owner-at-a-time), *One environment, one owner at a time*). A fill writes to a shared product for minutes or hours. The Confluence fill restarts `knobas-confluence`, and the TeamCity fill stops and starts `knobas-teamcity`.
- Bring the product up and seeded with `just live-up <product>`. The scale seed reads its address and credentials from `seed-state.json`, or `seed-state-forgejo.json` for Forgejo, as the live suites do.
- Have `python3` 3.9 or newer on the PATH, and `git` for a Gitea or Forgejo fill. `docker` must reach the live target. On a remote target those calls go to the WSL box like every other.
- Fill Gitea at its default count before TeamCity. The TeamCity fill points each leaf project at a scale Gitea repository and reads that repository's branches from `http://127.0.0.1:3000`.

```sh
just live-up jira                  # the product up and seeded, as for a suite
just seed-scale jira               # = ./seed-scale.sh jira [--count N] [--workers W]
just seed-scale jira --plan-only   # the plan's numbers, no server contacted
```

| Option | Does |
|---|---|
| `--count N` | Sets the size. The defaults are Björn's. For `kuma` it must be the first option. |
| `--workers W` | Sets the parallel requests, 12 by default. Not for `kuma`. |
| `--agents N` | TeamCity only: agents for the fill, 3 by default, which is all the licence allows. |
| `--plan-only` | Prints what the plan holds and contacts no server. It still needs the state file. Not for `kuma`: `./seed-scale.sh kuma --plan-only` adds the monitors. |

When the seed refuses a product, its message names the fix.

### What a fill does to the product

The plan is fixed. `scale/*.py` draw every item from a generator keyed on a fixed seed, the product and the item's index, and date everything over the two years ending 2026-11-01, not over the day of the fill. Filling again makes the same data.

Each phase is idempotent, and the fill records each finished phase on the server. On a filled product it reads those markers and adds nothing: Jira takes 3 s, Confluence 1 s, Gitea and Forgejo seconds per repository.

| Product | Where a finished phase is recorded |
|---|---|
| Jira | project property `knobas-scale-seed` |
| Confluence | space property `knobas-scale-seed` |
| TeamCity | parameters `knobas.scale.seed.builds` and `knobas.scale.seed.dates` on the `Scale` project |
| Gitea, Forgejo | the repository's Actions variable `KNOBAS_SCALE_SEED` |
| Kuma | nowhere. The fill finds its monitors by name. |

The products' APIs refuse dates, so the fill writes them into each product's own database. The header of each module in `scale/` says how.

| Product | Dates are written into | What that does to the product |
|---|---|---|
| Jira | `knobas-jira-db`, with `docker exec ... psql`: created, updated and resolved, and the dates of comments, change history, worklogs and closed sprints | A background reindex follows, so JQL sees them. |
| Confluence | `knobas-confluence-db`: the created and modified times of pages, comments and attachments | Confluence restarts so its caches read them, which also renews the licence. The index is then rebuilt over `POST /rest/prototype/1/index/reindex`, so CQL's `lastmodified`, which knobas' incremental sync walks by, agrees with `version.when`. |
| Gitea, Forgejo | the forge's SQLite, with `docker exec ... sqlite3`: created, closed and merged times, the merger, and the dates of comments and reviews | Nothing restarts. Commit dates are real, because the fill builds history locally with `git fast-import`. |
| TeamCity | its internal HSQLDB (`HISTORY`, `LIGHT_HISTORY`, `BUILD_STATE`), with `scale/Sql.java` in the server's own image | The server is stopped for the rewrite and started again. Each run keeps its own duration. |

### TeamCity's limits

- The free Professional licence stops at 100 pipelines (#1560 met it). The fixture, the chain and the live suite need some, so the scale tree has 85, not the 400 Björn's real server has. The fill refuses to start when it would pass 100. Stray pipelines outside `Scale` count too, and `just teamcity-live-seeded`'s leftover clearing removes them.
- The licence allows three agents. The fill starts `knobas-scale-agent-2` and `knobas-scale-agent-3` beside `knobas-teamcity-agent`, and removes them and their agent records when it ends.
- A run costs about ten seconds of agent time whatever it does, so 3,800 runs take about three and a half hours.

## Resume a stopped fill

Run the same `just seed-scale <product>` again with the product up.

- Jira, Confluence and the forges skip each finished phase and take an unfinished one up by reading what is there. That also covers a fill that died on a wait.
- The TeamCity fill counts the runs each pipeline already has, queues the rest, then rewrites the dates.
- A TeamCity fill that stops normally removes its two extra agents with their volumes and agent records. One that is killed hard leaves them running, and the next fill keeps them. While they run, `just estate-live`'s Docker half sees containers that `estate.json` does not hold.

The 2026-11-10 fill of the WSL box stopped with TeamCity at 2,613 runs, undated, and Forgejo at about 40 of its 58 repositories.

## Run the live suites beside the scale data

Since 2026-11-10 the live suites run beside it. Every leftover clearing and every exact set a suite asserts covers only what the seed and the suite own, and every source a suite builds has the same scope unless a test names its own. A suite neither touches nor counts the scale data. A new live test must keep to this.

| Suite | How it stays in its own scope |
|---|---|
| `live_jira_seeded`, `atlassian_live` | They search `project in (PAY, OPS, NARROW)`, which is `jira.projects` plus the narrowing project from `seed-state.json`, never the whole instance. |
| `live_teamcity_seeded` | It lists runs with `affectedProject:` for each seed project (`Ledger`, `Payout`, `SeededChain`). It deletes only the projects the suite makes: those named `Knobas...` or nested in a seed project. |
| `live_kuma` | It counts monitor types over the seeded six. `./seed-kuma.sh` leaves every `scale-...` monitor alone. |
| Gitea, Forgejo, Confluence | Their suites keep to `tidewater` and to their own spaces and pages. `atlassian_live`'s wait for a deleted page to leave the CQL index asks the page's space only, because a 200-row search of the whole instance never reaches it past the scale spaces. |

## Sync the filled products into knobas

`scale-sync.sh` runs the debug app, the binary `just demo` runs, on a profile of its own and lets the app's own scheduler sync the live products.

```sh
./scale-sync.sh <root> [<source id>...] [--wait S]
just scale-sync <root> [<source id>...]
```

Before you start, quit any demo knobas on this profile. `~/.theseus/postgresql/18.6.0/bin/psql` must exist. Run the app once and it installs it.

- `<root>` is a scratch `HOME` for every process the script starts. The demo profile lands in `<root>/Library/Application Support/dev.knobas.desktop/demo`, and Björn's own profile is never touched. The app's output goes to `<root>/app.log`.
- The script first runs `live-sources.sh add --scale`. `--scale` widens each source's scope to the scale projects, spaces and organizations the product holds. `KNOBAS_LIVE_SCALE`, a space-separated list of `gitea`, `forgejo`, `jira` and `confluence`, narrows which sources widen.
- It then waits for a finished sync of each source id you name, or of every enabled source when you name none, for up to `--wait` seconds, 7200 by default. The ids are `live-gitea`, `live-forgejo`, `live-jira`, `live-confluence`, `live-teamcity` and `live-kuma`.
- It prints each source's first sync (outcome, seconds, rows upserted), the mirror's rows per source and kind and the database's size. Then it quits the app and stops its database server, so the directory can be copied.

Run it once per JVM product while that product is up. The sources are updated in place.

## What a fill and a sync measured

Measured 2026-11-10. The products ran on the WSL box. The fills and the app ran on the Mac through the tunnel.

| Product | Fill | First sync into knobas | Mirror rows |
|---|---|---|---|
| Jira | about 25 min: bulk create at 20/s over six projects side by side, transitions at about 150/s, comments at about 100/s, then about 15 min of reindex | 110 s | 20,007 tickets, 9 views |
| Confluence | 317 s, restart and reindex included | 74 s | 5,032 pages |
| Gitea | about 50 min at 8 repositories at a time, because an issue or pull request is one request after another inside a repository | 242 s at the adapter's default 10 requests/s | 11,486 |
| TeamCity | 2,613 of 3,800 runs in about 3 h on three agents, about 18 runs/min, stopped there for a deadline | 26 s | 2,711 |
| Kuma | 30 s | 0.2 s | 218 |

The knobas database then held 693 MB, 1.4 GB on disk. The one write that crawled was the mirror's upsert: `sync.item`'s 500-row statement passed the 1 s slow-statement threshold 58 times, up to 2.8 s and 78 s in all, most of it inside Jira's first sync. Kuma's scheduled sync rewrote all 218 monitors every five minutes with nothing changed.

## The saved scale database

The database of that sync is kept as a whole scratch `HOME`, outside the repository, with a README of its own. Work on a clone, never on the original.

```sh
HOME=<clone> target/debug/knobas-app --demo   # from the repository root
```

Never start the binary without both `--demo` and `HOME`. That opens the default profile.

[`../perf/README.md`](../perf/README.md) has the clone command and runs the frame and page-open probes on this database.

## The responsiveness probe's saved database

`just responsiveness-probe` (#753) times every read the shell makes against a seeded mirror of 22,000 items. Building that mirror takes minutes and reading it takes seconds, so the probe keeps the built database (#781).

A snapshot is the whole server root, `data/` and `.pgpass`, under `$TMPDIR/knobas-probe-snapshots/`. Each key has two copies: one never analyzed, with autovacuum off on every table, and one analyzed. The probe clones the copy for its `RP_ANALYZED` state with `cp -cR`, an APFS clone that takes seconds whatever the size. It starts its own postmaster on the clone and leaves the snapshot untouched. It prints one `RP-SNAPSHOT` line: `built` or `restored`, the snapshot's name, its size and its age.

| Variable | Default | Does |
|---|---|---|
| `RP_ITEMS` | 22000 | Scales the sizes. `RP_ITEMS=2500` is the size to iterate on and has a snapshot of its own. An explicit `RP_TICKETS`, `RP_PAGES` or `RP_BUILDS` still wins. |
| `RP_ANALYZED` | 0 | `1` reads the analyzed copy. Anything else reads the never-analyzed one. |
| `RP_FRESH` | 0 | `1` rebuilds both copies, and the after-pass snapshot when `RP_PASS=skip`. |
| `RP_PASS` | `run` | `run` runs and times the suggestion pass and prints `RP-PASS ... ms=...`. `skip` restores a snapshot taken after the pass and prints `RP-PASS skipped (snapshot ...)`, so a reading is never mistaken for a pass measurement (#793). |

### When a snapshot is rebuilt

A snapshot is named `m<migrations>-p<seed>-<never|analyzed>`.

- `<migrations>` hashes every file in `crates/knobas-db/migrations`.
- `<seed>` hashes the seed parameters: the sizes (`RP_TICKETS`, `RP_PAGES`, `RP_BUILDS`, `RP_PRS`), `RP_SYNTHETIC`, the pipelines and monitors (`RP_PIPELINES`, `RP_MONITORS`, `RP_SAMPLES`), and the seeded Gitea's address when it was synced.

Change a migration, a size or `RP_SYNTHETIC` and the probe builds a new snapshot on its own. Use `RP_FRESH=1` in the two cases the name cannot see: the seeded Gitea's content has changed, since the snapshot holds what Gitea held when it was built, or the seed code in `responsiveness_probe.rs` has changed.

The after-pass snapshot is built once when it is missing: the probe restores the base copy, runs the pass and saves. Its name adds a hash of `crates/knobas-core/src/suggest.rs` and the migrations, so a change to the pass builds a new one.

### Disk

The probe first deletes snapshots older than 7 days, snapshots of another migrations hash, after-pass snapshots of an older pass, and unfinished copies older than an hour. The 22,000-item copies are about 0.2 GB each as files. A clone costs little until the probe writes.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `scale-sync: <id>: no finished sync after Ns` | The first sync outlasted `--wait`. | Run it again with a longer `--wait`. |
| `scale-sync: live-sources reported a product it skipped or refused` | A product is down or not seeded. The rest were still added. | `just live-up <product>`, then run `scale-sync` again. |
| The probe stops before cargo starts, or fails with `the fixture's Gitea token is refused` | `./seed --env` refused the recorded Gitea token. | Run `./seed` to mint a new one, then retry. |
