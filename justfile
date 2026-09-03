# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# `front` runs before the cargo recipes because it is the fast half: a
# svelte-check error or a broken `vite build` is reported in seconds instead of
# after a full workspace compile. It is *not* a build dependency of the cargo
# recipes -- `tauri::generate_context!` only reads `frontendDist` when the
# `custom-protocol` feature is on (production, i.e. `tauri build`); a dev build
# with `devUrl` set takes the empty default asset set and never looks at
# `app/dist`. Nothing here may create that directory either: a missing one is
# exactly how `tauri build` refuses to bundle an app with no frontend in it.
check: fmt front shell clippy clippy-libs inventory test

# The test inventory: every test this workspace defines, by name, committed.
#
# `just check` going green is not evidence that the tests you wrote still
# exist. A test that is *deleted* takes its own failure with it, so the suite
# reports success on what is left and the count moves in whatever direction the
# rest of the commit pushed it. This has happened here: a mis-scoped splice
# removed four tests in one commit while three were added in the same commit,
# so the file-level count *rose*; the loss was found only because a mutation
# that used to die stopped dying, and two of the four survived a name-by-name
# audit because an audit can only look for names someone already suspects.
#
# Diffing the names is the check that does not depend on suspicion. A deleted
# test cannot hide behind an added one, because both are lines.
#
# This does not forbid deleting a test -- retiring one is often right. It makes
# the deletion *visible*: `just inventory-update` turns it into a `-` line in
# the PR diff, where a reviewer sees it and asks why, instead of it being
# invisible against a green suite.
#
# Scope: unit and integration tests, keyed by package, target and test name, so
# two crates that both have `tests/mockd.rs` stay distinct and a test that moves
# between crates reads as one removal plus one addition. Doctests are not listed
# -- cargo builds no binary for them, so there is nothing to enumerate; they are
# still run by `test`. Nor are the tests named in
# `test-inventory-conditional.txt`: a `#[cfg(target_os = ...)]` test exists on
# one platform and not another, and the file is written on macOS and checked on
# `ubuntu-latest`, so it cannot be pinned by a list that has to match on both.
# Each line there costs the gate one test and is argued for in place.
#
# The scratch file is per-invocation, from `mktemp`. It used to be a fixed
# `/tmp` path, which two `just check` runs on one machine -- the parallel
# worktrees this repo is worked in -- wrote at the same time, so the `diff`
# read a half-written or foreign file and the gate failed on lines belonging to
# nobody's tree. A gate that can fail for reasons unconnected to its own diff
# is one everybody learns to re-run, which is how a real `-` line gets waved
# through; that is the failure this recipe exists to prevent, so its scratch
# file cannot be shared. The atomic write below removes the *torn* read, not the
# *foreign* one -- a shared path would still hand this `diff` another worktree's
# perfectly complete inventory -- so `mktemp` here is not redundant.
#
# Both recipes trap `INT` and `TERM` as well as `EXIT`, because bash does not run
# an `EXIT` trap when a signal it has no handler for terminates the shell: a
# `SIGTERM`ed run was observed leaving its scratch file in `$TMPDIR`. Each signal
# trap cleans up, restores the default disposition and re-raises, so the recipe
# still dies of the signal it was sent rather than reporting a tidy exit 0.
#
# `inventory-update` writes `test-inventory.txt` in place -- that path is inside
# the worktree, so no two worktrees share it -- and the write is atomic:
# `_inventory-write` builds into a sibling temp file and `mv`s it over, so a
# reader sees the whole old inventory or the whole new one. It used to be a
# plain redirect, which truncates the destination the instant the pipeline
# starts, and the first stage of that pipeline is a full cargo build. The
# committed file therefore sat *empty* for the minutes that build ran: a
# `just check` in the same worktree diffed against nothing, and an interrupted
# `inventory-update` left an empty file behind to be committed. An empty
# inventory is the worst state this file can be in -- it makes every test read
# as deleted, which is the exact failure the gate exists to make visible.
inventory:
    #!/usr/bin/env bash
    set -euo pipefail
    actual=$(mktemp "${TMPDIR:-/tmp}/knobas-inventory-actual.XXXXXX")
    trap 'rm -f "$actual"' EXIT
    trap 'rm -f "$actual"; trap - INT; kill -INT $$' INT
    trap 'rm -f "$actual"; trap - TERM; kill -TERM $$' TERM
    just _inventory-write "$actual"
    if ! diff -u test-inventory.txt "$actual"; then
        echo >&2
        echo "error: the test inventory does not match test-inventory.txt." >&2
        echo "  '-' lines are tests that no longer exist. If that is deliberate," >&2
        echo "  run 'just inventory-update' and commit it, so the removal shows up" >&2
        echo "  in the diff a reviewer reads." >&2
        exit 1
    fi

# Regenerate `test-inventory.txt`. Run this whenever you add or remove a test,
# and commit the result alongside the change that caused it.
inventory-update:
    #!/usr/bin/env bash
    set -euo pipefail
    just _inventory-write test-inventory.txt
    echo "test-inventory.txt: $(wc -l < test-inventory.txt | tr -d ' ') tests"

# Enumerate the workspace's tests into the file named by $1.
#
# `--no-run` builds the test binaries and `--message-format=json` names them;
# asking each binary to `--list` itself is what makes the result the *harness's*
# answer rather than a guess parsed out of the source. `--list` enumerates, it
# does not execute, so nothing here starts a database.
#
# The package name comes from `package_id`, which cargo spells two ways
# depending on version (`<path>#<version>` and `<path>#<name>@<version>`); both
# are handled. LC_ALL=C keeps the sort byte-wise, so the file does not churn
# when a machine's locale differs.
#
# `test-inventory-conditional.txt` is subtracted here rather than at diff time,
# so `test-inventory.txt` means one thing on every machine: the tests that exist
# everywhere. Filtering only the *actual* side would leave the committed file
# platform-shaped, and filtering both sides at the diff would still let
# `inventory-update` write a file whose contents depend on who ran it.
#
# The result is assembled in a temp file and `mv`d onto `$1`, because `$1` may
# be the committed `test-inventory.txt` and a redirect would empty it for the
# length of the build above. The temp file is a *sibling* of the destination on
# purpose: `mv` is atomic only within one filesystem, and a `$TMPDIR` that is a
# different mount would silently turn it back into copy-then-delete, i.e. a
# destination that is observably partial again. Nothing ever reads the temp
# file, so its only obligation is not to outlive the recipe -- hence the trap,
# which covers the interrupt that used to be what left a truncated inventory in
# the tree.
#
# The temp file is seeded from the destination first, for its *mode* and not its
# contents: `mktemp` creates 0600 and `mv` carries the temp file's permissions
# onto the destination, so a bare rename would tighten `test-inventory.txt` from
# 0644 to 0600 on every `inventory-update` -- silently, because git tracks only
# the exec bit, so neither the gate nor the diff would ever show it. Copying the
# destination over the temp first makes the replacement inherit the mode it
# replaces, which is what the redirect did.
_inventory-write FILE:
    #!/usr/bin/env bash
    set -euo pipefail
    tmp=$(mktemp "{{FILE}}.XXXXXX")
    trap 'rm -f "$tmp"' EXIT
    trap 'rm -f "$tmp"; trap - INT; kill -INT $$' INT
    trap 'rm -f "$tmp"; trap - TERM; kill -TERM $$' TERM
    if [ -e "{{FILE}}" ]; then cp -p "{{FILE}}" "$tmp"; fi
    env -u RUSTUP_TOOLCHAIN cargo test --workspace --no-run --message-format=json 2>/dev/null \
      | jq -r 'select(.executable != null and .profile.test == true)
               | (.package_id | if test("#.*@") then (split("#")[1] | split("@")[0])
                                else (split("#")[0] | split("/") | last) end) as $pkg
               | "\($pkg)\t\(.target.kind[0])/\(.target.name)\t\(.executable)"' \
      | while IFS=$'\t' read -r pkg target exe; do
            "$exe" --list 2>/dev/null | sed -n 's/: test$//p' | sed "s|^|${pkg}\t${target}\t|"
        done \
      | grep -vxF -f <(sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' test-inventory-conditional.txt) \
      | LC_ALL=C sort > "$tmp"
    mv "$tmp" "{{FILE}}"

# shellcheck over every tracked script in testenv/.
#
# It lives here because the workflow that used to own it does not run. The
# `testenv` workflow is disabled along with `check` (billing, 2026-08-29), so
# from then until it is re-enabled nothing linted these scripts at all -- which
# is how a `# shellcheck disable=SC2016` came to sit one line above the wrong
# command for a whole commit. A gate nobody runs is not a gate.
#
# DISCOVERY IS BY SHEBANG, NOT BY GLOB, and the reasoning is the workflow's:
# a glob cannot fail loudly when a new script stops matching it, it just
# quietly checks less than you think. `git ls-files` rather than `find`, so a
# seeded volume or a stray local script can never enter the set. Kept
# deliberately parallel to `.github/workflows/testenv.yml`, which does the same
# discovery and should keep agreeing with this.
#
# THE EMPTY GUARD IS THE POINT, not decoration. Bare `shellcheck` exits 3 with
# "No files specified.", so a discovery that matched nothing would fail -- but
# it would fail in a usage dump that reads as a broken recipe rather than as a
# discovery that stopped matching. It also keeps the recipe off the exit code
# of a tool whose "no arguments is an error" is a behaviour, not a promise.
#
# The container is pinned; a local binary is preferred when there is one, so
# `just check` does not start requiring Docker of everyone. The two can differ
# in version -- 0.11.0 here against the 0.9.0 that ubuntu-latest ships -- and a
# stricter local gate is the right way round while local *is* the gate.
shell:
    #!/usr/bin/env bash
    set -euo pipefail
    scripts=()
    while IFS= read -r f; do
        [ -f "$f" ] || continue
        if head -n 1 -- "$f" | grep -qE '^#!.*\b(ba)?sh\b'; then
            scripts+=("$f")
        fi
    done < <(git ls-files -- testenv)

    if [ ${#scripts[@]} -eq 0 ]; then
        echo "error: shebang discovery matched no scripts under testenv/ --" >&2
        echo "  nothing was linted. Fix the discovery, do not delete this guard." >&2
        exit 1
    fi

    echo "shellchecking ${#scripts[@]} scripts"
    if command -v shellcheck >/dev/null; then
        shellcheck "${scripts[@]}"
    elif command -v docker >/dev/null; then
        docker run --rm -v "$PWD:/mnt" -w /mnt \
            koalaman/shellcheck:v0.11.0 "${scripts[@]}"
    else
        echo "error: neither shellcheck nor docker is available." >&2
        echo "  brew install shellcheck (or start Docker)." >&2
        exit 1
    fi

fmt:
    env -u RUSTUP_TOOLCHAIN cargo fmt --all --check

clippy:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets -- -D warnings

# The same lint pass over the libraries alone -- which is the only way to see
# them the way something that merely *depends* on them does.
#
# `--all-targets` pulls in every crate's dev-dependencies, and cargo unifies
# features across the whole invocation. `knobas-db` dev-depends on itself with
# `test-util` on, so that one dev-dependency silently turns the feature on for
# the library build too, and the lib gets linted in a configuration nothing
# ships. Items whose only callers are behind `test-util` then look live, and
# their dead-code warnings surface only later -- in `tauri dev`, or for any
# consumer building `knobas-db` without the feature.
#
# Dropping `--all-targets` for `--lib` drops the dev-dependencies with it, so
# each library is checked with the features a consumer actually gets. Almost
# free after the pass above: same crates, same profile, a subset of the units.
clippy-libs:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --lib -- -D warnings

test:
    env -u RUSTUP_TOOLCHAIN cargo test --workspace

# Install app/ dependencies if they are missing or older than the lockfile.
#
# `npm ci` deletes and reinstalls node_modules wholesale: right on a fresh
# clone or after the lockfile moves, pure waste on every other run. Staleness
# is a timestamp comparison, not a content one -- npm's hidden
# `node_modules/.package-lock.json` is its own flattened view of the tree and is
# never byte-equal to `package-lock.json`, so comparing the two files would
# reinstall every single time. npm rewrites the hidden file on install, so
# "lockfile is newer" means exactly "installed against an older lockfile",
# including after a pull or a branch switch.
deps:
    #!/usr/bin/env bash
    set -euo pipefail
    cd app
    if [ ! -d node_modules ] || [ package-lock.json -nt node_modules/.package-lock.json ]; then
        npm ci
    fi

front: deps
    cd app && npm run check && npm run build

# The desktop app against the embedded database. Set KNOBAS_DB_URL to point it
# at a server you manage instead.
#
# The Tauri CLI is a devDependency of `app/` (there is no `cargo tauri-cli` in
# this repo), so `deps` is what provisions it; it then has to run from the crate
# that owns `tauri.conf.json`, which is why the CLI comes off PATH rather than
# from the current directory.
dev: deps
    cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri dev

# The demo profile: its own data directory, its own database on its own port,
# its own keychain service. Demo data never mixes with a real corpus
# (interfaces §8 P13), and `demo_load` is refused outside it.
#
# Two `--`: the first ends the Tauri CLI's own arguments, the second ends
# cargo's, so `--demo` reaches the knobas binary itself. The startup log line
# `profile demo=true` is the acceptance test for that.
demo: deps
    cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri dev -- -- --demo

# The Gitea adapter against the REAL pinned container in `testenv/`.
#
# That container is this adapter's contract source (interfaces §4.2). The
# wiremock stand-in the rest of its suite runs against exists only so
# `just check` and CI stay docker-free (roadmap §3) -- it encodes one person's
# reading of Gitea's API, and `tests/live_gitea.rs` re-asserts every shape it
# encodes against the server that decides them. **If the two disagree, the fake
# is what is wrong.**
#
# Not part of `check`, which is why every test in that file is `#[ignore]`d;
# this recipe is what un-ignores them.
#
# WHICH RECIPE CERTIFIES WHAT. This one runs the DEFAULT compose file, and
# certifies the *shapes* interfaces §4.2 fixes: the key forms, the `{ok,data}`
# envelope on `/repos/search`, `state=all`, the discussion living on the issue
# of the same index, `sort=recentupdate` really ordering newest-first, `since=`
# being server-side and inclusive, and a revoked token's 401. It cannot certify
# what happens when the server answers FEWER records than the adapter asked for
# -- every corpus the seed creates fits in one page of 50 here -- and that is
# what `gitea-live-capped` below is for.
#
# Gitea and its seed only: `testenv/seed` also waits for uptime-kuma and mockd,
# which this suite never touches. Both steps are idempotent, so re-running this
# against an already-seeded environment just runs the tests.
#
# WHAT IT CREATES AND WHAT IT REMOVES. The suite is not read-only: three of its
# tests open a branch through Gitea's own API, two of those a pull request on
# it, and one of those 51 comments. That is deliberate -- exit criterion B is
# about what the real server does with something it has just been told -- and
# all of it is deleted again when each test ends, passing or panicking alike
# (`live_env::Litter`, whose Drop also checks the removal really happened).
# Anything a *killed* run left behind is cleared by the next run before it
# starts. So this recipe is repeatable: measured, the seeded corpus is the same
# shape after twelve runs as before the first, and each run takes the same time
# as the one before it. `testenv/reset` -- which destroys every testenv volume,
# Uptime Kuma's included -- is the deliberate remedy, not the routine one.
# Before issue #143 there was no cleanup: every run left three branches, two
# pull requests and 51 comments behind, and seven runs were enough to make
# `gitea-live-capped` refuse to start.
#
# Serial, and `--test-threads=1` for two reasons now: the runs share one server,
# and the clearing above cannot tell a sibling test's live branch from a corpse.
#
# WHAT DOES NOT NEED THIS RECIPE. The guard's own contracts -- that the
# `knobas-` prefix it deletes by matches nothing `seed-gitea.sh` creates, that
# the leftovers really go, and that a clean repository is left alone -- are
# pinned docker-free in `crates/knobas-source-gitea/tests/litter_guard.rs` and
# run in `just check`.
#
# ONE ENVIRONMENT, ONE OWNER. There is a single Docker environment shared by
# every worktree on the machine, and worktree exclusivity does not cover it:
# `seed-gitea.sh` re-mints the Gitea token when the worktree it runs from has
# no `seed-state.json`, so a second agent seeding mid-run makes the first
# agent's requests answer 401 -- and a `seed-state.json` in a worktree that did
# not do the latest seed holds a token that is already dead. Claim it before you
# run this. testenv/README.md, "One environment, one owner at a time".
gitea-live:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    docker compose up -d --wait gitea
    ./seed-gitea.sh
    eval "$(./seed --env)"
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-gitea --test live_gitea \
      -- --ignored --nocapture --test-threads=1

# The same real container, reconfigured to answer FEWER records than the adapter
# asked for: `testenv/docker-compose.capped.yml` sets Gitea's
# `[api] MAX_RESPONSE_ITEMS` to 1.
#
# WHICH RECIPE CERTIFIES WHAT. This one certifies exactly one property, and the
# one `gitea-live` structurally cannot: that every paged walk survives a server
# capping below the limit it requested (issue #81, live-certified by #115). Its
# suite is `tests/live_gitea_capped.rs` and it is one test; `gitea-live` keeps
# everything else.
#
# THE ORDER MATTERS. The seed runs FIRST, against the uncapped default file,
# because `seed-gitea.sh` decides what already exists by reading listings with
# `limit=50` and no paging -- capped to one record, `commit_exists` sees only a
# branch's newest commit, judges the rest missing, re-POSTs a file that is
# already there and dies on the 422. The overlay then recreates the same
# container over the same volume, so the seeded corpus is still there; the trap
# puts the uncapped container back afterwards, so a later `just gitea-live` is
# not silently running against a capped server.
#
# `INT` and `TERM` as well as `EXIT`, the rule `inventory` above states and
# `check-ports.sh` follows: bash need not run an `EXIT` trap when a signal it
# has no handler for terminates the shell, and this recipe's slow live run is
# one somebody will Ctrl-C. Each signal trap **clears the `EXIT` trap first**,
# then restores, then re-raises -- so the container is uncapped exactly once and
# the recipe still dies of the signal it was sent rather than reporting a tidy
# exit 0. (`inventory` can let both fire because its handler is an idempotent
# `rm -f`; this one is a `docker compose up`.) The trap is belt-and-braces
# either way: the real containment is the `MAX_RESPONSE_ITEMS: "50"` pin in
# docker-compose.yml, which uncaps on the next `docker compose up` whatever
# happened to this shell.
#
# ONE ENVIRONMENT, ONE OWNER -- and this recipe is the harder of the two to
# share. It seeds (so it re-mints the Gitea token, 401ing anyone else mid-run)
# *and* it recreates the shared container capped to one record and back again,
# so a concurrent `just gitea-live` reads a server answering short pages and
# fails for a reason that is not in its own tree. Claim the environment before
# you run this. testenv/README.md, "One environment, one owner at a time".
gitea-live-capped:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    testenv=$PWD
    docker compose up -d --wait gitea
    ./seed-gitea.sh
    eval "$(./seed --env)"
    uncap() { cd "$testenv" && docker compose up -d --wait gitea >/dev/null; }
    trap 'uncap' EXIT
    trap 'trap - EXIT INT; uncap; kill -INT $$' INT
    trap 'trap - EXIT TERM; uncap; kill -TERM $$' TERM
    docker compose -f docker-compose.yml -f docker-compose.capped.yml up -d --wait gitea
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-gitea --test live_gitea_capped \
      -- --ignored --nocapture --test-threads=1

# TeamCity's live certification: the adapter against a **real** TeamCity.
#
# The same contract as `gitea-live` -- the fake is wrong when it disagrees with
# the server -- with one difference that changes what may be written and what
# may be asserted: **there is no container here.** The instance `.env.example`
# points at is JetBrains' public one, it is guest-readable, and it is *not
# ours*. So the suite is strictly read-only, and every assertion in it is by
# form: the corpus changes between one request and the next, so a fixed id, a
# count or a title would be a test that fails for a reason nobody can act on.
#
# No docker and no seed, so this recipe is two lines: load `.env` and un-ignore
# the tests. `.env` is optional -- with no `KNOBAS_TEAMCITY_URL` the suite
# skips, naming the variable, rather than failing. `cp .env.example .env` is
# enough to run it; the default URL needs no token.
#
# Serial and unparallelised on purpose. The rate limiter is per adapter
# instance, so concurrent tests would not share one budget, and the server
# whose budget it is belongs to somebody else.
teamcity-live:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f .env ]; then set -a; . ./.env; set +a; fi
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-teamcity --test live_teamcity \
      -- --ignored --nocapture --test-threads=1

# The TeamCity adapter against the **seeded, self-hosted** TeamCity from
# `./seed --teamcity` (issue #266) -- the sibling `teamcity-live` cannot be:
# a corpus we own, so the suite asserts the seeded content by id, number,
# status and branch, runs the contract battery (clause 2 included), and
# queues one build through REST to watch the `sinceBuild` watermark move and
# then stand still. It deletes what it queued, and clears what a killed run
# left. The header of `tests/live_teamcity_seeded.rs` states the whole
# contract, including what it writes.
#
# The environment is assumed up and seeded, exactly as `gitea-live` obtains
# its variables: `./seed --env` prints KNOBAS_TEAMCITY_URL and
# KNOBAS_TEAMCITY_TOKEN once `./seed --teamcity` has run. It is deliberately
# not brought up here -- the TeamCity profile is opt-in and its seed depends
# on the Gitea seed for its VCS roots -- so a missing token stops with the
# three commands to run. testenv/README.md, "TeamCity, end to end".
#
# Serial and unparallelised for the same reason as `gitea-live`: one server,
# and one test that mutates it. One owner at a time: testenv/README.md, "One
# environment, one owner at a time".
teamcity-live-seeded:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    eval "$(./seed --env)"
    if [ -z "${KNOBAS_TEAMCITY_TOKEN:-}" ]; then
      echo "teamcity-live-seeded: no KNOBAS_TEAMCITY_TOKEN in seed-state.json -- the real" >&2
      echo "  TeamCity is not seeded from this tree. From testenv/:" >&2
      echo "    docker compose --profile real-teamcity up -d teamcity teamcity-agent" >&2
      echo "    ./seed            # Gitea first: the VCS roots point at it" >&2
      echo "    ./seed --teamcity" >&2
      exit 1
    fi
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-teamcity --test live_teamcity_seeded \
      -- --ignored --nocapture --test-threads=1

# The real Jira and Confluence, end to end, inside one licence window: fetch
# the timebomb keys, stand the pair up, walk both setup wizards, seed the
# Tidewater content, prove the seed by reading it back, run every live suite
# gated on the Atlassian URLs, and `down -v` the pair -- from a trap, so the
# teardown runs when a step fails too (issue #275, ADR-0013).
#
# THREE HOURS IS THE WINDOW. The licences are 10-user timebomb keys that
# expire three hours after they are applied (testenv/README.md, "Jira and
# Confluence, end to end"); the pair is disposable by design, which is why
# this recipe ends in `down -v` rather than leaving a seeded environment the
# way `gitea-live` does.
#
# MEASURED on this machine (12 cores, the 8 GB Docker VM), from empty volumes
# with the images already pulled:
#
#   2026-09-03, both products brought up at once (the shape before #314): the
#     pair seeded and verified in about four minutes -- of which the two wizard
#     walks and Jira's final start are about three and the content seed about
#     one (310 key placeholders in and out again) -- the live suites about ten
#     seconds between them once compiled, and `down -v` about ten. This is also
#     the shape that died twice under seven-agent load, on Jira's post-wizard
#     start, before a suite ran (#313).
#
#   <FILL:314 2026-09-03, one product at a time as below>
#
# The suites are seconds, not hours, so the three-hour window holds with well
# over two and a half hours of margin either way; the number to re-measure is
# the one in this header, whenever a suite is added below or the order changes.
#
# WHERE THE LIVE SUITES GO. One line per suite in the block marked below, each
# a `cargo test -p <crate> --test <live suite> -- --ignored --nocapture
# --test-threads=1`, gated on KNOBAS_JIRA_URL / KNOBAS_CONFLUENCE_URL from
# `./seed --env`. Three exist: the Jira adapter's `live_jira_seeded` (#276) --
# sync, payload, cursor, the real 401 -- the app crate's `atlassian_live`,
# which is the engine-and-queue half (credential health end to end, and the
# three write ops read back out of Jira), and the Confluence adapter's
# `live_confluence_seeded` (#284), which is the **only** witness that adapter
# has: ADR-0013 refused a mockd half for a product with no machine-readable
# spec. Serial and unparallelised for the reason `gitea-live` gives: one
# server, and tests that write to it.
#
# THE SUITES SHARE ONE SEEDED SERVER, AND EACH TAKES BACK WHAT IT WROTE, from
# a `Drop` that checks rather than assumes. What a *killed* run leaves is put
# back by the next run of the suite that wrote it, whose leftover clearing
# works from `seed-state.json` rather than from anything a run remembers. On
# the Jira side: a stray issue and a stray comment deleted, a label removed, a
# status moved back through the workflow -- the union of what either Jira
# suite writes. On the Confluence side: every seeded page's title put back,
# which is the whole of what that suite writes (one rename, to witness that a
# renamed page keeps its content id). Order does not matter, and neither does
# a re-run: after a green run
# the server holds exactly the seeded corpus, and a run started against a
# dirty one prints what it put back. What neither suite can undo is the `PAY`
# key counter -- the create leaves the project one key further on -- which is
# why nothing downstream may assume the fixture's keys are the highest ones.
#
# THE 8 GB VM. Jira asks for ~4 GB, Confluence ~2 GB, plus a PostgreSQL each,
# and Docker Desktop's VM here has 8 GB: the pair cannot share it with the
# seeded TeamCity (~2.2 GB for server and agent), so this recipe REFUSES to
# start while `knobas-teamcity` is up and prints the `stop` to run. The same
# 8 GB is why the pair does not comfortably *start* together either -- two JVMs
# claiming their heaps at once is what pushed Jira's post-wizard restart past
# its cap under load -- and why the two products are brought up and seeded one
# at a time below, Jira first. It does
# not stop it itself, because `--profile real-teamcity stop` is somebody's
# seeded environment going away under them (testenv/README.md, "One
# environment, one owner at a time"), and `stop` -- never `down -v` -- is the
# right verb there: the TeamCity volumes hold the seeded builds.
#
# `down -v` NAMES THE FOUR SERVICES. `docker compose --profile real-atlassian
# down -v` with no service named also takes the default profile's containers
# and volumes with it -- Gitea and its seeded corpus included (checked with
# `--dry-run` on Compose v5.1.2). Naming jira, jira-db, confluence and
# confluence-db removes exactly their four volumes and nothing else.
#
# `INT` and `TERM` as well as `EXIT`, the rule `gitea-live-capped` states:
# each signal trap clears the `EXIT` trap first, tears down once, and
# re-raises, so a Ctrl-C mid-wizard still leaves no timebombed pair behind.
atlassian-live:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    testenv=$PWD
    # `{{{{` is just's escape for a literal double brace; the closing pair passes through as is.
    if docker ps --format '{{{{.Names}}' | grep -qx knobas-teamcity; then
      echo "atlassian-live: knobas-teamcity is running, and the Docker VM (8 GB) cannot hold" >&2
      echo "  the Atlassian pair next to it. Stop it -- stop, not down -v: its volumes hold" >&2
      echo "  the seeded builds -- and put it back afterwards:" >&2
      echo "    (cd testenv && docker compose --profile real-teamcity stop teamcity teamcity-agent)" >&2
      echo "    just atlassian-live" >&2
      echo "    (cd testenv && docker compose --profile real-teamcity up -d teamcity teamcity-agent)" >&2
      exit 1
    fi
    t0=$(date +%s)
    # Above both `up`s below, and it has to stay there: Confluence reads its key
    # at first start.
    eval "$(./fetch-timebomb-keys.sh)"
    teardown() {
      cd "$testenv"
      # All four named on every path, including a failure before Confluence was
      # created: `down -v` on a service with no container is a no-op, and the
      # alternative -- tearing down only what got started -- is bookkeeping that
      # would leave a volume behind the first time it was wrong.
      docker compose --profile real-atlassian down -v jira jira-db confluence confluence-db
      echo "atlassian-live: pair torn down; $(( $(date +%s) - t0 ))s wall clock in all"
    }
    trap 'teardown' EXIT
    trap 'trap - EXIT INT; teardown; kill -INT $$' INT
    trap 'trap - EXIT TERM; teardown; kill -TERM $$' TERM
    # ONE PRODUCT AT A TIME, JIRA FIRST. Both JVMs starting together on the 8 GB
    # VM is what stretched Jira's post-wizard restart past its old 300s cap and
    # killed two windows before a suite ran (#313 -> #314). Jira now has the VM
    # to itself until it is RUNNING and its REST answers; Confluence's container
    # does not exist until then. `--wait` holds for each database's healthcheck
    # -- the products have none -- and seed-atlassian.sh is what waits for a
    # state a wizard can be driven from.
    #
    # THE LICENCE FETCH STAYS ABOVE BOTH `up`s. Confluence reads ATL_LICENSE_KEY
    # at first start, so the key has to be in this shell before its container is
    # created -- which is now the second `up`, not the first. Moving the fetch
    # down between the two would still work today and would break the moment the
    # order changed again, so it stays where nothing can get in front of it.
    docker compose --profile real-atlassian up -d --wait jira-db jira
    ./seed-atlassian.sh jira              # waits for FIRST_RUN, walks Jira's wizard
    docker compose --profile real-atlassian up -d --wait confluence-db confluence
    ./seed-atlassian.sh confluence        # ... and Confluence's, on a quiet VM
    ./seed-atlassian-content.sh           # the Tidewater content
    ./seed-atlassian-content.sh --verify  # PAY-231 with its worklogs, one page with its body
    eval "$(./seed --env)"
    echo "atlassian-live: seeded and verified after $(( $(date +%s) - t0 ))s; KNOBAS_JIRA_URL=$KNOBAS_JIRA_URL KNOBAS_CONFLUENCE_URL=$KNOBAS_CONFLUENCE_URL"
    cd ..
    # ---- live suites gated on the Atlassian URLs: one line each, added here ----
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-jira --test live_jira_seeded -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test atlassian_live -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-confluence --test live_confluence_seeded -- --ignored --nocapture --test-threads=1
    echo "atlassian-live: every Atlassian-gated live suite green (3 suites)"
