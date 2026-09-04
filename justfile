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

# Where the three Gitea-backed live recipes get their gate variables, in one
# place because all three obtain them the same way: the lines this text names
# are literally the lines above each guard call. Kept as a variable rather than
# a second private recipe -- what the three share is a message and a variable
# list, not behaviour, and a forwarding recipe would be an indirection with
# nothing in it.
gitea_live_env := "testenv/, which is what the lines above this one do:
  docker compose up -d --wait gitea
  ./seed-gitea.sh
  eval \"$(./seed --env)\""

# THE ALL-SKIPPED RUN. Every live recipe below calls this before it invokes
# cargo, and it refuses the run in which nothing would have run (issue #351).
#
# `just teamcity-live` once reported **"12 passed"** with all twelve of those
# tests skipped: there was no gitignored `.env`, so `KNOBAS_TEAMCITY_URL` was
# unset and `live_or_skip!` returned from each test before it opened a
# connection. A live recipe exists to say "the adapter works against the real
# server". That run said nothing whatsoever and reported success -- into a PR
# body, under the working model's live-run rule, as evidence.
#
# WHY THE GATE VARIABLES AND NOT THE SKIP COUNT. Counting the skips looks like
# the more direct measurement, and it is not available: a `live_or_skip!` skip
# is an early `return`, so libtest counts it as **passed**. The "12 passed"
# above *is* the all-skipped run -- there is no skip count anywhere in cargo's
# output to compare a total against. The only trace is the `SKIP:` line the
# macro prints, which lives in one test file, is pinned by nothing, and is
# visible only for as long as `--nocapture` stays on the command line. A guard
# built on that would be reading a *message* where it means to read a fact, and
# it would stop guarding silently the day somebody reworded the message -- which
# is the same class of error one level up again. The gate variable is the fact:
# it is the single input `live_or_skip!` and every sibling suite's `need()`
# consult, and it is knowable before a test binary is even built.
#
# WHAT THIS DOES NOT CLAIM. A variable that is set is not a server that answers.
# This is the *necessary* condition, checked at the one place where its absence
# is silent: a wrong URL or a dead token already fails loudly and in the suite,
# as a connection error or a 401. The hole being closed is the unset one.
#
# ONE SKIP STAYS LEGAL. Nothing here says anything about an individual test. A
# suite whose gate variables are all present may still skip a test for a reason
# of its own and pass; it is the run where *nothing* ran that is refused.
#
# CALLING IT. `just _require-live-env '<where they come from>' NAME [NAME...]`,
# after the recipe has sourced its variables and while they are exported -- this
# runs as a child process, so an unexported shell variable is invisible to it.
# The first argument is printed verbatim and is what the person who hits this
# actually needs; it may span lines.
#
# It is `{{quote(SOURCE)}}` below rather than `'{{SOURCE}}'`, and the difference
# is not cosmetic: with the plain interpolation, an apostrophe in the text --
# `the recipe's .env` -- closes the shell word early and this script dies of a
# syntax error **on the error path only**. A run with all its variables set
# returns at the line above and never reaches it, so the tree would stay green
# and only the diagnostic this whole recipe exists to print would be gone. Same
# reason for `printf` over `echo`, which would eat a leading `-n`. Callers that
# interpolate a just variable use `{{quote(...)}}` too; a hand-written literal
# in a recipe body is the caller's own shell quoting and fails on every run,
# which is the loud direction.
_require-live-env SOURCE +NAMES:
    #!/usr/bin/env bash
    set -euo pipefail
    missing=
    for name in {{NAMES}}; do
      if [ -z "${!name:-}" ]; then missing="${missing:+$missing }$name"; fi
    done
    [ -n "$missing" ] || exit 0
    echo "error: unset or empty: $missing" >&2
    echo "  Every test in this suite is gated on these. Without them the suite would" >&2
    echo "  skip its way to a green line certifying nothing, so this recipe stops here" >&2
    echo "  instead of reporting a success it has not earned (issue #351)." >&2
    echo >&2
    echo "  Where they come from:" >&2
    printf '%s\n' {{ quote(SOURCE) }} | sed 's/^/    /' >&2
    exit 1

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
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
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
    # Before the overlay, not after: a run refused for a missing variable must
    # not be one that left the shared container capped on its way out.
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
    uncap() { cd "$testenv" && docker compose up -d --wait gitea >/dev/null; }
    trap 'uncap' EXIT
    trap 'trap - EXIT INT; uncap; kill -INT $$' INT
    trap 'trap - EXIT TERM; uncap; kill -TERM $$' TERM
    docker compose -f docker-compose.yml -f docker-compose.capped.yml up -d --wait gitea
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-gitea --test live_gitea_capped \
      -- --ignored --nocapture --test-threads=1

# The start-work flow over the real Gitea: ticket -> branch -> pull request ->
# link -> In Progress, and back again when the pull request is merged (issue
# #44). The suite is `crates/knobas-app/tests/start_work_live.rs` and it is one
# test, deliberately -- what is under test is the round trip.
#
# WHICH RECIPE CERTIFIES WHAT. `gitea-live` above certifies the *adapter*
# against the shapes interfaces §4.2 fixes. This one certifies the *flow* over
# it: the real orchestrator, the real write queue, the real adapters and a real
# database. Its sibling `tests/start_work.rs` proves the same sequence against a
# fake dispatcher and runs inside `just check`; nothing there touches a server.
# Until this recipe existed nothing ran this file at all, and what an unrun
# suite accumulates is what #347 found in it: an assertion no implementation
# could fail.
#
# HALF OF IT IS STILL A MOCK, AND THIS RECIPE IS NOT M2's EXIT CERTIFICATE.
# The repository side is the real container; the *ticket* side is
# `knobas_mockd::spawn_mock_jira()`, in process. ADR-0013 says no mock is a
# witness for an acceptance or exit criterion, so what this run witnesses is
# the branch, the pull request, the draft prefix and the link at a server that
# decides them -- not the two Jira transitions, which are asserted against
# mockd's workflow. Criterion 1 is met end to end only once the ticket side is
# the seeded Jira `atlassian-live` stands up, and that is not this recipe.
#
# WHAT IT WRITES TO GITEA AND WHAT IT TAKES BACK. It cuts a scratch base branch
# `knobas-i44-<pid>-base` from the repository's default branch, opens
# `knobas-i44-<pid>` off it and a draft pull request from that; the draft prefix
# is read back off Gitea's own copy of the title there, which is where story 8's
# claim is witnessed. Then it commits one file on the branch -- a pull request
# with no diff is not one Gitea will merge -- renames it out of draft, and
# **merges** it into the scratch base, because the reverse direction is about a
# pull request somebody actually finished. (The rename is also what makes the
# merge reachable at all: Gitea will not merge a title still carrying the WIP
# prefix. That refusal is never exercised here, since the rename comes first, so
# it is a reason for the step and not a thing this run certifies.)
#
# `Litter`'s `Drop` takes all of it back when the test ends, passing or
# panicking, under the `knobas-` prefix `litter_guard.rs` pins nothing in the
# seed shares: `DELETE /repos/{owner}/{repo}/issues/{index}` for the pull
# request, then both branches, then a re-read of the branch listing, the pull
# listing and the default branch's head to check the server agrees they are
# gone. The pull request goes first because Gitea will not delete a branch an
# open pull request points at.
#
# **The default branch is never written to, and that is a requirement rather
# than tidiness** (issue #373). This recipe and
# `crates/knobas-source-gitea/tests/live_gitea_capped.rs` share one
# `tidewater/payout-service`, and that suite's `HEADROOM` is a budget of 19
# records per listing -- the pull listing it walks, and the commits of every
# branch, the default one included, which the seed leaves at exactly 19. So a
# commit on the default branch turns `just gitea-live-capped` red, and no API
# call takes one back: `DELETE issues/{index}` reclaims a pull request and
# `DELETE branches/{name}` a branch, but a merge commit on the default branch
# needs a force-push or `testenv/reset`. Until #373 this recipe merged there and
# left the merged pull request standing, so every run spent two commits and one
# pull for good; now it merges into the scratch base branch and the guard
# refuses to end a run whose default-branch head moved. Measured over five
# consecutive runs: branches 4 -> 4, pulls 14 -> 14, default-branch commits
# 19 -> 19, root files 10 -> 10.
#
# WHAT DOES NOT COME BACK, and why neither matters. Gitea does not reuse a
# deleted pull request's index, so each run's pull request is a larger integer
# than the last -- but nothing counts indices. `live_gitea_capped.rs` counts
# *records* (`must_be_capped_with_more_behind_it` measures `keys.len()`), and
# the record goes with the `DELETE`. And the nine `knobas-i44-*.txt` files
# already on the default branch are what the old shape left behind; removing
# them means rewriting the seeded history, so they stay. `testenv/reset` is the
# remedy if either ever matters, the same deliberate-not-routine one
# `gitea-live` names.
#
# Serial and unparallelised for `gitea-live`'s reasons: one server, and a test
# that writes to it. ONE ENVIRONMENT, ONE OWNER -- this seeds, so it re-mints
# the Gitea token and 401s anyone else mid-run. Claim it first.
# testenv/README.md, "One environment, one owner at a time".
start-work-live:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    docker compose up -d --wait gitea
    ./seed-gitea.sh
    eval "$(./seed --env)"
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
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
# **A GREEN HERE SAYS NOTHING ABOUT THE SEEDED SERVER, IN EITHER DIRECTION.**
# This recipe reads the **repo-root `.env`** -- not `./seed --env`, not
# `seed-state.json` -- and `.env.example`'s `KNOBAS_TEAMCITY_URL` is
# `https://teamcity.jetbrains.com/guestAuth`, JetBrains' **public** instance.
# So unless somebody has edited their own `.env`, what this certifies is the
# adapter against a server we do not own, whose corpus changes under us. The
# recipe for the container in `testenv/` is `teamcity-live-seeded` below, and
# the two are not substitutes: `./seed --env` printing `KNOBAS_TEAMCITY_URL`
# does not change this recipe's default either (testenv/README.md, "TeamCity,
# with one build agent").
#
# No docker and no seed, so the body is three lines: load `.env`, refuse a run
# with nothing to run against, un-ignore the tests. `cp .env.example .env` is
# enough; the default URL needs no token, which is why only the URL is gated.
#
# `.env` USED TO BE OPTIONAL, and that is the bug this guard closes. With no
# `.env` the suite skipped every test by name and libtest counted the skips as
# passes: **"12 passed", nothing run, exit 0** (issue #351, found by #347's live
# window). See `_require-live-env` above for why the variable is what is
# checked rather than the skips.
#
# Serial and unparallelised on purpose. The rate limiter is per adapter
# instance, so concurrent tests would not share one budget, and the server
# whose budget it is belongs to somebody else.
teamcity-live:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f .env ]; then set -a; . ./.env; set +a; fi
    just _require-live-env 'the repo-root .env, which is gitignored:
      cp .env.example .env
    Its default URL is the public JetBrains guest instance and needs no token.' \
      KNOBAS_TEAMCITY_URL
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
# on the Gitea seed for its VCS roots -- so a missing URL or token stops the
# recipe, through the shared `_require-live-env` guard above, with the three
# commands to run. testenv/README.md, "TeamCity, end to end".
#
# TWO SUITES, in two crates, and the second is not the adapter's (issue #389).
# `knobas-app`'s `teamcity_seeded_live` is M3.3's exit criterion for this
# source: a seeded build the mirror attributes to the reader is on the standup
# digest for the day it ran. The digest is an app-crate read, so `live_teamcity_seeded.rs`
# -- in the adapter's crate -- cannot make that assertion, and a live suite with
# no recipe is one nothing runs (testenv/README.md). It is strictly read-only
# and runs second, so it disturbs neither the exact-set assertions nor the
# leftover clearing of the suite above it.
#
# Serial and unparallelised for the same reason as `gitea-live`: one server,
# and one test that mutates it. One owner at a time: testenv/README.md, "One
# environment, one owner at a time".
teamcity-live-seeded:
    #!/usr/bin/env bash
    set -euo pipefail
    cd testenv
    eval "$(./seed --env)"
    just _require-live-env 'seed-state.json, via eval "$(./seed --env)". No entry
    there means the real TeamCity is not seeded from this tree. From testenv/:
      docker compose --profile real-teamcity up -d teamcity teamcity-agent
      ./seed            # Gitea first: the VCS roots point at it
      ./seed --teamcity' \
      KNOBAS_TEAMCITY_URL KNOBAS_TEAMCITY_TOKEN
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-teamcity --test live_teamcity_seeded \
      -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test teamcity_seeded_live \
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
#   2026-09-03, one product at a time as below: **392 s in all**, with TeamCity
#     stopped and the machine otherwise busy. Seeded and verified at 317 s, of
#     which Jira's half is about 160 (56 s to FIRST_RUN, 53 s more before it
#     served its first wizard step, four POSTs, and a post-wizard restart that
#     had already finished by the time the wait for it began), Confluence's
#     about 40 (FIRST_RUN 9 s after `up`, on a VM Jira had just stopped
#     competing for), and the content seed the rest. The three suites, 26 tests,
#     ran in 14 s once compiled; the teardown took the balance.
#
#   2026-09-03, the same shape re-run on the merged bytes, TeamCity stopped and
#     two other agents building: **327 s in all**, seeded and verified at 265,
#     the three suites (28 tests by then) in 22. Same run, the two numbers this
#     ticket is about: Jira served its wizard 46 s after answering FIRST_RUN --
#     one progress line, `jira is serving no form yet -- 30s of 600s`, printed
#     inside that gap -- and the post-wizard wait for RUNNING returned in 0 s
#     again. Full runs sat between 311 s and 392 s when this was written.
#     The floor has since moved DOWN rather than up: 295 s on 2026-09-04,
#     four suites and 39 tests by then, on a machine with nothing else
#     building (#332's certification run). These numbers measure the
#     machine as much as the recipe, and the direction the caps below care
#     about is the slow one, which nothing has moved.
#
# The suites are seconds, not hours, so the three-hour window holds with well
# over two and a half hours of margin either way; the number to re-measure is
# the one in this header, whenever a suite is added below or the order changes.
#
# THE SEQUENCING IS THE FIX; THE WIDER CAP IN seed-atlassian.sh IS INSURANCE.
# No run since has come near even the old 300 s cap -- 292 s, 399 s, 480 s, and
# the 392 s and 327 s ones above, all with TeamCity stopped -- and what has kept
# that cap survivable is the refusal below to start while TeamCity is up. The
# pair sharing the 8 GB VM with a third JVM is the case that killed #313 and
# the case nobody has measured.
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
    ./seed-atlassian-content.sh --verify  # PAY-231 and one page, read back with its ancestors
    eval "$(./seed --env)"
    # `./seed --env` prints the Atlassian block only once seed-state.json has a
    # `jira`/`confluence` entry, so an unseeded state file yields four suites
    # that would each stop on their own `need()` -- or, the day one of them
    # grows a skip, would not. Refused here, once, for all four.
    just _require-live-env 'seed-state.json, via eval "$(./seed --env)", once
    seed-atlassian.sh and seed-atlassian-content.sh have run -- which is what
    the lines above this one in this recipe do.' \
      KNOBAS_JIRA_URL KNOBAS_JIRA_USER KNOBAS_JIRA_PASSWORD \
      KNOBAS_CONFLUENCE_URL KNOBAS_CONFLUENCE_USER KNOBAS_CONFLUENCE_PASSWORD
    echo "atlassian-live: seeded and verified after $(( $(date +%s) - t0 ))s; KNOBAS_JIRA_URL=$KNOBAS_JIRA_URL KNOBAS_CONFLUENCE_URL=$KNOBAS_CONFLUENCE_URL"
    cd ..
    # ---- live suites gated on the Atlassian URLs: one line each, added here ----
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-jira --test live_jira_seeded -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test atlassian_live -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-confluence --test live_confluence_seeded -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test confluence_live -- --ignored --nocapture --test-threads=1
    echo "atlassian-live: every Atlassian-gated live suite green (4 suites)"
