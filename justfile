# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# `front` runs *beside* the cargo chain, not before it. Neither writes what
# the other reads: svelte-check, vitest and `vite build` never touch
# `target/`, and what `front` produces (`app/node_modules`, `app/dist`) no
# cargo recipe reads -- the mirror tests `include_str!` files under `app/src`,
# which `front` only reads too. `just` runs a recipe's dependencies one after
# another, so as a dependency `front` sat in front of the compile and its 24 s
# were paid in full on every gate (measured 2026-09-05, warm, idle: `front`
# 24 s, the cargo chain 178 s, `just check` 202 s). Started together, `front`
# is finished long before `test` is, and the gate is the cargo half's length.
# `just` has a `[parallel]` attribute that would start the dependencies at
# once, but it interleaves their output line by line and reports nothing per
# half; the point of capturing each half to its own file is that a red still
# reads as one recipe's output.
#
# Each half writes to a per-invocation `mktemp` file (the inventory recipe
# below says why a shared path is not an option on a machine with several
# worktrees on it). A half that passes is reported as its summary; a half
# that fails is reported as its captured output in full, and both are when
# both fail -- running them together must not cost a signal the serial run
# had. Neither half is cut short by the other failing, for the same reason,
# and the exit status is non-zero if either half's was. `front` still goes
# through `deps`: a missing `app/node_modules` is installed, and if that
# cannot happen the frontend half is a red, not a skip.
#
# The traps are the inventory recipe's: `INT` and `TERM` stop whatever is
# left of both halves and remove the scratch files before re-raising, so a
# cancelled gate leaves no cargo or vitest running under nobody and nothing in
# `$TMPDIR`. The kill is explicit because the halves cannot be relied on to
# get the signal themselves. Without job control, bash starts a `&` job with
# `SIGINT` ignored, so the two subshells below survive a Ctrl-C; and a signal
# aimed at this shell alone (a tool's timeout, a `kill` by pid) reaches
# nothing below it. `just` does not pass the ignore on -- its children start
# with `INT` at the default -- so a terminal Ctrl-C does end the cargo and
# node processes under it; the trap is what covers the subshells and the
# single-pid case. (Checked on bash 3.2 and just 1.58: a background subshell
# survived an `INT` and died of a `TERM`, which is why the kill sends `TERM`;
# a `sleep` under a background `just` died of an `INT`.) Descendants are
# killed deepest first so a `just` in the chain cannot start its next recipe
# after its current one dies.
#
# `front` is *not* a build dependency of the cargo recipes --
# `tauri::generate_context!` only reads `frontendDist` when the
# `custom-protocol` feature is on (production, i.e. `tauri build`); a dev build
# with `devUrl` set takes the empty default asset set and never looks at
# `app/dist`. Nothing here may create that directory either: a missing one is
# exactly how `tauri build` refuses to bundle an app with no frontend in it.
check:
    #!/usr/bin/env bash
    set -euo pipefail
    front_log= cargo_log= front_pid= cargo_pid=
    cargo_recipes=(fmt shell test-layout witness-unit clippy clippy-libs inventory test)
    descendants() {
        local p
        for p in $(pgrep -P "$1" || true); do descendants "$p"; done
        echo "$1"
    }
    stop_halves() {
        local p pids=
        for p in $front_pid $cargo_pid; do pids="$pids $(descendants "$p")"; done
        [ -z "${pids// /}" ] || kill -TERM $pids 2>/dev/null || true
        wait
    }
    cleanup() { rm -f "$front_log" "$cargo_log"; }
    # Traps first, so a second `mktemp` failing cannot leave the first behind.
    trap 'cleanup' EXIT
    trap 'stop_halves; cleanup; trap - INT; kill -INT $$' INT
    trap 'stop_halves; cleanup; trap - TERM; kill -TERM $$' TERM
    front_log=$(mktemp "${TMPDIR:-/tmp}/knobas-check-front.XXXXXX")
    cargo_log=$(mktemp "${TMPDIR:-/tmp}/knobas-check-cargo.XXXXXX")

    # Starts `just RECIPE...` in the background with everything it prints in
    # LOG, ending with the half's own exit status and wall time; the caller
    # takes the pid from `$!`.
    run_half() {
        local name=$1 log=$2
        shift 2
        ( SECONDS=0; status=0; just "$@" || status=$?
          echo "$name: exit $status after ${SECONDS} s"; exit "$status" ) >"$log" 2>&1 &
    }

    echo "check: running 'front' beside '${cargo_recipes[*]}';"
    echo "check: each half's output is captured and reported when it finishes."
    run_half front "$front_log" front; front_pid=$!
    run_half cargo "$cargo_log" "${cargo_recipes[@]}"; cargo_pid=$!

    # A passing half is one line of status plus the lines a reader would look
    # for anyway. For `front`: svelte-check's count -- `svelte-check found N
    # errors` as a rule, the machine-format `COMPLETED N FILES` line when
    # svelte-check sees CLAUDECODE=1 in the environment (4.7.6 chooses its
    # output by that variable, not by whether stdout is a terminal) -- then
    # vitest's totals and the vite build; for the cargo chain: `test`'s
    # per-binary lines (`test: ok <package> <target> N passed; N failed; N
    # ignored; ...`, one per test binary and one per doc-test crate) added
    # up here, independently of the total `test` prints itself, so the two
    # can be compared. Two spaces after the verdict: the per-binary lines are
    # padded there and the recipe's own total lines (`test: ok: ...`,
    # `test: FAILED (...)`) are not, so a total cannot be counted as a binary.
    # A failing half is its whole log, so nothing has to be re-run to see why.
    report() {
        local name=$1 log=$2 status=$3
        if [ "$status" -eq 0 ]; then
            echo "check: $(tail -n 1 "$log")"
            case $name in
                front) grep -E 'svelte-check found|COMPLETED [0-9]+ FILES|Test Files|^ *Tests |built in' "$log" || tail -n 5 "$log" ;;
                cargo) awk '/^test: (ok|FAILED)  / { n++; for (i = 1; i < NF; i++) {
                             if ($(i+1) == "passed;") p += $i
                             if ($(i+1) == "failed;") f += $i
                             if ($(i+1) == "ignored;") g += $i } }
                           END { printf "  %d test binaries and doc-test crates: %d passed, %d failed, %d ignored\n", n, p, f, g }' "$log" ;;
            esac
        else
            echo "check: $name FAILED; its full output follows" >&2
            echo "----- $name -----" >&2
            cat "$log" >&2
            echo "----- end of $name -----" >&2
        fi
    }

    # A reaped pid is cleared at once: a signal after that must not aim
    # `stop_halves` at whatever process has since been given the number.
    front_status=0; wait "$front_pid" || front_status=$?
    front_pid=
    report front "$front_log" "$front_status"
    cargo_status=0; wait "$cargo_pid" || cargo_status=$?
    cargo_pid=
    report cargo "$cargo_log" "$cargo_status"

    if [ "$front_status" -ne 0 ] || [ "$cargo_status" -ne 0 ]; then
        echo "check: FAILED (front exit $front_status, cargo exit $cargo_status)" >&2
        exit 1
    fi
    echo "check: ok in ${SECONDS} s"

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
# The binaries come from `_test-executables` below (one build, one
# enumeration, shared with `test`); asking each binary to `--list` itself is
# what makes the result the *harness's* answer rather than a guess parsed out
# of the source. `--list` enumerates, it does not execute, so nothing here
# starts a database. LC_ALL=C keeps the sort byte-wise, so the file does not
# churn when a machine's locale differs.
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
    just _test-executables \
      | awk -F'\t' '$1 == "test"' \
      | while IFS=$'\t' read -r _ pkg target exe _; do
            "$exe" --list 2>/dev/null | sed -n 's/: test$//p' | sed "s|^|${pkg}\t${target}\t|"
        done \
      | grep -vxF -f <(sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' test-inventory-conditional.txt) \
      | LC_ALL=C sort > "$tmp"
    mv "$tmp" "{{FILE}}"

# Build the workspace's test executables and name them, one per line:
#
#     test    <package>  <kind>/<target>        <executable>  <package dir>
#     server  knobas-db  bin/knobas-test-server <executable>  <package dir>
#
# tab-separated. The package directory is the binary's working directory
# under `cargo test` (cargo runs each test binary from its package root and
# sets the `CARGO_*` runtime variables there); `test` runs every binary from
# the same place so a test that reads a path relative to its crate behaves
# identically under both. Nothing in the tree does today (checked 2026-09-05:
# no runtime `CARGO_*` read, no relative filesystem path in a test), which is
# why it is a column here and not a workaround anywhere else.
#
# `inventory` and `test` both consume this: one enumeration expression, kept
# in one place, so the two recipes cannot drift apart on
# what a test executable is (before #415 each carried its own copy). Within a
# recipe it is the only cargo call before the binaries run -- `test` used to
# build once and then enumerate in a second `--no-run`. Across recipes it
# still runs once each; the second run on a warm tree is a no-op build and
# under a second.
#
# `--no-run` builds; `--message-format=json-render-diagnostics` names what it
# built on stdout and still renders compiler diagnostics on stderr the way a
# plain `cargo test` does, so a broken tree reads as a compile error here and
# not as "no executables" further down. `-q` silences cargo's own progress
# lines and nothing else.
#
# The `test` lines are every executable built with the test harness (the
# `.profile.test` flag: `tests/*.rs` targets, each crate's unit tests, and a
# bin's unit tests). The `server` line is `knobas-test-server`, the
# `test-util`-only bin `test` starts one Postgres from; it is built by the same
# invocation because `knobas-db`'s self-dev-dependency turns the feature on,
# and it is not a `test` line because its `Cargo.toml` says `test = false`.
#
# The package name comes from `package_id`, which cargo spells two ways
# depending on version (`<path>#<version>` and `<path>#<name>@<version>`); both
# are handled.
_test-executables:
    #!/usr/bin/env bash
    set -euo pipefail
    env -u RUSTUP_TOOLCHAIN cargo test -q --workspace --no-run --message-format=json-render-diagnostics \
      | jq -r 'select(.executable != null)
               | (.package_id | if test("#.*@") then (split("#")[1] | split("@")[0])
                                else (split("#")[0] | split("/") | last) end) as $pkg
               | (.manifest_path | split("/")[:-1] | join("/")) as $dir
               | if .profile.test == true then
                   "test\t\($pkg)\t\(.target.kind[0])/\(.target.name)\t\(.executable)\t\($dir)"
                 elif .target.kind == ["bin"] and .target.name == "knobas-test-server" then
                   "server\t\($pkg)\tbin/\(.target.name)\t\(.executable)\t\($dir)"
                 else empty end'

# shellcheck over every tracked script in testenv/ and scripts/.
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
    done < <(git ls-files -- testenv scripts)

    if [ ${#scripts[@]} -eq 0 ]; then
        echo "error: shebang discovery matched no scripts under testenv/ or scripts/ --" >&2
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

# The desktop witness's own unit tests (issue #500).
#
# `desktop-witness` itself cannot be in the gate: it takes over the screen for
# minutes, and it needs a code-signing identity and a TCC grant that a fresh
# checkout does not have. What can be in the gate is the part of that harness
# which is a decision rather than a side effect -- how it compares Launch
# Services' answer against the bundle it built, and how it reads the
# accessibility probe's three lines. Both are pure text and both have been
# wrong in this shape before. Most of the suite needs nothing but bash; its
# last four checks compile `ax.swift` and ask the helper for the probe and for
# a registered path, and skip themselves where macOS and `swiftc` are not both
# present.
#
# The test-layout script's own tests (issue #573) ride here too: they are bash
# against fixture trees, need nothing installed, and take under a second.
witness-unit:
    testenv/desktop-witness-test.sh
    scripts/test-layout-test.sh

# The integration-test layout guard of ADR-0017 (issue #573): a top-level
# `crates/<crate>/tests/*.rs` must be on `test-layout-exceptions.txt`, with its
# reason, and every entry there must name a file that exists. Everything else is
# a module of the crate's one binary, `tests/it/main.rs`. Files are found on
# disk, not with `git ls-files`, because cargo compiles an untracked one too.
test-layout:
    scripts/test-layout.sh guard

# The rename check a crate's merge PR runs (issue #573): the working tree's
# `test-inventory.txt` must be BASE's with every test of CRATES' merged files
# moved from `test/<file>` to `test/it` as `<file>::<name>`, byte for byte, and
# nothing else changed. Run `just inventory-update` first.
#
#     just inventory-rename knobas-search
#
# BASE defaults to the merge-base with `main`, not `main` itself: a sibling PR
# that lands while this one is open changes `main`'s inventory, and those lines
# are not this merge's. On a branch rebased onto `main` the two are the same.
inventory-rename +CRATES:
    #!/usr/bin/env bash
    set -euo pipefail
    base=${BASE:-$(git merge-base HEAD main)}
    old=$(mktemp "${TMPDIR:-/tmp}/knobas-inventory-base.XXXXXX")
    trap 'rm -f "$old"' EXIT
    trap 'rm -f "$old"; trap - INT; kill -INT $$' INT
    trap 'rm -f "$old"; trap - TERM; kill -TERM $$' TERM
    git show "$base:test-inventory.txt" >"$old"
    echo "inventory-rename: old inventory from $(git rev-parse --short "$base")"
    scripts/test-layout.sh rename "$old" test-inventory.txt {{CRATES}}

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

# One embedded PostgreSQL for the whole run. Every test binary that uses
# `knobas-db`'s shared test connector reaches it through `KNOBAS_TEST_DB_URL`
# and gets a database of its own on it, instead of running `initdb` and
# starting a postmaster of its own -- which sixty-odd binaries used to do, one
# after another, and which is where two overlapping gates ran macOS out of
# SysV shared-memory segments (#249). With the variable unset, as in a
# `cargo test -p <crate>` by hand, each binary still brings up its own server.
#
# The server is `knobas-test-server`, a `test-util`-only bin in `knobas-db`
# (see its `Cargo.toml`). `_test-executables` builds it together with the test
# binaries and names both; nothing below runs cargo again except the doc-test
# run, which rebuilds nothing.
#
# The server lives as long as its stdin is open: fd 3 here is the write end of
# a fifo it reads, and closing that fd is the shutdown. The traps close it on
# EXIT and, because bash runs no EXIT trap when a signal it has no handler for
# kills the shell, on INT and TERM too -- each restoring the default
# disposition and re-raising, as `inventory` does -- so a cancelled gate leaves
# no postmaster behind. The server also stops itself on SIGINT/SIGTERM, which a
# Ctrl-C delivers to it directly; either route ends in the same `pg_ctl stop`.
#
# A fifo rather than `coproc`: `/bin/bash` on macOS is 3.2, which has none.
#
# THE BINARIES RUN IN PARALLEL, not one after another as `cargo test` runs
# them. 112 binaries, 98 of them under 3 s each, still added up to 71 s of the
# serial gate; through `xargs -P`, with each binary's stdout and stderr
# captured to its own file under the gate directory, the run is as long as
# its slowest binary plus the queue behind it. Each worker always exits 0 and
# writes the binary's real status to a file beside its log, because xargs
# stops feeding on an exit of 255 and reports 123 for anything else non-zero,
# and neither tells which binary it was; a red must never cut the others
# short or hide behind them. Doc-tests (`cargo test --workspace --doc`, 14
# short binaries, not in the inventory because cargo builds no executable for
# them) run alongside the pool with their output captured the same way.
# Binaries run without `--ignored`, so the live suites stay ignored.
#
# JOBS. `N` is bounded to the core count (`getconf _NPROCESSORS_ONLN`, which
# both macOS and Linux answer) and chosen by measurement on the 12-core
# machine, warm tree, idle, 2026-09-05: the numbers are in the recipe beside
# the constant. `KNOBAS_TEST_JOBS` overrides it, for measuring another value
# without editing this file.
#
# WHAT IS PRINTED. A passing binary is one line -- package, target, libtest's
# `N passed; N failed; N ignored;` and its `finished in` time -- printed as
# it finishes, so a run shows progress and a reader sees which binary is
# slow. A failing binary's captured output is printed in full once the pool
# has drained, between `----- <package> <target> -----` markers, so nothing
# has to be re-run to see why. The last line is the total, and `check` adds
# up the per-binary lines itself as a cross-check on it. Exit status is 1 if
# any binary's status was non-zero, or the doc-test run's, or xargs's own,
# or if libtest's `failed` counts added up over every log are -- the last
# is the harness's word and the one signal a broken status file cannot fake.
#
# THE WORKER runs under `xargs` as `bash -c`, so it is an exported function
# rather than a script file; it finds its inputs through `KNOBAS_GATE_DIR`
# because xargs hands it nothing but the line number. `3>&-` on the pool and
# on the doc-test run: xargs, every test binary and every postmaster a
# lifecycle test starts would otherwise inherit the fifo's write end, and the
# server sees EOF only when the *last* holder is gone -- a postmaster a
# failing test left behind would turn a red gate into one that never returns.
#
# THE POOL RUNS AS A BACKGROUND JOB and the shell `wait`s on it, for the reason
# `check` gives: bash runs a trap only after the foreground command returns,
# so with `xargs` in the foreground a signal aimed at this shell alone would
# do nothing until the whole run had finished. Background jobs start with
# `INT` ignored, which the binaries inherit, so the traps kill the pool's
# descendants explicitly with `TERM` (deepest first, so xargs cannot start
# the next binary after its current one dies) before closing the pipe.
#
# GATE SLOTS. At most two gate-sized recipes run on one machine at a time. The
# protocol that enforces it lives in `scripts/gate-slot.sh`, sourced below,
# with the measurement, the whole reasoning, and the list of what sources it in
# its header -- one place, so a third gate is one edit. In short: one of `KNOBAS_GATE_SLOTS` (default 2)
# symlinks under `$TMPDIR/knobas-gate-slots` is taken before this recipe does
# anything else, a third gate prints one line naming the holders and polls up
# to `KNOBAS_GATE_SLOT_WAIT` seconds, and the slot is released by the same
# EXIT, INT and TERM traps that close the pipe. The ceiling is SysV shared
# memory, not CPU: one gate peaks at 13 of macOS's 32 segments.
#
# Portability: plain bash 3.2, jq, xargs with `-P`/`-I` (both BSD and GNU),
# pgrep, getconf, `ln -s`, `readlink` and `find -mmin`. `check.yml` on
# `ubuntu-latest` runs the same recipe.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    # Not `knobas-test-...`: that prefix names the scratch roots the test
    # connector's reaper sweeps, and a directory so named with no lock beside
    # it is deleted from under this recipe by the first binary that starts.
    # The traps are armed before anything they clean up exists, so an
    # interrupt between `mktemp` and the server's start leaks nothing.
    gate= server_pid= pool_pid= doc_pid= closed=
    # `slot`, `release_slot` and `hold_gate_slot`, shared with `search-perf`
    # so there is one implementation of the slot protocol. Sourced before the
    # traps below, which call `release_slot`.
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    descendants() {
        local p
        for p in $(pgrep -P "$1" || true); do descendants "$p"; done
        echo "$1"
    }
    stop_run() {
        local p pids=
        for p in $pool_pid $doc_pid; do pids="$pids $(descendants "$p")"; done
        [ -z "${pids// /}" ] || kill -TERM $pids 2>/dev/null || true
        # Not a bare `wait`: that would also wait on the server, which only
        # exits once `close_pipe` has closed its stdin.
        for p in $pool_pid $doc_pid; do wait "$p" 2>/dev/null || true; done
    }
    close_pipe() {
        [ -z "$closed" ] || return 0
        closed=1
        exec 3>&-
        if [ -n "$server_pid" ]; then
            wait "$server_pid" || echo "warning: the test server exited with status $?" >&2
        fi
        [ -z "$gate" ] || rm -rf "$gate"
    }
    trap 'close_pipe; release_slot' EXIT
    trap 'stop_run; close_pipe; release_slot; trap - INT; kill -INT $$' INT
    trap 'stop_run; close_pipe; release_slot; trap - TERM; kill -TERM $$' TERM

    hold_gate_slot test

    gate=$(mktemp -d "${TMPDIR:-/tmp}/knobas-gate.XXXXXX")
    just _test-executables > "$gate/executables"
    awk -F'\t' '$1 == "test"' "$gate/executables" > "$gate/tests"
    server=$(awk -F'\t' '$1 == "server" { print $4 }' "$gate/executables")
    count=$(wc -l < "$gate/tests" | tr -d ' ')
    if [ -z "$server" ]; then
        echo "error: cargo built no knobas-test-server; is knobas-db's test-util bin still declared?" >&2
        exit 1
    fi
    if [ "$count" -eq 0 ]; then
        echo "error: cargo built no test executables" >&2
        exit 1
    fi

    mkfifo "$gate/stdin"
    "$server" < "$gate/stdin" > "$gate/url" &
    server_pid=$!
    exec 3> "$gate/stdin"

    # `read` fails on a line with no newline yet, so this waits for the whole
    # URL; the file itself appears a moment after the fifo is opened, hence the
    # silenced stderr -- redirected *before* the `<`, because bash applies
    # redirections left to right and reports a failed open on whatever stderr
    # is at that moment.
    until read -r url 2>/dev/null < "$gate/url" && [ -n "$url" ]; do
        if ! kill -0 "$server_pid" 2>/dev/null; then
            echo "error: the test server exited before printing a URL" >&2
            exit 1
        fi
        sleep 0.1
    done
    export KNOBAS_TEST_DB_URL="$url"
    export KNOBAS_GATE_DIR="$gate"

    # N = 6. Measured 2026-09-05 on the 12-core machine, warm tree, nothing
    # else running, this recipe, from its start to its last binary (the
    # server's teardown afterwards used to add 10-12 s at every N -- `pg_ctl
    # stop -m fast` plus the removal of a data directory grown to 355
    # databases and 4 GB -- so `just test` end to end was 46-49 s at N=6;
    # since #421 the server stops in immediate mode and leaves the removal to
    # an `rm` that outlives it, and the teardown is under half a second):
    #   N=12: 43 s, 42 s   N=8: 36 s, 38 s   N=6: 38 s, 34 s, 37 s   N=4: 36 s
    # From 12 down to 8 the run gets 5 s shorter and below 8 it stops moving.
    # The pool is CPU-bound, not queue-bound: every binary's libtest runs one
    # test thread per core, so 6 binaries already offer 72 runnable threads
    # to 12 cores, and the sum of libtest's own times inflates from 82 s
    # (serial) to 128/180/245/380 s at N=4/6/8/12 -- pure contention. 6 is
    # the middle of the flat region: half the cores, the 1-minute load
    # average near 12 rather than the 18 N=12 reaches, and a connection peak
    # of 82 client backends against the server's 400 (N=12 peaked at 143).
    jobs=${KNOBAS_TEST_JOBS:-6}
    cores=$(getconf _NPROCESSORS_ONLN)
    case $jobs in
        ''|*[!0-9]*) echo "error: KNOBAS_TEST_JOBS must be a whole number of at least 1, not '$jobs'" >&2; exit 1 ;;
    esac
    # Through arithmetic, not compared as a string: `00` is all digits and
    # would otherwise reach xargs as `-P 00`, which it reads as 0 -- no limit,
    # the one value this bound exists to keep out.
    jobs=$((10#$jobs))
    if [ "$jobs" -lt 1 ]; then
        echo "error: KNOBAS_TEST_JOBS must be at least 1 (0 would be xargs's 'no limit')" >&2; exit 1
    fi
    [ "$jobs" -le "$cores" ] || jobs=$cores

    # One binary: run it with everything it prints in `out/<n>.log`, its exit
    # status in `out/<n>.status`, and one line on stdout. The summary is the
    # binary's own `test result:` line reduced to its counts and time.
    run_one() {
        local n=$1 pkg target exe dir status=0 summary
        local log="$KNOBAS_GATE_DIR/out/$n.log"
        IFS=$'\t' read -r _ pkg target exe dir < <(sed -n "${n}p" "$KNOBAS_GATE_DIR/tests")
        # `</dev/null`: GNU xargs gives a child /dev/null for stdin, BSD xargs
        # hands it its own, which here is the queue of job numbers -- a test
        # that read stdin would eat the queue on macOS and not on Linux.
        ( cd "$dir" && exec "$exe" </dev/null ) > "$log" 2>&1 || status=$?
        echo "$status" > "$KNOBAS_GATE_DIR/out/$n.status"
        summary=$(sed -n 's/^test result: [A-Za-z]*\. \([0-9]* passed; [0-9]* failed; [0-9]* ignored;\).*finished in \([0-9.]*s\)$/\1 \2/p' "$log" | tail -n 1)
        if [ "$status" -eq 0 ] && [ -n "$summary" ]; then
            printf 'test: ok      %-24s %-40s %s\n' "$pkg" "$target" "$summary"
        else
            printf 'test: FAILED  %-24s %-40s %s (exit %s)\n' "$pkg" "$target" "${summary:-no test result line}" "$status"
        fi
    }
    export -f run_one
    mkdir "$gate/out"

    echo "test: $count binaries, $jobs at a time, doc-tests alongside; one Postgres for all of them"
    ( awk '{ print NR }' "$gate/tests" | xargs -P "$jobs" -I{} bash -c 'run_one "$1"' _ {} ) 3>&- &
    pool_pid=$!
    ( env -u RUSTUP_TOOLCHAIN cargo test --workspace --doc ) > "$gate/doc.log" 2>&1 3>&- &
    doc_pid=$!

    # A reaped pid is cleared at once, so a later signal cannot aim `stop_run`
    # at whatever process has since been given the number.
    pool_status=0; wait "$pool_pid" || pool_status=$?
    pool_pid=
    doc_status=0; wait "$doc_pid" || doc_status=$?
    doc_pid=

    # Doc-tests: one line per crate on success, in the same shape as the
    # binaries' lines so `check` adds them up the same way; the whole log on
    # failure.
    if [ "$doc_status" -eq 0 ]; then
        awk '/^ +Doc-tests / { crate = $2 }
             /^test result:/ { sub(/^test result: [A-Za-z]*\. /, ""); sub(/ 0 measured.*finished in /, " ")
                               printf "test: ok      %-24s %-40s %s\n", crate, "doc-tests", $0 }' "$gate/doc.log"
    else
        echo "test: doc-tests FAILED (exit $doc_status); their full output follows" >&2
        echo "----- doc-tests -----" >&2
        cat "$gate/doc.log" >&2
        echo "----- end of doc-tests -----" >&2
    fi

    # Every binary the pool did not report a 0 for, in list order, in full.
    failed=0
    n=0
    while IFS=$'\t' read -r _ pkg target exe _; do
        n=$((n + 1))
        status=$(cat "$gate/out/$n.status" 2>/dev/null || echo "not run")
        [ "$status" = 0 ] && grep -q '^test result:' "$gate/out/$n.log" && continue
        failed=$((failed + 1))
        echo "test: $pkg $target FAILED (exit $status); its full output follows" >&2
        echo "----- $pkg $target -----" >&2
        [ ! -f "$gate/out/$n.log" ] || cat "$gate/out/$n.log" >&2
        echo "----- end of $pkg $target -----" >&2
    done < "$gate/tests"

    doc_crates=$(grep -c '^ *Doc-tests ' "$gate/doc.log" || true)
    # `|| true` on the cat: if no worker ever ran, `out/*.log` matches nothing
    # and the recipe must still reach its FAILED line rather than die here.
    IFS=$'\t' read -r failed_tests totals < <(
        (cat "$gate"/out/*.log "$gate/doc.log" 2>/dev/null || true) \
          | awk '/^test result:/ { for (i = 1; i < NF; i++) {
                     if ($(i+1) == "passed;") p += $i
                     if ($(i+1) == "failed;") f += $i
                     if ($(i+1) == "ignored;") g += $i } }
                 END { printf "%d\t%d passed, %d failed, %d ignored\n", f, p, f, g }')
    # Red on any binary's status, on the doc-test run's, on xargs's own, and
    # on libtest's failed count added up over every log: the last is the
    # harness's word rather than the runner's and is the one signal a broken
    # status file could not fake.
    # The pipe is closed here rather than left to the EXIT trap so that the
    # time on the last line is the whole run, the server's stop included.
    close_pipe
    if [ "$failed" -ne 0 ] || [ "$doc_status" -ne 0 ] || [ "$pool_status" -ne 0 ] || [ "$failed_tests" -ne 0 ]; then
        echo "test: FAILED ($failed of $count binaries, doc-tests exit $doc_status, pool exit $pool_status): $totals; in ${SECONDS} s" >&2
        exit 1
    fi
    echo "test: ok: $count binaries and $doc_crates doc-test crates, $totals, in ${SECONDS} s"

# The launcher's perf gate (#556). Beside `just check`, never inside it.
#
# WHAT IT RUNS. The five `#[ignore]`d tests in
# `crates/knobas-search/tests/perf.rs` and the sixth of the same class in
# `tests/coverage.rs` (`the_author_probe_stays_inside_the_launchers_budget`,
# which that file marks *"`#[ignore]`d for the same reason `tests/perf.rs`
# is"*). One `cargo test` over both targets, which cargo runs one after the
# other, `--test-threads=1` so no two of them are timing each other. Until
# this recipe existed the file was run by a human typing its doc comment's
# cargo line, and `just check` cannot run it: binaries there run without
# `--ignored`.
#
# WHY NOT IN `check`. Every assertion in `perf.rs` is wall-clock against a
# fixed budget, `just check` runs two at a time beside other agents' builds,
# and `browse, no text` was measured at 27 ms and at 429 ms on the same tree
# within an hour on 2026-09-08. In the gate it would be a check that measures
# the machine instead of the thing.
#
# IT PRINTS AND IT DOES NOT JUDGE. A recipe that refused above a load
# threshold would go red on a correct tree whenever a browser was open -- a
# criterion that cannot pass. So it reports the head SHA, whether the tree is
# clean, and `uptime`'s one-minute load average twice, once before the build
# and once after the last test, and leaves the judgement of whether those
# numbers are a *reading* to the reader under the #533 re-run standard, which
# is quoted where it belongs -- `docs/agents/working-model.md`, this recipe's
# entry, off `docs/decisions/2026-09-v1-5-unattended-rulings.md`'s `## #533`.
# Two marks rather than one because that standard asks for the average at
# start *and* at end, and because a sibling that arrived mid-run is exactly
# what a once-sampled average misses.
#
# IT STILL GOES RED, on two things and neither of them the machine. The exit
# status is cargo's -- a failed budget assertion is a non-zero exit and
# libtest names the test -- and it is 1 when the harness ran a different
# number of tests than the pin below says, which is the #351 class: with
# `--ignored`, a benchmark that lost its `#[ignore]` is not a failure but a
# green over nothing measured. The recipe's own last lines say what the
# status and the count were rather than leaving them to a trailing `echo` to
# swallow.
#
# IT TAKES A GATE SLOT, through `scripts/gate-slot.sh`, the same protocol and
# the same directory `test` uses: it seeds 100 k rows several times over and
# starts a postmaster per test binary, so it counts against the two-gate
# ceiling and a third gate waits on it.
search-perf:
    #!/usr/bin/env bash
    set -euo pipefail
    # How many tests the two targets are pinned to run under `--ignored`: five
    # in `tests/perf.rs` and one in `tests/coverage.rs`. It is a pin like the
    # inventory's, and moving it is the price of adding a benchmark --
    # `test-inventory.txt`'s `knobas-search<TAB>test/perf` and
    # `test/coverage` lines are where the names themselves live.
    expected=6
    log=
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    # Armed before the `mktemp` they clean up, so an interrupt in between
    # leaks nothing. `cargo` runs in the FOREGROUND, unlike `test`'s pool:
    # bash runs a trap only after the foreground command returns, so a signal
    # aimed at this shell alone reaches cargo only when it is done -- which
    # here is what is wanted, because the slot should be held for exactly as
    # long as the run is. A terminal Ctrl-C goes to the process group and
    # reaches cargo directly.
    cleanup() { [ -z "$log" ] || rm -f "$log"; }
    trap 'cleanup; release_slot' EXIT
    trap 'cleanup; release_slot; trap - INT; kill -INT $$' INT
    trap 'cleanup; release_slot; trap - TERM; kill -TERM $$' TERM
    hold_gate_slot search-perf
    log=$(mktemp "${TMPDIR:-/tmp}/knobas-search-perf.XXXXXX")

    # One greppable line per mark: `search-perf: before ...` and
    # `search-perf: after ...`. The load average is `uptime`'s first figure on
    # either wording it has -- macOS prints `load averages: 2.71 2.83 2.90`,
    # Linux `load average: 0.52, 0.58, 0.59`. One space after the colon, never
    # the two `check`'s per-binary sum counts on.
    mark() {
        local when=$1 tree=clean
        [ -z "$(git status --porcelain)" ] || tree=dirty
        echo "search-perf: $when head=$(git rev-parse HEAD) tree=$tree" \
             "load1=$(uptime | sed -E 's/.*load averages?:[[:space:]]*//; s/[,[:space:]].*//')" \
             "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }

    mark before
    status=0 run_from=$SECONDS
    # Through `tee` so the run is still watchable line by line -- `--nocapture`
    # is the point of it -- while the counts below are read off the harness's
    # own `test result:` lines rather than off this recipe's confidence.
    # `pipefail` is what makes `$?` cargo's status and not tee's.
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-search \
      --test perf --test coverage \
      -- --ignored --nocapture --test-threads=1 2>&1 | tee "$log" || status=$?
    mark after
    # Two numbers, because `SECONDS` alone would fold a slot wait of up to
    # fifteen minutes into what reads as cargo's own time.
    echo "search-perf: cargo exited $status after $((SECONDS - run_from)) s;" \
         "the recipe, slot wait included, took ${SECONDS} s"
    # READ THE COUNTS (the working model's rule for this list, #351). With
    # `--ignored`, a benchmark that lost its `#[ignore]` -- or was renamed, or
    # filtered out by a typo in the target list -- is not a failure: libtest
    # prints `0 passed; 0 failed` and cargo exits 0, and the recipe would
    # report a green over a measurement that never happened. That is the one
    # refusal here, and it is a refusal about whether anything was measured,
    # never about the machine the measurement was taken on.
    ran=$(awk '/^test result:/ { for (i = 1; i < NF; i++) {
                   if ($(i+1) == "passed;") p += $i
                   if ($(i+1) == "failed;") f += $i } }
               END { print p + f }' "$log")
    echo "search-perf: $ran of $expected pinned tests ran"
    if [ "$ran" -ne "$expected" ]; then
        echo "error: $ran tests ran under --ignored, $expected are pinned." >&2
        echo "  Either a benchmark lost its #[ignore] (it is in 'just check'" >&2
        echo "  now, where a wall-clock budget must not be), or one was added" >&2
        echo "  or renamed and this recipe's pin has not moved with it." >&2
        [ "$status" -ne 0 ] || status=1
    fi
    echo "search-perf: these are timings; whether they are a reading is the #533"
    echo "search-perf: re-run standard's question, not this recipe's --"
    echo "search-perf: docs/decisions/2026-09-v1-5-unattended-rulings.md, '## #533'."
    exit "$status"

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

# The desktop witness (issue #500, ADR-0016): the signed bundle, launched from
# the path Launch Services registered, driven by desktop automation.
#
# `just desktop-witness launcher-hotkey` builds and signs a debug bundle with
# the `knobas-dev` identity, registers it, launches it on the demo profile,
# waits for its window, runs the named driver against the real accessibility
# tree, and quits it. The drivers live in testenv/desktop-witness/drivers/;
# running with no argument lists them.
#
# NOT part of `just check`, and it must not become part of it. It takes the
# screen away from whoever is at the Mac for the length of a run, it needs a
# code-signing identity and an Accessibility grant, and at most one may run on
# this machine at a time. The gate carries `witness-unit` instead.
#
# Scope, from the v1.5 grilling: OS-level features only -- the editor, the
# terminal, the hotkey -- because those have no instance to run a suite
# against. A rendered panel is witnessed by headless Chrome against the
# `?fake-ipc` dev server (deputy's ruling of 2026-09-08 on #496), not here.
desktop-witness driver="":
    testenv/desktop-witness.sh {{ quote(driver) }}

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

# Where the Uptime Kuma live recipe gets its gate variables. Kuma's own two,
# not the whole seed's: `./seed --env` refuses without `seed-state.json`, which
# is Gitea's, and seeding Gitea to run a Kuma suite would re-mint the Gitea
# token under whatever agent is mid-run against it (testenv/README.md, "One
# environment, one owner at a time"). `--env-kuma` is that half.
kuma_live_env := "testenv/, which is what the lines above this one do:
  docker compose up -d --wait uptime-kuma
  ./seed-kuma.sh
  eval \"$(./seed --env-kuma)\"

  `seed-kuma.sh` also needs `testenv/hetzner/hosts.env`, which is gitignored --
  three of the monitors ping the Hetzner servers by IP. It refuses without it
  rather than seeding an estate missing half of itself; copy the file into this
  worktree or run ./hetzner/provision.sh."

# The Uptime Kuma adapter against the REAL pinned container in `testenv/`
# (issue #442, M4.1), and then **M4.1's exit witness**: the whole alert chain
# against that container (issue #450). ADR-0013: the real container is the
# witness.
#
# TWO SUITES, in two crates, and the second is not the adapter's -- the shape
# `teamcity-live-seeded` established (issue #389) and its reason:
# `crates/knobas-app/tests/alert_chain_live.rs` asserts what the *inbox* does
# with an alert, and the inbox is an app-crate read, so a suite in
# `knobas-source-kuma` could not make the assertion at all. It runs second and
# is strictly additive to the first: it creates no monitor, so it disturbs
# neither the exact-set assertions of the suite above it nor its litter
# clearing.
#
# WHICH RECIPE CERTIFIES WHAT. `tests/contract.rs` runs the contract battery
# against a *recording* of this server's `/metrics`, so that `just check` stays
# docker-free (roadmap §3). This one certifies the same shapes against the
# server that decides them -- the four gauge families and their labels, `"null"`
# for a field a monitor has not got, `-1` for a response time that did not
# happen, `app_version`, a wrong key's 401 -- plus the one thing no recording
# can witness: that a monitor really deleted in Kuma really leaves the mirror.
# The suite's own header says what it deliberately does not run, and why.
#
# WHAT IT CREATES AND WHAT IT REMOVES. Four monitors, none of them named by
# `monitors.json` and each owned by one test: `knobas-live-scratch` and
# `knobas-write-scratch` are *added* through `testenv/kuma-monitor.sh` -- the
# same socket.io channel the seed uses -- and `knobas-live-created` and
# `knobas-write-created` are added by **knobas itself**, which is what issue
# #453's create is (a monitor the seed's helper made would witness nothing
# about the op under test). All four are deleted through that helper however
# the test ends, and anything a killed run leaves behind is removed by the
# `./seed-kuma.sh` above, which deletes every monitor `monitors.json` does not
# name. **No shared container is stopped by anything here** (M4 spec, issue
# #427).
#
# AND WHAT IT KNOCKS DOWN: `127.0.0.1:8299`, the canary's port, and nothing
# else. The exit suite releases it to make the estate's one red on purpose and
# rebinds it on the way out; this recipe binds it before either suite runs and
# rebinds it from a `trap` however the run ends, including a killed one -- so a
# failure leaves the estate green rather than leaving a monitor red for the
# next reader to explain. `testenv/canary.sh` is idempotent both ways, its
# whole blast radius is a host port nobody else binds, and the reason the
# estate carries such a check at all is that **every other thing Kuma watches
# here is shared** (`testenv/README.md`, "The canary").
#
# THE TUNNEL DOES NOT HAVE TO BE UP. Three of the seeded monitors check the
# products through `hetzner/tunnel` and are red without it. The suite asserts
# that every monitor carries *a* state Kuma published, never that a particular
# one is up, so a dead tunnel is not a red run here -- it is a red monitor,
# which is what a monitor is for.
#
# Serial and unparallelised: the runs share one server, and one test asserts
# how many monitors of each type the estate holds while another is adding one
# of its own.
#
# ONE ENVIRONMENT, ONE OWNER. `seed-kuma.sh` re-mints the API key on either of
# two triggers: the worktree it runs from holds no `kuma-api-key`, or it holds
# one the instance answers 401 or 403 to (#552). Re-minting destroys the
# instance's old key, so a second agent seeding mid-run makes the first agent's
# `/metrics` requests answer 401 -- and the 401 trigger makes that trade
# mutual, because the tree that got 401ed heals by re-minting in turn, which
# 401s the tree that just seeded. Only an answer the instance actually gave
# counts: `000` from a published port that is not answering, a 5xx, a proxy's
# 404 -- none of those is an answer, and the seed refuses rather than guess.
# Claim the environment before you run this.
# testenv/README.md, "One environment, one owner at a time".
kuma-live:
    #!/usr/bin/env bash
    set -euo pipefail
    # The repo root, captured before anything `cd`s: the trap below runs with
    # whatever directory the shell died in, and a relative path in it would
    # fail exactly when it is most needed.
    root=$PWD
    cd testenv
    docker compose up -d --wait uptime-kuma
    ./seed-kuma.sh
    eval "$(./seed --env-kuma)"
    just _require-live-env {{ quote(kuma_live_env) }} KNOBAS_KUMA_URL KNOBAS_KUMA_API_KEY \
      KNOBAS_KUMA_USER KNOBAS_KUMA_PASSWORD
    # The canary bound before either suite, and rebound however this ends.
    # `|| true` because the trap must not turn a suite's failure into a
    # different exit status, and `status` is not consulted first: `up` is
    # idempotent and says so itself.
    #
    # INT and TERM as well as EXIT, the `check` recipe's shape and its reason:
    # bash runs no EXIT trap for a signal it is not trapping, and a run someone
    # ctrl-Cs during the fall is exactly the run that leaves the canary red for
    # the next reader to explain. Each re-raises after restoring, so the caller
    # still sees an interrupted recipe; EXIT then restores a second time, which
    # costs a `curl` against a port that is already answering.
    #
    # Armed BEFORE the bind, which is `test`'s rule ("the traps are armed
    # before anything they clean up exists, so an interrupt between `mktemp`
    # and the server's start leaks nothing"). It costs nothing here -- an
    # interrupt during `up` restores a port that is either bound or free, and
    # `up` is what restores it either way.
    restore_canary() { cd "$root/testenv" && ./canary.sh up || true; }
    trap 'restore_canary' EXIT
    trap 'restore_canary; trap - INT; kill -INT $$' INT
    trap 'restore_canary; trap - TERM; kill -TERM $$' TERM
    ./canary.sh up
    cd ..
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-kuma --test live_kuma \
      -- --ignored --nocapture --test-threads=1
    # The same write half one seam higher (#452): the pause queued through
    # `submit_write`, settled, and read back out of `sync.item`. The adapter
    # suite above proves `Source::write` pauses a monitor; everything between
    # the button and that call -- the op decoded, the instance's write ops read
    # out of the keychain, the queue row, the flush, the mirror re-read -- is
    # knobas' own and is what a reader actually runs.
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test kuma_write_live \
      -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test alert_chain_live \
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
#   2026-09-06, the products on three Hetzner servers (testenv/hetzner/), one
#     per server, and the recipe still one product at a time: **986 s in all**,
#     seeded and verified at 883 -- Jira 147 s to FIRST_RUN and 176 s to its
#     first wizard form on a 2-vCPU cx23, every JVM start CPU-bound, the four
#     suites 76 s over the tunnel. Which is why, under `KNOBAS_TESTENV_HETZNER=1`
#     (what `source testenv/hetzner/env` sets), the recipe now starts and seeds
#     both products AT ONCE -- each has a server of its own, so the 8 GB reason
#     below does not apply. The local branch is unchanged.
#
#   2026-09-06, the parallel shape, Jira and Confluence on cx33s (4 vCPU, 8 GB):
#     **851 s in all**, seeded and verified at 754. Confluence was FIRST_RUN at
#     24 s and seeded at 141 s, entirely inside Jira's wait; Jira was FIRST_RUN
#     at 174 s, served its first form at 350 s, was RUNNING at 576 s -- NO
#     FASTER than on the cx23 (147 s / 176 s), with the host reporting zero
#     steal. Jira's start is single-threaded and the cx line is a 2.1 GHz
#     Skylake, so the 130 s saved is all overlap and none of it the resize.
#     The content seed pays ~90 ms of tunnel round trip per request on top.
#     The four suites: 80 s.
#
#   2026-09-06, KNOBAS_ATLASSIAN_KEEP=1 on cpx22s (AMD): the pair set up from
#     empty in 243 s (Jira FIRST_RUN at 72 s, its first form at 141 s, RUNNING at
#     233 s; Confluence seeded inside that) and then KEPT, its licences renewed
#     on the servers (testenv/hetzner/renew-licence.sh). Runs after that skip
#     the wizards and the content seed and are the suites: **226 s** for the
#     first run after set-up (the content seed from empty), **60 s** for every
#     run after (seeded and verified at 10 s, four suites in 46 s).
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
    if [ "${KNOBAS_ATLASSIAN_KEEP:-}" = 1 ]; then
      # THE PAIR STAYS UP. On the Hetzner servers the products live between
      # runs and testenv/hetzner/renew-licence.sh re-applies the timebomb key
      # on a timer, so there is no three-hour window to fit inside and no
      # `down -v` at the end: the next run finds both RUNNING, both seeds say
      # "already set up", the content seed skips everything, and the run is
      # the suites. Every step below is idempotent, which is what makes this
      # a flag and not a second recipe. A wipe is explicit, from testenv/
      # under the shim: `docker compose --profile real-atlassian down -v jira
      # jira-db confluence confluence-db`.
      echo "atlassian-live: KNOBAS_ATLASSIAN_KEEP=1 -- the pair is kept, not torn down"
      # TWO WORDS WITH THE RENEWER ON EACH SERVER (testenv/hetzner/renew-licence.sh).
      # First: if it last ran more than 120 minutes ago, run it now and wait --
      # a restart in the middle of a suite is a connection refused, and a
      # licence with under 30 minutes left is one a slow run could outlive.
      # Then: leave a run-in-progress marker for this run's duration, which the
      # renewer's timer waits on before it restarts anything. The marker is
      # removed from the EXIT trap, so a failed run does not hold the renewer.
      for _r in jira confluence; do
        ssh -o ConnectTimeout=10 "knobas-$_r" 'age=$(( $(date +%s) - $(stat -c %Y /opt/knobas/renew-status 2>/dev/null || echo 0) ))
          if [ "$age" -gt 7200 ]; then echo "atlassian-live: '"$_r"': licence last renewed ${age}s ago; renewing first"; systemctl start knobas-renew-licence.service; fi
          mkdir -p /opt/knobas && touch /opt/knobas/run-in-progress' &
      done
      wait
      trap 'for _r in jira confluence; do ssh -o ConnectTimeout=10 "knobas-$_r" rm -f /opt/knobas/run-in-progress || true; done' EXIT
    else
      trap 'teardown' EXIT
      trap 'trap - EXIT INT; teardown; kill -INT $$' INT
      trap 'trap - EXIT TERM; teardown; kill -TERM $$' TERM
    fi
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
    if [ "${KNOBAS_TESTENV_HETZNER:-}" = 1 ]; then
      # ONE SERVER PER PRODUCT (testenv/hetzner/README.md, set by `source
      # testenv/hetzner/env`): the 8 GB reason for the sequence in the other
      # branch is gone, so Confluence's whole start and wizard overlap Jira's.
      # Each product is an `up` then its seed, in a subshell; a failure in
      # either is this recipe's failure, after the other has finished -- a
      # half-started pair is what the teardown trap exists for, and it runs on
      # the exit below. seed-atlassian.sh's `record` locks seed-state.json, so
      # the two writers cannot lose each other's block.
      # `--no-recreate`: a container that exists is left as it is, even when
      # this shell's environment differs from the one it was created with --
      # which is exactly what happens to a KEPT pair the day Atlassian rotates
      # the key on its page and CONFLUENCE_LICENSE_KEY changes (measured on a
      # smaller drift, 2026-09-06: compose rebuilt a healthy Confluence under
      # the first kept run, and the seed waited 80 s for it to come back). A
      # config change to a running product is a hand-run `up -d` under the shim.
      ( docker compose --profile real-atlassian up -d --wait --no-recreate jira-db jira && ./seed-atlassian.sh jira ) &
      jira_pid=$!
      ( docker compose --profile real-atlassian up -d --wait --no-recreate confluence-db confluence && ./seed-atlassian.sh confluence ) &
      confluence_pid=$!
      status=0
      wait "$jira_pid" || { status=1; echo "atlassian-live: jira's up or seed failed" >&2; }
      wait "$confluence_pid" || { status=1; echo "atlassian-live: confluence's up or seed failed" >&2; }
      [ "$status" -eq 0 ] || exit 1
    else
      docker compose --profile real-atlassian up -d --wait jira-db jira
      ./seed-atlassian.sh jira              # waits for FIRST_RUN, walks Jira's wizard
      docker compose --profile real-atlassian up -d --wait confluence-db confluence
      ./seed-atlassian.sh confluence        # ... and Confluence's, on a quiet VM
    fi
    ./seed-atlassian-content.sh           # the Tidewater content
    # AND AGAIN, DELIBERATELY. Every step of that script is find-then-skip, and
    # the skip branch is the only one that reads where a page already sits off
    # the server instead of choosing it -- so a window that always seeds from
    # scratch never reaches it, on a script four suites read (#396). The second
    # run writes nothing and costs seconds; the day it writes twice, or records
    # a different tree than the first run built, this line is what says so.
    ./seed-atlassian-content.sh
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
    # M4.2's exit witness (#455): one test, the share export's Jira clause --
    # every call it makes to this Jira is a read, and it writes nothing.
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test share_exit -- --ignored --nocapture --test-threads=1
    echo "atlassian-live: every Atlassian-gated live suite green (5 suites); $(( $(date +%s) - t0 ))s so far"

# **Both importers** against the real systems (issues #509 and #510, v1.5
# streams 9 and 10). `crates/knobas-app/tests/estate_live.rs` is the suite and
# it is two tests: the hcloud producer against Hetzner Cloud, and the Docker
# producer against the four engines `testenv/hetzner/estate.json` names.
#
# WHAT IT CERTIFIES, AND WHY IT IS THE ONLY THING THAT CAN. Both producers are
# run against a fake in `just check` (`tests/assets_ipc.rs`: a recorded hcloud
# answer, and a stub docker executable), which certifies shape -- the decode,
# the file, the origin-key match, the argument rule, the landing. A recording
# and a stub are green forever, so these facts have no witness but this recipe:
#
#   1. that Hetzner still answers in that shape, and that the docker CLI still
#      writes one JSON object per line;
#   2. that the three `hcloud_id` values in `testenv/hetzner/estate.json` are
#      the ids of the three real servers (#508 wrote them from `hcloud server
#      list`; a *missing* id is red on a mutant, a plausible-but-wrong one is a
#      second copy of a server and silent);
#   3. that the four `docker_context` values in that file are contexts this
#      machine has, each reaching the engine the file says it does;
#   4. that every **running** container on those four engines is written down in
#      that file, under the name it carries there -- the "diff against the
#      estate file" the roadmap booked.
#
# **A RED RUN NAMING A SERVER OR A CONTAINER IS READ FIRST AS A WRONG VALUE IN
# THAT FILE**, and only then as a bug in a producer: a wrong `hcloud_id` is
# unknown by id and unmatched by key, so its server previews as *new*; a
# container running on a server and never written down previews as *new* the
# same way. That is the orchestrator's reading, posted on #509 under the
# deputy's ruling of 2026-09-08 on #508, and both suites' headers repeat it
# where a reader of a stack trace will meet it.
#
# NO TUNNEL, NO CONTAINER, NO SEED. hcloud's API is public. The three Hetzner
# docker contexts are `ssh://knobas-<product>`, which `~/.ssh/config` resolves
# to each server's own address on port 22, while `testenv/hetzner/tunnel`
# forwards only the products' HTTP ports (8111, 8080, 8090) -- so no forward is
# in a docker call's path. That is a reading of what the two carry and not a run
# with the tunnel down; the run that was made, on 2026-09-08, had it up. Every
# call either half makes is a
# read, against a shared fixture (`testenv/README.md`: no shared container is
# stopped by anything here, and nothing here touches one). The `hcloud` contexts
# named `terra-*` belong to other projects and are not consulted: this reads the
# *token*, and the token is the project.
#
# WHAT IT DOES NEED, besides the token: the **docker CLI** and the four contexts
# `testenv/hetzner/provision.sh` writes (`orbstack` is OrbStack's own). A context
# this machine does not have is `unreachable` and red -- deliberately not a gap
# in the produced file, because a file that quietly omitted one engine's
# containers is indistinguishable from an engine holding none.
#
# The variable is **gated rather than skipped on**, `teamcity-live`'s rule and
# issue #351's: a suite that skipped by name on an unset variable is one libtest
# counts as a pass, and a green line certifying nothing is worse than a refusal.
# The docker CLI is gated the same way and for the same reason.
#
# Serial and unparallelised: two tests, one rate-limiter budget, somebody else's
# servers.
estate-live:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f .env ]; then set -a; . ./.env; set +a; fi
    just _require-live-env 'the repo-root .env, which is gitignored -- the same
    HETZNER_API_TOKEN testenv/hetzner/provision.sh reads:
      cp .env.example .env   # then paste the Hetzner Cloud API token in
    No tunnel and no seed are needed; this reads hcloud and the docker contexts.' \
      HETZNER_API_TOKEN
    if ! command -v docker >/dev/null 2>&1; then
      echo "error: the docker CLI is not on the PATH." >&2
      echo "  The Docker half of this recipe spawns it once per container engine" >&2
      echo "  (\`docker --context <context> ps --format json\`), so without it the" >&2
      echo "  suite would refuse every engine and report a fault that is this" >&2
      echo "  machine's rather than the estate's. The four contexts it needs come" >&2
      echo "  from testenv/hetzner/provision.sh; \`orbstack\` is OrbStack's own." >&2
      exit 1
    fi
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test estate_live \
      -- --ignored --nocapture --test-threads=1
