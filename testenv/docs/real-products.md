# The real TeamCity, Jira and Confluence

This page covers the three products behind the opt-in Compose profiles: how to set each one up from empty volumes, what its seed puts in, which live suites run against it and what they leave behind. Read it when you write or debug a live test against one of them, or when a seed or a suite stops with a message you do not know. The everyday start and stop (`just live-up`, `just live-down`, the locks) is in [live-products.md](live-products.md).

Commands on this page run in `testenv/` unless the text says otherwise. An agent's shell starts at the repository root, where a hook refuses `cd testenv && ...`, so an agent prefixes each command with `env -C testenv`. `just` recipes run from the repository root.

TeamCity calls a pipeline a build configuration and a run a build. This page says pipeline and run, as [CONTEXT.md](../../CONTEXT.md) does, and keeps TeamCity's words only inside its own names, such as `buildTypes` and `teamcity.builds`.

## What the profiles cost

The three products are off by default because they are expensive. `images.env` holds the digest pins and the pull sizes.

| Product | Profile | Services | Port | Pull, compressed | Also |
| --- | --- | --- | --- | --- | --- |
| TeamCity 2026.1.3 | `real-teamcity` | `teamcity`, `teamcity-agent` | 8111 | about 2.5 GB server, about 1 GB agent | About 10 GB on disk once the data directory exists. The agent publishes no port. |
| Jira Software 10.3.24 | `real-atlassian` | `jira-db`, `jira` | 8080 | about 700 MB | Wants about 4 GB of RAM. Three-hour licence. |
| Confluence 9.2.21 | `real-atlassian` | `confluence-db`, `confluence`, `docs-assets` | 8090, 8095 | about 800 MB | Three-hour licence. |

The administrator is `knobas` / `knobas-dev` on all three, and every port binds 127.0.0.1 only. Jira and Confluence each need their PostgreSQL 15 container. Jira 10 removed the embedded H2 database, so without `jira-db` the setup wizard stops at its database step.

On this machine's 8 GB Docker VM only one of the three runs at a time (Björn, 2026-10-12, #633). `just live-up` and the `*-live` recipes refuse a second one and name the one that is up. A plain `docker compose up` does not check, so prefer the recipes.

## Start a product and run its suites

```sh
just live-up teamcity          # or jira, or confluence
just teamcity-live-seeded      # or jira-live, confluence-live, atlassian-live
just live-down teamcity
```

Each `*-live` recipe starts its product, seeds it, runs its suites serially and puts the product back as it found it. A product that was already up stays up, so `just live-up` first and the recipe second is the fast loop. `just live-down` stops TeamCity and keeps its volumes. It removes Jira or Confluence with their volumes. `just jira-live` and `just confluence-live` each check first that every name in the justfile's `atlassian_live_confluence_tests` is still a test. The other recipes are in [The live recipes](../README.md#the-live-recipes).

One owner at a time. Every suite here asserts exact sets and cannot tell another worktree's writes from leftovers. See [One environment, one owner at a time](../README.md#one-environment-one-owner-at-a-time).

## TeamCity

### Set up TeamCity by hand

You need `docker`, `curl`, `jq` and `openssl`, and Gitea up and seeded with `./seed`. The VCS roots authenticate with the Gitea token in `seed-state.json`, and the seed stops without it.

1. Start the server and its agent.

   ```sh
   docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent
   ```

2. Seed. `./seed-teamcity.sh` is the same command.

   ```sh
   ./seed --teamcity
   ```

3. Load the variables the suites read. This adds `KNOBAS_TEAMCITY_URL` and `KNOBAS_TEAMCITY_TOKEN`.

   ```sh
   eval "$(./seed --env)"
   ```

The setup takes about forty seconds from empty volumes, unattended. The seeded runs come on top. The seed waits up to 600 s for the server to answer, 600 s for it to leave maintenance, 300 s for the agent and 600 s for each run.

Step 2 does this, in order:

1. Walks the first-start wizard: a new installation, the internal HSQLDB database, the licence agreement accepted without usage statistics.
2. Creates the administrator. `TEAMCITY_ADMIN_USER` and `TEAMCITY_ADMIN_PASS` override `knobas` / `knobas-dev`. The password must be printable ASCII, at most 116 characters, because the seed RSA-encrypts it with `openssl` the way the wizard's page does.
3. Mints the access token `knobas-seed` and records it in `seed-state.json`. A later seed reuses it while it authenticates.
4. Waits for the agent `knobas-agent` and authorises it. It also re-enables any agent a killed suite run left disabled (#1049) and deauthorises every other agent (#1560), because the suite expects every seeded run on that one agent.
5. Runs `seed-teamcity-builds.sh`, which creates the content below.

A second `./seed --teamcity` against a set-up server creates nothing and exits 0. A server that was only stopped answers 503 while it starts. The seed prints `the server is working`, waits, then authenticates as the administrator and carries on.

Before you change anything:

- The wizard is not an API. The seed drives TeamCity's own `/mnt/do/*` commands and `createAdminSubmit.html`, so it refuses any image digest other than `VERIFIED_TEAMCITY_IMAGE`. The fix for that refusal is to re-derive the sequence against the new image. The script's header has the recipe.
- Both seed scripts talk to `http://127.0.0.1:8111` and ignore `KNOBAS_TEAMCITY_URL`. The repo-root `.env.example` points that variable at JetBrains' public guest server, and the seed must never post there.
- `just teamcity-live` reads the repo-root `.env`, so by default it runs against that public server. `./seed --env` printing `KNOBAS_TEAMCITY_*` does not change that. The recipe for this container is `just teamcity-live-seeded`.
- `specs/fetch.sh --teamcity` can read `/app/rest/swagger.json` from this server when a specific version is needed. The vendored `specs/teamcity.json` came from the public guest server and carries hand-made additions. Fetch to a scratch path and diff, never onto the vendored file. See [specs/README.md](../specs/README.md).

### What the TeamCity seed creates

The seed derives projects, pipelines and runs from the `builds` in `fixtures/tidewater/work.json` by the rules of `crates/knobas-mockd/src/tc_state.rs`. The ids are the ones the adapter's tests and the project census expect.

| Fixture `cfg` | Project, id and name | Pipeline name | Run | Outcome |
| --- | --- | --- | --- | --- |
| `Ledger_Deploy_Staging` | `Ledger` | `Deploy Staging` | `412` on `main` | SUCCESS. One step that prints one line. |
| `Payout_IntegrationTests` | `Payout` | `IntegrationTests` | `1187` on `feature/PAY-231-sepa-retry` | FAILURE, with a failed test and artifacts |
| `Payout_Build` | `Payout` | `Build` | `1188` on `feature/PAY-231-sepa-retry`, with `--running` only | Running, held at step 3 of 5, `cargo test` |

No pipeline has a description, because the fixture gives none.

**Run 1187.** Its one step prints the fixture's `log`, reports the test that the log's `test <name> ... FAILED` line names as a failed test, publishes artifacts and exits 1. TeamCity composes the `statusText` from the test counts: `Tests failed: 1 (1 new), passed: 0`. The artifacts are `sepa-report.txt` (the log again, #665), `sepa-chart.png` and `sepa-statement.pdf` (the files in `testenv/teamcity-artifacts/`, #2480), and the folder `reports/` holding the same three (#856). The agent has no checkout of `testenv`, so the step carries the two binary files as base64. `live_teamcity_seeded.rs` compares the served bytes with the committed files.

**`Payout_Build`** has five steps so that the server itself says `Step 3/5`. The third is named `cargo test` and sleeps. The others are `step 1`, `step 2`, `step 4` and `step 5`.

The fixture says nothing about the rest, so the seed decides it.

**VCS roots.** One Git root per project, `Payout_PayoutService` and `Ledger_LedgerApi`, at `http://gitea:3000/tidewater/<repo>.git` over the Compose network. Each authenticates with the seed's Gitea token and names every seeded branch of its repository in its branch specification. A mirrored commit's page carries Gitea's own `ROOT_URL`, `http://127.0.0.1:3000`, so a change opens its commit only through a Gitea source whose `clone_urls` lists `http://gitea:3000` (#683). `teamcity_seeded_live.rs` configures that.

**Parameters and a trigger, for the source view (#664).** All on `Payout_IntegrationTests`:

| Where | Name | Value |
| --- | --- | --- |
| Project `Payout` | `tidewater.region` | `eu-central-1`. The pipeline shows it as inherited. |
| Pipeline | `env.SEPA_RETRIES` | `5` |
| Pipeline | `env.SEPA_SANDBOX_KEY` | A password parameter with a made-up value. `live_teamcity_seeded.rs` proves the value never reaches a view. |
| Pipeline | `env.LOG_LEVEL` | A select of `info` and `debug`, labelled *Debug*, default `info`, for the Run dialog's typed fields (#852) |
| Pipeline | VCS trigger on `+:feature/*` | Disabled. Enabled, it would queue a run on every push the Gitea live suite makes. |

**A run that lists a change (#683).** TeamCity lists a change only for a commit it has never seen on any branch, and never for a pipeline's first run on a branch. So the seed keeps the branch `seed/change-witness` in `ledger-api`. It cuts the branch at `main`'s head, runs `Ledger_Deploy_Staging` on it, adds one commit (the file `seed-change-witness.txt`, through Gitea's contents API) and runs the pipeline again. The second run lists that commit as its change. This is the only write the TeamCity seed makes to Gitea. Run 1187 lists no change, because it is its pipeline's first run. Its *Revision* row leads to its commit.

**A chain, for the chain views (#1010).** Project `SeededChain` under `_Root` holds `SeededChain_Compile`, `SeededChain_Package` and `SeededChain_Release`. Each checks out `payout-service` through the project's own VCS root and waits on the one before by a snapshot dependency. One run of `Release` on `feature/PAY-231-sepa-retry` queues all three. The chain is not in `work.json` and no fixture pipeline is in it, so no fixture run number moves. It is not the suite's own `KnobasChain857`, which that test creates and deletes.

The change pair and the chain both need the agent. While `--running` holds it, the seed skips them and says so.

| Key under `teamcity` in `seed-state.json` | Holds |
| --- | --- |
| `url`, `user`, `password`, `token`, `version` | The server, the administrator, the token, the product version |
| `builds[]` | `fixture_number`, `build_type`, `real_id` for each fixture run |
| `settings` | `build_type`, `secret_parameter`, `secret_value` |
| `changes` | `build_type`, `repo`, `branch`, `sha`, `before_id`, `after_id` |
| `chain` | `project`, `branch`, `build_types`, `builds[]` |

Run numbers come out as the fixture's because the seed sets each pipeline's number counter before its first run. Run ids, timestamps and the triggerer (`knobas`) are the server's. Assert the number and look the id up. *What the seed cannot reproduce* in [testenv/README.md](../README.md) has the full list.

If you edit the seed, queue a run on the default branch with no `branchName`. A run queued with `branchName: main` lands on a logical branch `main` that is not `<default>`. The server marks it `defaultBranch: false` and every REST listing under the default filter hides it. The header of `seed-teamcity-builds.sh` says where each endpoint and field name came from.

### What a re-seed changes on an older server

The seed reads everything before it creates it, and it also brings an older server up to date:

- A step script that differs from the current one is rewritten in place.
- A finished run 1187 that reports no failed test, lacks a seeded artifact, or ran from another VCS root URL is deleted and run again under the same number. Its id changes, and `seed-state.json` records the new one.
- A project whose finished run names a commit this Gitea has never held is deleted whole and made again (#964). A Gitea that was recreated and reseeded has other shas. The chain goes the same way, and also when an older seed made it with other steps.
- A VCS root whose branch specification lacks a branch gets it.
- A change pair that no longer matches its record is made again. The seed deletes the branch's runs, cuts the branch again and gives it a new commit.

A mirror that synced the old run 1187 keeps it as a live item beside the new one. A run (kind `build`) is a windowed kind, so no sync or backfill tombstones it ([ADR-0003](../../docs/adr/0003-per-kind-full-sync-exhaustive.md)). *Refresh* on that run answers gone and tombstones the item. `just live-sources-remove` removes the whole source.

A read that fails, or a run that lists no artifacts at all, stops the seed and deletes nothing (#2480). `seed-teamcity-builds-test.sh` checks those decisions without a server.

### Hold the running run

One agent cannot hold a run forever, so the seed starts run 1188 of `Payout_Build` only when asked:

```sh
./seed --teamcity --running
```

The run holds the agent for up to four hours. `--running` and the live suite exclude each other on one environment. To end the run sooner:

```sh
eval "$(./seed --env)"
id=$(jq -r '.teamcity.builds[] | select(.fixture_number==1188) | .real_id' seed-state.json)
curl -X POST -H "Authorization: Bearer $KNOBAS_TEAMCITY_TOKEN" -H 'Content-Type: application/json' \
     -d '{"comment":"released by hand","readdIntoQueue":false}' "$KNOBAS_TEAMCITY_URL/app/rest/builds/id:$id"
```

A cancelled run finishes with status `UNKNOWN`. The agent takes a few seconds to kill the step, so `state` still reads `running` right after the POST. Later seeds leave a finished 1188 alone, with or without `--running`.

To get back to the plain seed, which is what a live suite must find, delete the run:

```sh
curl -X DELETE -H "Authorization: Bearer $KNOBAS_TEAMCITY_TOKEN" "$KNOBAS_TEAMCITY_URL/app/rest/builds/id:$id"
```

The next `./seed --teamcity` rewrites `teamcity.builds` without 1188. `--running` then starts it again as 1188, because the seed resets the counter.

### Run the seeded TeamCity suites

```sh
just teamcity-live-seeded
```

The recipe starts Gitea, then the server and its agent, seeds all three and exports what `./seed --env` prints. When it ends it stops the server and agent if it started them. It never removes them, because the volumes hold the seeded runs. Gitea stays up. Two worktrees that start the recipe together run one after the other on the JVM lock (#690).

| Suite | Crate | What it does |
| --- | --- | --- |
| `live_teamcity_seeded.rs` | `knobas-source-teamcity` | Asserts the seeded content by id, number, status and branch. Runs the contract battery, clause 2 included, which the public server cannot be held to. Watches the `sinceBuild` watermark move on a run it queues and stand still afterwards. |
| `teamcity_seeded_live.rs` | `knobas-app` | Read-only. A seeded run is on the standup digest for the day it ran, `under:` finds each fixture pipeline, and a run's change and revision open the mirrored Gitea commit. |

The adapter suite writes, and takes each write back from a `Drop` that checks. It queues runs through REST and through the adapter, makes projects named `Knobas...`, mints two users and a custom role (#2040, #2055), and for some tests disables the agents or changes the server's permission settings. The watermark test queues `Ledger_Deploy_Staging` on `fix/PAY-228-partial-refund-drift`. That run takes the pipeline's next number and the counter is not wound back, which is harmless, because the seed sets a counter only while the fixture's run does not exist.

A killed run reaches no `Drop`. The next run clears what it left before any test takes a baseline. In the seed's projects (`Ledger`, `Payout`, `SeededChain` and what is nested in them) it cancels and deletes every run whose id is not in `seed-state.json`. Then it deletes every project a run made, meaning one named `Knobas...` or one inside a seed project. So recovery from a dirty server is to run the suite again, never `testenv/reset`. After a green run the seed's projects hold exactly the seeded runs.

Nothing outside the seed's projects is read or deleted. A project under `_Root` with any other name belongs to someone else, such as the `Scale` tree of [Björn's scale](../scale/README.md). `a_project_another_estate_owns_is_left_alone_by_the_cleanup` holds the suite to that.

The suite refuses to start while a seeded run is in flight and says how to release it. A run somebody else queues mid-run is the one case no check catches. The suite reads `testenv/seed-state.json`, or the path in `KNOBAS_TEAMCITY_SEED_STATE`.

### TeamCity troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `REFUSING to touch TEAMCITY`, or `a maintenance stage this script does not know` | The image is not the one the wizard sequence was derived on, or the wizard changed. | Recreate the container from the pin in `images.env`, or re-derive the sequence and update `VERIFIED_TEAMCITY_IMAGE`. |
| `the server has an administrator, but it is not knobas/knobas-dev` | Someone set the server up with other credentials. | Export `TEAMCITY_ADMIN_USER` and `TEAMCITY_ADMIN_PASS` to match. To start over instead, remove the two containers and the volumes `tc-data`, `tc-logs` and `tc-agent-conf` by name, the way `_live_atlassian_remove` in `live-lib.sh` removes Jira's. Do not run the bare `down -v` the message prints. It takes Gitea and its volume too. |
| `no agent named knobas-agent registered in 300s` | The agent container is not running. | `docker compose --env-file images.env --profile real-teamcity up -d teamcity-agent` |
| Every run stays queued, or `no connected, authorised, enabled agent` | A killed suite run left the agents disabled. | `./seed --teamcity` re-enables them. |
| `the recorded access token no longer authenticates` | Another worktree seeded last. The token name `knobas-seed` is shared, and minting it again revokes the earlier one. | Nothing. The seed mints a new token. The other worktree gets the same line on its next seed. |
| `build id N lists no artifacts ... deleting nothing` | A failed read, or the server cleaned the artifacts up. | If they are really gone, delete the run by hand and seed again. |
| The suite panics with `seeded build(s) ... are still in flight` | A seeded run is queued or running, usually 1188. | Cancel it or wait, then run the suite again. |

## Jira and Confluence

### The three-hour licence

Both products run on Atlassian's Data Center timebomb keys: 10 users, three hours, free, public, no my.atlassian.com account. Atlassian ended self-service 30-day Data Center trials on 2026-03-30, so these are the only free licences left.

- `./fetch-timebomb-keys.sh` reads the keys from Atlassian's page and checks that each decodes to the 10-user, 3-hour Data Center licence for its product. It needs `python3` and network access. There is nothing to type or paste.
- The keys belong in your shell, never in `images.env`, which is tracked. `./fetch-timebomb-keys.sh --write` also writes them to the gitignored `.env.licences`, so they survive a new terminal.
- Confluence reads `CONFLUENCE_LICENSE_KEY` at first-time setup only, so fetch the keys before the container's first start. `seed-atlassian.sh` checks the container's own environment for the key, not your shell's.
- Jira has no licence variable. Its key goes in through the wizard, and the seed fetches it itself when `JIRA_LICENSE_KEY` is unset.
- The licence runs out three hours after the product applies it. Restarting the product renews it for another three hours and keeps its data (measured on the WSL box, 2026-11-01, #2373). `just live-up` and the `*-live` recipes restart a product that has less than an hour left (`KNOBAS_LIVE_LICENCE_MARGIN_S`, 3600 by default). See [live-products.md](live-products.md).
- The versions are pinned to what a timebomb key starts. Jira 11.x is reported to reject the key. `seed-atlassian.sh` refuses any other image digest, because the wizard forms it drives are not an API and change between versions.

So the usual shape is to stand a product up, run what needs it and remove it with `just live-down jira` or `just live-down confluence`. That removes the product, its database, both volumes and its block in `seed-state.json`. For Confluence it also removes `docs-assets`.

Never run a bare `down -v`. A profile-scoped `down -v` with no service named also takes the default profile's containers and volumes, Gitea and its seeded corpus included. With the services named it still removes the project network once nothing running is on it, and every stopped container attached to that network then fails its next `up`.

### Set up Jira and Confluence by hand

`just live-up jira` and `just live-up confluence` each do their half of this. Do it by hand only when you need to watch a step. You need `docker`, `curl`, `jq` and `python3`.

1. Fetch the keys, before either `up`.

   ```sh
   eval "$(./fetch-timebomb-keys.sh)"
   ```

2. Start Jira alone, walk its wizard and seed its content. Jira's restart after the wizard is the slowest step, and a second JVM starting beside it on the 8 GB VM is what pushed that restart past its wait (#314).

   ```sh
   docker compose --env-file images.env --profile real-atlassian up -d --wait jira-db jira
   ./seed-atlassian.sh jira
   ./seed-atlassian-content.sh jira
   ```

3. Start Confluence and walk its wizard. On the 8 GB VM, remove Jira first.

   ```sh
   docker compose --env-file images.env --profile real-atlassian up -d --wait confluence-db confluence
   ./seed-atlassian.sh confluence
   ```

4. Start the server the showcase's embeds load from. See [The docs-assets server](#the-docs-assets-server).

5. Seed Confluence's content.

   ```sh
   ./seed-atlassian-content.sh confluence
   ```

6. Load the variables the suites read. This adds `KNOBAS_JIRA_*` and `KNOBAS_CONFLUENCE_*` for each product that is seeded.

   ```sh
   eval "$(./seed --env)"
   ```

On a machine with the memory for both at once, bring the pair up together and run `./seed --atlassian`. It walks both wizards, Jira first, and seeds both.

`seed-atlassian.sh` creates the administrator `knobas` / `knobas-dev` on both. `ATLASSIAN_ADMIN_USER`, `ATLASSIAN_ADMIN_PASS` and `ATLASSIAN_ADMIN_MAIL` override it. Re-running either seed against a finished instance creates nothing. The content seed reads before every create and ends with `done: created N, skipped M`, so against a half-seeded instance it finishes the job.

To prove a seed, add `--verify` to `./seed-atlassian-content.sh jira` or `confluence`:

| `--verify` for | Checks |
| --- | --- |
| Jira | PAY-231 is there with its worklogs and is in an active sprint. A throwaway issue in `NARROW` is offered fewer statuses than the project has, then deleted. |
| Confluence | A CQL search of `ENG` returns exactly the seeded pages and the home page, waited for up to 180 s. *SEPA payout retry design* has its body and sits under *Payments architecture overview*. |

The throwaway issue is the only write `--verify` makes. It goes to `NARROW` and never to a fixture project, where it would move the key counter.

### What the wizard waits print

`seed-atlassian.sh` waits in four places. The limits are constants at the top of the script.

| Wait | Up to | Constant |
| --- | --- | --- |
| The product answers `/status` with `FIRST_RUN` or `RUNNING` | 600 s | `FIRST_RUN_CAP_S` |
| Jira serves its first wizard form, about a minute after `FIRST_RUN` | 600 s | `FIRST_RUN_CAP_S` |
| Jira reaches `RUNNING` after the wizard | 900 s | `JIRA_RUNNING_CAP_S` |
| A wizard form accepts a POST. A product that already serves the form can still answer 500, or not answer, while it warms. | 300 s | `WIZARD_POST_CAP_S` |

Each wait prints what the product shows and how far into the limit it is, every 30 s. One poll times out after 10 s (`POLL_TIMEOUT_S`). One wizard POST times out after 240 s (`WIZARD_POST_TIMEOUT_S`), because a cut POST could apply a step twice. The slowest POST measured is 46 s (#367). A hung POST is therefore silent for 240 s, prints one line and fails, at worst 545 s in.

A state that climbs is a slow start. One state repeated to the limit is a hang. `UNREACHABLE` throughout is a container to read `docker logs` for. A wait that runs out says something about the machine, such as a second JVM starting beside the product, so do not widen it as if it were a flake.

### Run the Jira and Confluence suites

```sh
just atlassian-live      # jira-live, then confluence-live
```

Jira is gone before Confluence starts. Each half fetches the keys, starts the product, runs the wizard seed, runs the content seed twice (the second run must write nothing, #396) and `--verify`, then runs every suite gated on that product's URL, serially. It removes the product with its volumes from a trap, so the teardown also runs when a step fails and on Ctrl-C. A product that was already up is used as it is and left up.

Measured on #633's runs (12 cores, the 8 GB VM, images pulled, empty volumes): `jira-live` takes 299 s in all, seeded and verified at 244 to 263 s, its three suites about 15 s once compiled. `confluence-live` takes 91 to 104 s, seeded and verified at 56 to 58 s.

| Recipe | Suite | What it covers |
| --- | --- | --- |
| `jira-live` | `crates/knobas-source-jira/tests/live_jira_seeded.rs` (#276) | The adapter: sync, payload, cursor, the real 401, attachments |
| `jira-live` | `crates/knobas-app/tests/atlassian_live.rs`, Jira half | The engine and the write queue: credential health end to end, the write ops read back out of Jira, `under:` on the seeded epic |
| `jira-live` | `crates/knobas-app/tests/share_exit.rs` (#455) | The share export restored on a clean profile. Reads only. |
| `confluence-live` | `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` (#284) | The adapter. This is its only test against a server. [ADR-0013](../../docs/adr/0013-the-real-instance-is-the-witness-a-mock-certifies-nothing.md) refused a mock, because Confluence has no machine-readable spec to make one from. |
| `confluence-live` | `crates/knobas-app/tests/atlassian_live.rs`, Confluence half | A comment that mentions me becomes an inbox item, credential health, the launcher's ancestor path |
| `confluence-live` | `crates/knobas-app/tests/confluence_live.rs` | The page write path, and the showcase pages as the Docs reader's blocks |

To run one suite against a product that is already up and seeded, run `eval "$(./seed --env)"` and then call cargo from the repository root:

```sh
env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-jira --test live_jira_seeded -- --ignored --nocapture --test-threads=1
```

`atlassian_live.rs` holds tests for both products, so it runs twice. With Jira it skips the names in `atlassian_live_confluence_tests`. With Confluence it runs exactly those. When you add a Confluence test to the file, add its name to that list. A Confluence test missing from the list fails in the Jira run on the unset `KNOBAS_CONFLUENCE_URL`. A listed name the file no longer has stops both recipes before they start a product.

Every suite but `share_exit` writes, and each takes back what it wrote from a `Drop` that checks. A killed run reaches no `Drop`, and the next run cleans up after it:

- **Jira.** The next run of `live_jira_seeded` restores the seed's projects from `seed-state.json` and prints what it put back: a stray issue with its sub-tasks, comment or attachment deleted, the `knobas-live-suite` label removed, a status moved back through the workflow. It works from the file, so it also clears what `atlassian_live` left.
- **Confluence.** The next run of `live_confluence_seeded` restores every seeded page's title from `seed-state.json`. The next content seed deletes every page in `ENG` the seed did not create. It skips a fixture page that exists, so it does not put an edited body back.

So after a green run the seed's projects hold exactly the seeded corpus, and recovery from a dirty Jira is to run the adapter suite again.

The cleanup searches the seed's projects only: `PAY`, `OPS` and `NARROW`. Every source a suite configures is scoped to them unless a test names its own scope. Other projects on the same Jira, such as the `SCL*` projects of [Björn's scale](../scale/README.md), are neither touched nor counted.

No suite can undo the `PAY` key counter. A created ticket moves it one further, so nothing may assume the fixture's keys are the highest.

### What the Jira seed creates

`seed-atlassian-content.sh` reads `fixtures/tidewater/work.json`, the same file the demo loader compiles in.

| What | Detail | Key under `jira` in `seed-state.json` |
| --- | --- | --- |
| Users | The fixture's five people, password `tidewater-dev`. With `knobas` that is six accounts on the ten-user key. | |
| Projects `PAY` and `OPS` | From Jira Software's *Basic software development* template, `com.pyxis.greenhopper.jira:basic-software-development-template` | `projects`, `template_key`, `statuses` |
| Seven issues | PAY-200 (the epic), PAY-219, PAY-228, PAY-231, PAY-236, PAY-240 and OPS-77, each at its fixture key with its type, priority, assignee, estimate, description, status, Epic Name and Epic Link | `issues[]` |
| Comments and worklogs | The fixture's, on their issues. A worklog starts on the fixture's date at 09:00 UTC. | `issues[].comments`, `issues[].worklogs` |
| A *Blocks* link | From the fixture's `blocked_by`: PAY-228 is blocked by OPS-77 | |
| A Scrum board and a sprint (#811) | `PAY sprint board`. Its sprint `PAY Sprint 1` is active and holds PAY-231 only. | `sprint` |
| A Kanban board (#877) | `PAY and OPS flow` over `project in (PAY, OPS) ORDER BY Rank ASC`, with a limit of 2 on its In Progress column | `views.kanban` |
| A favourite filter (#877) | `PAY without a fix version`. Its JQL has a clause knobas cannot translate, `fixVersion is EMPTY`. | `views.favourite_filter` |
| Project `NARROW` (#522) | See below. It holds no issues. | `narrowing` |
| Two attachments on OPS-77 (#2410) | See below | `attachments` |
| This instance's field ids | Epic Link and Sprint | `epic_link_field`, `sprint.field` |

**The workflow of `PAY` and `OPS`** is *Software Simplified Workflow for Project `<KEY>`*: To Do, In Progress, In Review, Done, with every transition available from every status. Those are the fixture's four statuses and the names the live suites' transition tests use. The template's issue type scheme has no Story, so the seed adds the global Story type to each project's scheme. If a fixture status were ever missing from the workflow, `jira.unreachable_statuses` would say which issue kept which status. Today it is empty.

**The keys are the fixture's because the seed burns the keys in front of them.** Jira allocates keys from a per-project counter that no REST call sets. The seed bulk-creates placeholders (summary `(reserved)`, label `knobas-placeholder`) up to the key before each fixture issue, creates the issue, asserts its key and deletes every placeholder at the end. So the fixture's keys are reachable only in a project the seed created from empty. An issue deleted by hand cannot come back at its key, and the seed stops and says so.

**`NARROW` is a project whose workflow narrows (#522).** `PAY` and `OPS` offer every status from every status, so they cannot show that the reachable-transition read returns a subset. *Narrowing workflow fixture* comes from Jira Core's process-management template, `com.atlassian.jira-core-project-templates:jira-core-process-management`, which has seven statuses:

| From | Offers |
| --- | --- |
| Open | In Progress |
| In Progress | Under Review, Cancelled |
| Under Review | Approved, Rejected |
| Approved | Done |
| Done | nothing |
| Cancelled | Open |
| Rejected | In Progress |

It is not fixture content. It is recorded as `jira.narrowing` and not in `jira.projects`, and it holds no issues between runs. The test in `atlassian_live.rs` files its own ticket, walks it and deletes it, and refuses to start without `jira.narrowing`. With `NARROW` this Jira's whole status list (`GET /rest/api/2/status`) has nine entries, not four.

**The attachments on OPS-77 (#2410)** are the two files in `testenv/jira-attachments/`, a PNG and a PDF. The seed skips a name the issue already carries and fails unless each is there once at its own size. They are on an `OPS` issue because `OPS` is the project the adapter suite narrows to ask for a file as a user who may not browse it. `jira.attachments` records the issue, the ids and the `content` addresses of those two and no others, so anything else attached to a seeded issue is a stray the adapter suite deletes (#2471). The suite reads both back byte for byte against the files on disk.

### What the Confluence seed creates

| What | Detail | Key under `confluence` in `seed-state.json` |
| --- | --- | --- |
| Space `ENG` | Named *Engineering*. The fixture gives only the key. | `space`, `home_page_id` |
| Six fixture pages | See the next table. Comments are authored by `author`. | `pages[]`, each with `fixture_id`, `title`, `id`, `parent_id`, `comments`. Also `author`. |
| The `html` macro, enabled (#2440) | Confluence ships it disabled, and the embed tests need it. | |
| Space `ARCHIVE` | Named *Archive (not synced)*. No seeded knobas configuration syncs it. | `showcase.outside_space` |
| User `knobas-mentioned` | The account the showcase mentions. Never the seed's own, whose inbox a mention would fill. | |
| Fourteen showcase pages | Thirteen in `ENG`, one in `ARCHIVE`. See [The Docs reader showcase](#the-docs-reader-showcase). | `showcase.pages[]`, `showcase.assets` |

| Fixture page | Sits under | Content |
| --- | --- | --- |
| *Payments architecture overview* | The home page | Empty |
| *SEPA payout retry design* | *Payments architecture overview* | The fixture's two `##` sections as `<h2>` and `<p>`, and one comment |
| *Ledger reconciliation runbook* | The home page | Empty |
| *On-call handbook* | The home page | Empty |
| *Standup protocols* | The home page | Empty. The standup flow publishes under it. |
| *Daily protocol 2026-08-21* | The home page | Empty |

Only *SEPA payout retry design* has a body and a comment. The other five are titles, which makes them the test that an empty page is still a page.

The tree is the seed's decision. *SEPA payout retry design* is nested one level deeper so that a launcher hit's ancestor path has two segments. Over a flat space the path's separator and its outermost-first order would be tested on fixtures only (#396).

The space belongs to the seed. After it has found or created its own pages, the content seed deletes every other page in `ENG`, trashed and then purged (#2111). A killed suite run reaches no guard, and its page would otherwise come back in the next full sync.

### The Docs reader showcase

Besides the fixture's pages, the seed makes pages that hold every construct the Docs reader draws (spec #2432), to open in knobas and look at (Björn, 2026-11-02, ruling 6 of [the spec #2432 decision](../../docs/decisions/2026-11-02-spec-2432-docs-whole-pages.md), #2456). They hang under the home page of `ENG`, so `just live-demo` shows them in Docs under *Engineering*.

| Page | Under | What it shows |
| --- | --- | --- |
| *Docs reader showcase* | The home page | A mention of `knobas-mentioned`, a page link, the `jira` macro on PAY-231, a list, code, the four panels, a table, a `toc` placeholder. Labelled `knobas-docs`. |
| *Showcase: tables* | *Docs reader showcase* | Merged cells (colspan and rowspan), a header column, cells that hold a list, paragraphs, code, a panel, a nested table, a collapsible, an excerpt include and an HTML embed, and a fourteen-column table that scrolls sideways |
| *Showcase: task lists and expands* | *Docs reader showcase* | A task list with checked, unchecked and nested items. Collapsibles, titled and untitled, that hold a list, a table and a panel. Search Docs for *quarantine ledger* or *settlement drift* to land in one. |
| *Showcase: includes* | *Docs reader showcase* | An include and an excerpt include of *Showcase: payout runbook*. An include of *Retired payout runbook* in `ARCHIVE`, which is not mirrored. An excerpt include of *Showcase: on-call glossary*, which has no excerpt. A chain four deep, *Showcase: include depth 1* to *4*, the last too deep. A cycle, *Showcase: include cycle A* and *B*. All of these sit under this page. |
| *Showcase: embeds* | *Docs reader showcase* | An `html` embed whose script writes the number it shows. An `html` embed that runs a self-hosted Swagger UI on the payouts OpenAPI file. An `iframe` embed of a status page. A widget-connector embed of a YouTube video, the one embed that needs the internet. An `iframe` of a `javascript:` address, which knobas refuses to frame. |

The showcase page and the runbook name PAY-231, so the backend's `page_text_key` rule proposes a link from each to that ticket. The seeded Jira tickets themselves link no page.

How the pages are kept:

- The sources are `testenv/confluence-showcase/*.xml` in storage format. `manifest.json` gives the order, each page's parent, its space and its labels.
- Each page carries a `knobas-seed` content property with the sha256 of the storage the seed wrote. An unchanged file is a skip. An edited file reaches its page on the next seed as a new version.
- `seed-state.json` records them under `confluence.showcase`, apart from `confluence.pages`, which stays the fixture's corpus. The ones in `ENG` count as seeded pages for `--verify` and for the adapter suite's bound on a full sync of the space.
- `confluence_live.rs` reads these pages (blocks, tables, task lists and collapsibles, embeds, includes) and makes no page of its own for them. The pages it still makes are for the write paths and for the search and backlink marks (#860).

### The docs-assets server

The embeds load their files from `docs-assets` on port 8095, a Caddy container in the `real-atlassian` profile. It serves `testenv/docs-assets/` with `Access-Control-Allow-Origin: *`, because an embed's frame has an opaque origin and Swagger UI fetches its spec. `openapi.json` and `status.html` are in git. `./docs-assets.sh fetch` downloads Swagger UI's bundle and stylesheet once from npm (`swagger-ui-dist`, pinned version and integrity) into the gitignored `docs-assets/swagger-ui/`.

`just live-up confluence` and `just confluence-live` start the container, copy the files in and remove it with Confluence. The files reach the container by `docker cp` and not a bind mount, so the WSL target gets them too, and port 8095 is on the tunnel's list.

A seed by hand does not start it. To start it yourself:

```sh
./docs-assets.sh fetch
docker compose --env-file images.env --profile real-atlassian up -d --wait docs-assets
docker cp "$(./docs-assets.sh dir)/." knobas-docs-assets:/srv/
```

`KNOBAS_DOCS_ASSETS_URL` changes the address the seed writes into the pages. The default is `http://127.0.0.1:8095`.

### What the content seed cannot reproduce

- Created and updated timestamps. The server stamps now.
- The fixture's `spent_week_m`.
- A page's `edited` date and author.
- Ids. Issue, comment, worklog, page and page-comment ids, the space home page and the custom field ids are the server's.

A live assertion looks a literal id up in `seed-state.json` (`jira.issues`, `jira.epic_link_field`, `confluence.pages`, keyed by fixture key and fixture page id) and asserts everything else by key, title and text.

### Rules for a live test against these two

**Never send this Jira a wrong password.** Jira counts failed password logins per account. Past the container's default of a few it answers `403 Basic Authentication Failure - Reason : AUTHENTICATION_DENIED`, to the correct password as well, until an administrator clears the check. A suite that draws its 401s from a wrong password locks `knobas` out part-way through its own run, and everything after fails on a cause no failure names. Take refusals from a bearer token, which is not a login attempt and counts against nothing.

**A bearer token Jira cannot resolve searches anonymously.** It is not a failed login. `/rest/api/2/serverInfo`, `/myself` and `/issue/{key}/...` answer 401 with no `X-Seraph-LoginReason` header. `/rest/api/2/search` answers 200 with `total: 0`, because asking needs no permission. Only a wrong password carries `X-Seraph-LoginReason: AUTHENTICATED_FAILED`, with Jira's HTML login page for a body and no `errorMessages` envelope. Never read that empty 200 as an empty corpus. Deviations 14 to 16 in `crates/knobas-mockd/src/lib.rs` record all of it.

**Epic membership is in the Epic Link custom field, and only there.** `PAY` and `OPS` are classic Data Center projects, so `fields.parent` is absent from every issue, the epic's children included. The field's id is this instance's own and differs between seeds of the same script, which is why the seed records it as `jira.epic_link_field`. A Jira source configured without the adapter's `epic_link_field` option mirrors no epic membership from this server. `knobas-mockd` serves both spellings, so only this server shows the difference.

**Configure the administrator's username.** `knobas` authors every comment and worklog, because neither product's REST takes an author. `seed-state.json` records it as `jira.author` and `confluence.author`, and `KNOBAS_JIRA_USER` and `KNOBAS_CONFLUENCE_USER` from `./seed --env` already say it. The fixture's people exist so that assignees are the fixture's. Nothing is written as them.

### Jira and Confluence troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `REFUSING to touch JIRA` or `CONFLUENCE`, or `a wizard step this script does not know` | The image is not the one the wizard sequence was derived on, or the wizard changed. | Recreate the container from the pin in `images.env`, or re-derive the sequence and update `VERIFIED_JIRA_IMAGE` or `VERIFIED_CONFLUENCE_IMAGE`. |
| `confluence was started WITHOUT a licence key` | The key was not in the environment at the container's first start. | `eval "$(./fetch-timebomb-keys.sh)"`, then `just live-down confluence`, then start it again. |
| `jira never served its first setup step` | The message names the form Jira shows. The database step means `ATL_JDBC_*` did not reach the container. A later step means an earlier run walked part of the wizard. | For a part-walked wizard, `just live-down jira` and set it up from empty. Do not resume it. |
| `jira did not accept the licence key` | On the pinned version, the key was truncated or Atlassian withdrew it. | Fetch the keys again. |
| A wait runs out with one state repeated or `UNREACHABLE` throughout, or `never accepted a POST` | A hang, a container that never bound its port, or a product that is up and broken. | `docker logs --tail 50 knobas-jira` or `knobas-confluence`. The message quotes the last answer. |
| `seed-state.json.lock has been held for 10 s` | A dead seed left the lock directory. | If no other seed is running, `rmdir seed-state.json.lock`. |
| `PAY-231 cannot be PAY-231: project PAY is already at ...`, or `exists but is "..."` | The key counter is past the fixture's key. | `just live-down jira`, then set it up and seed again. |
| Every request answers `403 ... AUTHENTICATION_DENIED` | Too many wrong passwords locked `knobas` out. | The quick way inside a session is the command below. The clean way is `just live-down jira` and a fresh seed. |
| The content seed fails on a 503 right after Jira started from a stopped container | Jira says `RUNNING` while the plugins behind its REST API still warm up. | Wait and seed again. `just live-up jira` waits for the API itself, up to `KNOBAS_LIVE_API_WAIT_S`, 300 s by default. |
| `Confluence's html macro is disabled` | Turning it on through the plugin manager did not take. | As an administrator: Manage apps > System > Confluence HTML Macros > the `html` modules. Then seed again. |
| `page "..." is already in ENG with no ancestors at all` | The page was not created by the seed. | `just live-down confluence`, then set it up and seed again. |
| `--verify` reports pages missing from, or extra in, a CQL search after 180 s | Confluence's search has not caught up, or a stray page is still there. | Run the content seed again, which deletes strays, then `--verify`. |

Clearing the lockout by hand:

```sh
docker exec knobas-jira-db psql -U jira -d jira -c "update cwd_user_attributes set attribute_value='0' where attribute_name = 'login.currentFailedCount'"
```
