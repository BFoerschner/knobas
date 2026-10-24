# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# Four rustc jobs per cargo unless the caller says otherwise (#1892). Cargo's
# own default is one per core, twelve here, and the gates and up to three
# agents' builds share those cores; the briefs export 4 for every agent call,
# and this is the same figure for a call that forgot. `CARGO_BUILD_JOBS=12
# just check` still asks for twelve.
export CARGO_BUILD_JOBS := env("CARGO_BUILD_JOBS", "4")

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
# that fails is reported as its captured output in full, and every one that
# fails is -- running them together must not cost a signal the serial run
# had. No half is cut short by another failing, for the same reason, and the
# exit status is non-zero if any half's was. (Since #920 there are three: the
# script checks run as a half of their own.) `front` still goes
# through `deps`: a missing `app/node_modules` is installed, and if that
# cannot happen the frontend half is a red, not a skip.
#
# The traps are the inventory recipe's: `INT` and `TERM` stop whatever is
# left of every half and remove the scratch files before re-raising, so a
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
    # A stale section 10.8 index fails the gate before anything slow runs (#737),
    # and so does a hand-written table that lost step with the tree (#1919).
    python3 -I scripts/contract-index.py --check
    python3 -I scripts/doc-lists.py
    front_log= scripts_log= cargo_log= front_pid= scripts_pid= cargo_pid=
    # The script checks are a half of their own (#920): none of them runs
    # cargo or reads `target/`, and in the cargo chain their 23 s (warm,
    # quiet, 2026-10-15) were paid in full ahead of `clippy`.
    script_recipes=(shell test-layout witness-unit conflict-markers)
    cargo_recipes=(fmt clippy clippy-libs inventory ipc-mirror test)
    descendants() {
        local p
        for p in $(pgrep -P "$1" || true); do descendants "$p"; done
        echo "$1"
    }
    stop_halves() {
        local p pids=
        for p in $front_pid $scripts_pid $cargo_pid; do pids="$pids $(descendants "$p")"; done
        [ -z "${pids// /}" ] || kill -TERM $pids 2>/dev/null || true
        wait
    }
    cleanup() { rm -f "$front_log" "$scripts_log" "$cargo_log"; }
    # Traps first, so a second `mktemp` failing cannot leave the first behind.
    trap 'cleanup' EXIT
    trap 'stop_halves; cleanup; trap - INT; kill -INT $$' INT
    trap 'stop_halves; cleanup; trap - TERM; kill -TERM $$' TERM
    front_log=$(mktemp "${TMPDIR:-/tmp}/knobas-check-front.XXXXXX")
    scripts_log=$(mktemp "${TMPDIR:-/tmp}/knobas-check-scripts.XXXXXX")
    cargo_log=$(mktemp "${TMPDIR:-/tmp}/knobas-check-cargo.XXXXXX")

    # Starts the recipes in the background, one `just` each, with
    # everything they print in LOG; the caller takes the pid from `$!`. Each
    # recipe's wall time is a line of its own (`step: <recipe> N s`, #920),
    # the chain stops at its first red as `just A B C` does, and the last line
    # is the half's own exit status and wall time.
    run_half() {
        local name=$1 log=$2
        shift 2
        ( SECONDS=0; status=0
          for r in "$@"; do
              start=$SECONDS
              just "$r" || status=$?
              echo "step: $r $((SECONDS - start)) s"
              [ "$status" -eq 0 ] || break
          done
          echo "$name: exit $status after ${SECONDS} s"; exit "$status" ) >"$log" 2>&1 &
    }

    echo "check: running 'front' and '${script_recipes[*]}' beside '${cargo_recipes[*]}';"
    echo "check: each half's output is captured and reported when it finishes."
    run_half front "$front_log" front; front_pid=$!
    run_half scripts "$scripts_log" "${script_recipes[@]}"; scripts_pid=$!
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
                scripts) awk '/^step: / { printf "  step %-13s %4d s\n", $2, $3 }' "$log" ;;
                cargo) awk '/^test: (ok|FAILED)  / { n++; for (i = 1; i < NF; i++) {
                             if ($(i+1) == "passed;") p += $i
                             if ($(i+1) == "failed;") f += $i
                             if ($(i+1) == "ignored;") g += $i } }
                           END { printf "  %d test binaries and doc-test crates: %d passed, %d failed, %d ignored\n", n, p, f, g }' "$log"
                       # Where the half's time went (#920): each recipe's
                       # wall time, `test`'s wait for a gate slot (and on
                       # whom, #1866), its build and run, its slowest binaries.
                       awk '/^step: / { printf "  step %-13s %4d s\n", $2, $3 }' "$log"
                       { grep -E '^test: (build|run|gate slot free after) |^test: .*; waiting up to |^test: slowest|^  +[0-9.]+ s ' "$log" || true; } | sed 's/^/  /' ;;
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
    scripts_status=0; wait "$scripts_pid" || scripts_status=$?
    scripts_pid=
    report scripts "$scripts_log" "$scripts_status"
    cargo_status=0; wait "$cargo_pid" || cargo_status=$?
    cargo_pid=
    report cargo "$cargo_log" "$cargo_status"

    if [ "$front_status" -ne 0 ] || [ "$scripts_status" -ne 0 ] || [ "$cargo_status" -ne 0 ]; then
        echo "check: FAILED (front exit $front_status, scripts exit $scripts_status, cargo exit $cargo_status)" >&2
        # 75 is `hold_gate_slot`'s queue timeout (#1690), not a red step: it
        # passes through only when it is the sole failure, so the caller can
        # tell "the slots were held, run it again" from a red gate by `$?`.
        if [ "$cargo_status" -eq 75 ] && [ "$front_status" -eq 0 ] && [ "$scripts_status" -eq 0 ]; then
            echo "check: the gate slot wait expired (exit 75): the queue, not a red gate; run it again" >&2
            exit 75
        fi
        exit 1
    fi
    echo "check: ok in ${SECONDS} s"

# The frontend half of `check` alone, with the contract-index check it starts
# with. Enough for a head `scripts/gate-scope.sh` calls `front`: nothing in
# `crates/` reads what changed, so the cargo half would run the tree main
# already passed. Ruling: Björn, 2026-10-15.
#
# Its vitest run is the tests the change can reach, not the whole suite
# (#920): `front-changed` below. `check` still runs every test.
check-front base="origin/main":
    python3 -I scripts/contract-index.py --check
    python3 -I scripts/doc-lists.py
    just conflict-markers
    just front-changed "{{base}}"
    @echo "check-front: ok"

# The three checks that guard a docs-only change, nothing else: the
# contract-index check (a new `docs/contract/10.8/` file without a regenerated
# index), the doc-list check (#1919: a crate, `app/src/lib/` directory,
# live-suite recipe or live product a hand-written table lists, or misses) and
# the conflict-marker scan. Seconds, no `node_modules`, no cargo. What a
# docs-only self-merge runs in place of a gate (#1691, ruling D5 in
# `docs/decisions/2026-10-review-planning-rulings.md`); `scripts/docs-check-test.sh`
# pins the red cases. Calls the script directly, not `just conflict-markers`,
# so the test can run it from a scratch tree.
docs-check:
    python3 -I scripts/contract-index.py --check
    python3 -I scripts/doc-lists.py
    scripts/conflict-markers.sh

# `front` with vitest narrowed to what a change since BASE can reach (#920).
# svelte-check and `vite build` stay whole: together under 10 s, and a type
# error can surface in a file nothing imports from the change.
#
# Vitest runs `vitest related` over the changed `app/src` files -- every test
# file whose import graph reaches one, and every changed test file -- plus
# every test that reads source files off disk (`readFileSync`, `readdirSync`,
# `import.meta.glob`, `?raw`: the a11y, residue, house-rules and CSS guards
# and their kind). Those see the tree through `fs`, not through imports, so
# the graph cannot say when one is reached; they are found by that grep on
# every run, so a new guard is included without a list to keep. The whole
# suite runs instead when anything changed in `app/` outside `app/src`
# (config, `package.json`, the lockfile), when the test setup file changed
# (every test runs it, none imports it), or when an `app/src` file was
# deleted or renamed (a test that still imports it is reached by nothing).
#
# What it stops covering: a test that depends on a changed file other than
# by import or by reading files -- through a global a changed module sets up
# at import time in some other test's graph, say. None is known (2026-10-15).
# `check` runs every test, so the full gate still sees one.
front-changed base="origin/main": deps
    #!/usr/bin/env bash
    set -euo pipefail
    mb=$(git merge-base "{{base}}" HEAD)
    cd app
    npx svelte-check --tsconfig ./tsconfig.json
    # Captured before the loop, not read from a process substitution, so a
    # failing git stops the recipe instead of leaving `changed` empty.
    # Untracked files count as added (`A`).
    diff=$(git diff --name-status "$mb" -- .)
    untracked=$(git ls-files --others --exclude-standard --full-name -- . | awk '{ print "A\t" $0 }')
    all=
    changed=()
    while IFS=$'\t' read -r st f _; do
        [ -n "$f" ] || continue
        case $f in app/src/*) ;; app/*) all="$f is outside app/src" ;; *) continue ;; esac
        case $st in D*|R*) all="$f was deleted or renamed" ;; esac
        [ "$f" != app/src/lib/shell/test-setup.ts ] || all="the test setup file changed"
        changed+=("${f#app/}")
    done <<< "$diff"$'\n'"$untracked"
    guards=()
    while IFS= read -r f; do guards+=("$f"); done \
      < <(grep -rlE 'readFileSync|readdirSync|import\.meta\.glob|\?raw' src | grep -E '\.test\.(svelte\.)?ts$' | sort)
    if [ -n "$all" ] || [ ${#guards[@]} -eq 0 ]; then
        echo "front-changed: the whole vitest suite: ${all:-no fs-reading guard tests found}"
        npx vitest run
    else
        echo "front-changed: vitest related to ${#changed[@]} changed files, plus ${#guards[@]} fs-reading guard tests"
        npx vitest related --run ${changed[@]+"${changed[@]}"} "${guards[@]}"
    fi
    npm run build

# Screenshots under `?fake-ipc`, one dev server for all routes:
# `just shot 3 shots time job/PAY-231`; the leading `#/` is optional but quote it
# (`'#/time'`). Steps, one argument per shot: `'#/job/PAY-231 > #/builds'` opens a
# job first; `'#/repo/x/branches + click:New branch…'` clicks, then captures the dialog.
# `SHOT_THEME=light|dark` sets the colour scheme.
# See scripts/shot.sh for query parameters and size.
[positional-arguments]
shot slot out +routes:
    scripts/shot.sh "$@"

# The merge gate: `check-front` when `scripts/gate-scope.sh BASE` says the head
# is frontend-only against BASE, the full `check` otherwise.
#
# A full gate runs `pre-gate` first. Of 112 failed gate runs in Oct 10-19,
# 32 full gates failed on rustfmt (12) or clippy (20), both of which pre-gate
# answers in under a minute and the first of which it fixes; they failed
# because the merge-manager's fix commits and rebase landed after the
# implementer's pre-gate. What pre-gate changes is left uncommitted and named,
# so the merge-manager commits it before merging; the gate itself ran on it.
# A frontend-only head skips it: nothing pre-gate fixes applies to one, and
# `check-front` already runs its svelte-check and its three vitest guards.
gate base="origin/main":
    #!/usr/bin/env bash
    set -euo pipefail
    scope=$(scripts/gate-scope.sh "{{base}}")
    echo "gate: $scope"
    if [ "$scope" = front ]; then
        just check-front "{{base}}"
    else
        just pre-gate
        just check
        if [ -n "$(git status --short)" ]; then
            echo "gate: green, and pre-gate changed files; commit them before merging:"
            git status --short
        fi
    fi

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
      | scripts/inventory-list.sh \
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
# It lives here, in `just check`, because the `testenv` workflow that used to
# own it was disabled along with `check` (billing, 2026-08-29), and from then
# until both came back as manual dispatches nothing linted these scripts at
# all -- which is how a `# shellcheck disable=SC2016` came to sit one line
# above the wrong command for a whole commit. A gate nobody runs is not a gate.
#
# DISCOVERY IS BY SHEBANG, NOT BY GLOB. The plan's `shellcheck testenv/seed
# testenv/seed-*.sh ...` list already missed two of the seven scripts then in
# testenv/ (`check-ports.sh` and `specs/fetch.sh`): a glob cannot fail loudly when a new script stops matching it, it just
# quietly checks less than you think. `git ls-files` rather than `find`, so a
# seeded volume or a stray local script can never enter the set.
# `.github/workflows/testenv.yml` runs this recipe rather than a copy of it.
#
# THE EMPTY GUARD IS THE POINT, not decoration. Bare `shellcheck` exits 3 with
# "No files specified.", so a discovery that matched nothing would fail -- but
# it would fail in a usage dump that reads as a broken recipe rather than as a
# discovery that stopped matching. It also keeps the recipe off the exit code
# of a tool whose "no arguments is an error" is a behaviour, not a promise.
#
# ONE SHELLCHECK VERSION, named here and nowhere else (#2036). Two versions
# disagree about what an info is: the 0.9.0 that ubuntu-latest ships raised
# SC2317 and SC2119 on lines Homebrew's 0.11.0 passes, so a script clean on
# the Mac went red on the runner, and #1713 had already fixed one such line
# without the cause. A local binary is used only when it is this version, so
# `just check` does not require Docker of everyone; any other version (the
# runner's, or a Mac's after a `brew upgrade`) runs the pinned image instead,
# and with neither the recipe stops and names both. A bump changes this line
# and fixes what the new version reports, in one PR.
shellcheck_version := "0.11.0"

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

    pinned={{ shellcheck_version }}
    found=$(shellcheck --version 2>/dev/null | sed -n 's/^version: //p' || true)
    echo "shellchecking ${#scripts[@]} scripts with shellcheck $pinned"
    if [ "$found" = "$pinned" ]; then
        shellcheck "${scripts[@]}"
    elif command -v docker >/dev/null; then
        docker run --rm -v "$PWD:/mnt" -w /mnt \
            "koalaman/shellcheck:v$pinned" "${scripts[@]}"
    else
        echo "error: shellcheck $pinned is pinned, and the local one is ${found:-missing} with no docker for the pinned image." >&2
        echo "  Install shellcheck $pinned, or start Docker; to move the pin, change shellcheck_version in the justfile." >&2
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
# helper checks compile `ax.swift` and ask it for the probe, a registered path
# and live-shot's refusals (#1483), and skip themselves where macOS and
# `swiftc` are not both present; live-shot's own refusals run dry, stopping
# once its lock is held, and only on macOS. `shot-test.sh`'s pointer-step cases
# (#1483) start headless Chrome on a fixture page, and its start-up cases
# (#1957) a Chrome 20 s late, a dead one, a mute one and a hung one, about
# 36 s in all; they skip where Chrome is not installed.
#
# The test-layout script's own tests (issue #573) ride here too: they are bash
# against fixture trees, need nothing installed, and take under a second.
# So do the on-demand live products' (issue #633): the one-JVM-product rule
# and what a run puts back, against a stub `docker`. And the Gitea seed
# token's (issue #684): one worktree's seed never revokes another's token,
# against a stub `curl` playing Gitea's token API. And the JVM lock's (issue
# #690): a second live run waits for the first and goes on when it lets go, a
# dead holder's lock is taken over, the wait ends in a refusal naming the
# holder -- against a stub `docker` and a scratch lock path; and the Gitea
# lock's (issue #724), the same mechanism, which Gitea sessions queue on.
# And the `knobas-test-` guard (issue #803): the prefix the knobas-db test
# reaper deletes may not name a temp dir in `scripts/` or `testenv/`. And the
# merge-tail lock's (issue #1274): a holder whose PR just merged keeps the lock
# until it releases, a holder dead past the grace period is taken over by one
# waiter, `take` is re-entrant and `assert` names the holder -- against a stub
# `gh` and a private TMPDIR, about 15 s. And the gate scope's (issue #1543): a
# head that changes only an app file a testenv script pins answers `full`. And
# `docs-check` (issue #1691): green on a clean tree, red on a stray section 10.8
# file, on a conflict marker in prose, and on a table row the tree lost or a
# member the table lacks (#1919).
witness-unit:
    testenv/desktop-witness-test.sh
    scripts/bash-guard-test.sh
    scripts/shot-test.sh
    scripts/test-layout-test.sh
    scripts/inventory-list-test.sh
    scripts/ipc-mirror-check-test.sh
    scripts/append-only-merge-test.sh
    scripts/test-prefix-guard-test.sh
    scripts/test-prefix-guard.sh
    scripts/merge-tail-lock-test.sh
    scripts/gate-scope-test.sh
    scripts/gate-slot-test.sh
    scripts/machine-load-test.sh
    scripts/docs-check-test.sh
    testenv/live-test.sh
    testenv/gitea-token-test.sh
    testenv/seed-env-test.sh
    testenv/gitea-pr-state-test.sh
    testenv/live-lock-test.sh

# The step before every `just check` (issue #736): the mechanical misses that
# failed gates in the #701 wave, found in about a minute instead of a gate's.
# It FIXES what is mechanical (`cargo fmt --all`, `inventory-update`), runs the
# fast table tests that fail first -- the `$effect` cleanup table
# (`residue.test.svelte.ts`: a new component with an effect must be in it) and
# the write-op count (`every_op_has_its_own_snake_case_identifier`: a new
# `WriteOp` variant needs a probe in `every_write_op` and the count bumped) --
# clippy on the changed crates (#972) and svelte-check. Then it prints what it changed, so you commit exactly that.
# It is not the gate; run `just check` once after it, as before.
pre-gate:
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot() { { git status --short; git diff; } | shasum; }
    # One line, never a failure (#743): a configured rustc-wrapper that is
    # missing breaks every cargo call, and the error names cargo, not the wrapper.
    wrapper=${RUSTC_WRAPPER:-}
    if [ -z "$wrapper" ]; then
        wrapper=$(sed -n 's/^[[:space:]]*rustc-wrapper[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' \
            "${CARGO_HOME:-$HOME/.cargo}/config.toml" .cargo/config.toml 2>/dev/null | tail -1 || true)
    fi
    if [ -n "$wrapper" ] && ! { [ -x "$wrapper" ] || command -v "$wrapper" >/dev/null 2>&1; }; then
        echo "pre-gate: WARNING cargo's rustc-wrapper '$wrapper' is missing; every cargo call will fail (testenv/README.md, 'sccache on a dev machine')"
    elif [ -n "$wrapper" ] && [ "$(basename "$wrapper")" = sccache ] && ! "$wrapper" --version >/dev/null 2>&1; then
        echo "pre-gate: WARNING sccache at '$wrapper' does not run (testenv/README.md, 'sccache on a dev machine')"
    fi
    # A niced sccache server nices every compile on the machine (#1892):
    # sccache runs rustc as the server's child, and the server takes the
    # priority of whichever cargo call started it. On 2026-10-23 one had run
    # at nice 10 for eleven hours, so the gates' compiles ran niced behind
    # everything else. One line, never a failure.
    if [ -n "$wrapper" ] && [ "$(basename "$wrapper")" = sccache ]; then
        # The server is the `sccache` with no arguments; a client names the compiler it wraps.
        for p in $(pgrep -f '(^|/)sccache$' || true); do
            ni=$(ps -o ni= -p "$p" 2>/dev/null | tr -d ' ' || true)
            if [ -n "$ni" ] && [ "$ni" -gt 0 ]; then
                echo "pre-gate: WARNING the sccache server (pid $p) runs at nice $ni, and so does every compile it runs; 'sccache --stop-server' when no build is running, and the next cargo call starts it at normal priority"
            fi
        done
    fi
    before=$(snapshot)
    # A leftover conflict marker, then the bundle build (#999): PR #997's
    # rebase left a `|||||||` line in a stylesheet that only lightningcss's
    # minify in `vite build` refused. The grep is under a second, the build
    # about 3 s; both are also in the gate (`check`, `check-front`).
    just conflict-markers
    npm --prefix app run build
    env -u RUSTUP_TOOLCHAIN cargo fmt --all
    just inventory-update
    just contract-index
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source --lib every_op_has_its_own_snake_case_identifier
    # The generated IPC mirror (#1325): written here so a Rust change arrives
    # with its TypeScript among the files to commit; `check` fails without it.
    env -u RUSTUP_TOOLCHAIN cargo test -q -p knobas-app --lib ipc_mirror
    # The gate's clippy, on the crates this branch changed (#972); the flags are
    # the `clippy` and `clippy-libs` recipes' own.
    pkgs=$(scripts/changed-crates.sh)
    if [ -n "$pkgs" ]; then
        echo "pre-gate: clippy on $pkgs"
        just clippy $pkgs
        just clippy-libs $pkgs
    else
        echo "pre-gate: no Rust changed, no clippy."
    fi
    (cd app && npx vitest run src/lib/shell/residue.test.svelte.ts)
    (cd app && npx vitest run src/lib/shell/a11y.test.ts)
    (cd app && npx vitest run src/lib/shell/browser-globals.test.ts)
    (cd app && npx svelte-check --tsconfig ./tsconfig.json)
    echo
    if [ "$before" = "$(snapshot)" ]; then
        echo "pre-gate: green, nothing changed."
    else
        echo "pre-gate: green. Uncommitted changes (commit what is yours):"
        git status --short
    fi

# A leftover merge-conflict marker line (`<<<<<<<`, `|||||||`, `=======`,
# `>>>>>>>`) in any file git knows (#999). Deliberate fixtures are listed in
# `scripts/conflict-markers-allow.txt`. In `pre-gate`, `check` and `check-front`.
conflict-markers:
    scripts/conflict-markers.sh

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

# The fixture external adapter (#1198, ADR-0028), built to a WASM component
# for the WASM host's tests and the registry's: `knobas-wasm-host`'s
# `fixture_component()` reads it from here. The target comes with the toolchain
# (`rust-toolchain.toml` lists it), so nothing is installed outside the repo's
# pin. Release, because wasmtime compiles the component again on every load
# and a smaller one compiles faster; `test` runs this first.
#
# Twice: first against the frozen 1.0.0 world (`--features v1_0`, kept beside
# as `knobas_wasm_fixture_v1_0.wasm`, #1205), for the host's hold on 1.0.0
# components, then the current one. The second build relinks the fixture
# alone.
wasm-fixture:
    env -u RUSTUP_TOOLCHAIN cargo build -p knobas-wasm-fixture --target wasm32-wasip2 --release --quiet --features v1_0
    cp target/wasm32-wasip2/release/knobas_wasm_fixture.wasm target/wasm32-wasip2/release/knobas_wasm_fixture_v1_0.wasm
    env -u RUSTUP_TOOLCHAIN cargo build -p knobas-wasm-fixture --target wasm32-wasip2 --release --quiet

# The gate's lint pass. PKGS narrows it to the given cargo package selection
# (`just clippy -p knobas-core`; `pre-gate` passes the crates a branch changed,
# #972); with none it lints the whole workspace. The one place the flags live.
clippy *PKGS:
    env -u RUSTUP_TOOLCHAIN cargo clippy {{ if PKGS == "" { "--workspace" } else { PKGS } }} --all-targets -- -D warnings

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
clippy-libs *PKGS:
    env -u RUSTUP_TOOLCHAIN cargo clippy {{ if PKGS == "" { "--workspace" } else { PKGS } }} --lib -- -D warnings

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
# them. 112 binaries (2026-09-05), 98 of them under 3 s each, still added up
# to 71 s of the serial gate; through `xargs -P`, with each binary's stdout and stderr
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
# machine, warm tree, re-measured 2026-10-10 after ADR-0017's merges: the
# numbers are in the recipe beside the constant, with the start order's.
# `KNOBAS_TEST_JOBS` overrides it, for measuring another value without
# editing this file.
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
# GATE SLOTS. At most one gate-sized recipe runs on one machine at a time by
# default (two before #1892; `KNOBAS_GATE_SLOTS=2` asks for two). The
# protocol that enforces it lives in `scripts/gate-slot.sh`, sourced below,
# with the measurement, the whole reasoning, and the list of what sources it in
# its header -- one place, so a change of ceiling is one edit. In short: one
# of `KNOBAS_GATE_SLOTS` (default 1) symlinks under `$TMPDIR/knobas-gate-slots`
# is taken before this recipe does anything else, a gate that finds them all
# held prints one line naming the holders and polls up to
# `KNOBAS_GATE_SLOT_WAIT` seconds, and the slot is released by the same EXIT,
# INT and TERM traps that close the pipe. SysV shared memory would allow two
# (one gate peaks at 13 of macOS's 32 segments); the cores allow one, and the
# test threads per binary below bound what that one costs (#1892).
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
    # The fixture component the WASM host's tests load (#1198), before the
    # test binaries that load it.
    fixture_start=$SECONDS
    just wasm-fixture
    echo "test: wasm-fixture $((SECONDS - fixture_start)) s"
    build_start=$SECONDS
    just _test-executables > "$gate/executables"
    echo "test: build $((SECONDS - build_start)) s"
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

    # N = 6. Re-measured 2026-10-10 on 0ffc2c07 (#580), once every crate's
    # integration tests were one binary (ADR-0017) and the pool had shrunk
    # from about 130 binaries to 52: the 12-core machine, warm tree, runs
    # back to back and interleaved round-robin so load drift hits every N
    # alike (load average 15-28 throughout, from the runs themselves), the
    # whole `just test`:
    #   N=12: 94, 89, 84 s   N=8: 89, 84, 81 s   N=6: 83, 85, 86 s   N=4: 80, 85, 85 s
    # 4, 6 and 8 cannot be told apart (means 83-85 s, while rounds of one N
    # vary by 3-10 s) and 12 is the slowest. The pool is still CPU-bound, not
    # queue-bound: every binary's libtest runs one test thread per core, and
    # the longest binary (`knobas-app`'s `test/it`, 577 tests) takes 30-60 s
    # of the run under that contention. 6 stays, the middle of the flat
    # region: half the cores, and a connection peak far below the server's
    # 400 (82 client backends at N=6 on 2026-09-05, when the pool was 112
    # binaries; N=12 peaked at 143 then).
    #
    # Re-measured 2026-10-15 (#920), whole `just check` after a one-line
    # `knobas-core` edit, 1-minute load under 4 at each start, interleaved:
    # `test`'s run N=6: 79, 77, 78 s; N=8: 82, 82 s; N=10: 82, 85 s. The run
    # is as long as `knobas-app`'s `test/it` (77-85 s on its own), so more
    # binaries at once only slow that one down. 6 stays. SysV segments peaked
    # at 14 of 32 at every N.
    #
    # THE START ORDER is cargo's build order, as `_test-executables` prints
    # it, which starts `knobas-app`'s `test/it` last. Measured the same day,
    # same way, at N=6, in a run of its own just before the one above and
    # under less load (so compare within each run, not across them): cargo's
    # order 71, 82, 80 s; reversed (that binary first) 83, 88, 86 s; the
    # seven slowest binaries first (`knobas-app`,
    # `knobas-core` and `knobas-sync`'s `test/it`, `scheduler_loop`,
    # `knobas-db`'s `embedded`, `schema` and lib) 87, 89, 90 s. Starting the
    # heavy binaries together only stacks their test threads on the same
    # cores: it was slower in every round, by 7-16 s. No reordering is
    # applied.
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

    # TEST THREADS PER BINARY, T = 4 (#1892): libtest's own default is one
    # test thread per core, so six binaries at once could run 72 tests, and
    # nearly every one of them starts with a `create database` on the gate's
    # server. That is where a gate's load came from: on 2026-10-23, sampling
    # the runnable threads of one `just test`'s own processes (the daemonized
    # postmaster and its backends included) every 2 s, beside the loop's other
    # work, warm tree, one run each:
    #   T=12 (default): run 134 s, own runnable threads mean 20.0 (p90 36), 14.4 of them Postgres
    #   T=4:            run 138 s, mean 8.4 (p90 16), Postgres 5.2
    #   T=2:            run 158 s, mean 4.5 (p90 12), Postgres 2.8
    # The run is bound by the server and the file system (a scratch
    # database's files are created, then unlinked with the root), not by
    # test threads, so four threads cost it nothing measurable. Since #1897 a
    # scratch database a finished test held is reset and handed on instead
    # (`test_util`'s `recycle`): about 180 databases a gate rather than 930,
    # and every gate binary against a private server, before and after, two
    # rounds: run 176 and 137 s -> 101 and 95 s, the server's CPU 343 and
    # 300 s -> 231 and 228 s (system time halved), 10.8 GB of data -> 3.2. Paired
    # with one gate slot (`scripts/gate-slot.sh` has that measurement: two
    # gates and a pre-gate side by side, load1 median 43.5 before, 18.5
    # after), not two: two slots at T=2 still timed a vitest test out beside
    # them. `KNOBAS_TEST_THREADS` overrides it; it reaches every binary and
    # the doc-test run as `RUST_TEST_THREADS`.
    threads=${KNOBAS_TEST_THREADS:-4}
    case $threads in
        ''|*[!0-9]*) echo "error: KNOBAS_TEST_THREADS must be a whole number of at least 1, not '$threads'" >&2; exit 1 ;;
    esac
    threads=$((10#$threads))
    if [ "$threads" -lt 1 ]; then
        echo "error: KNOBAS_TEST_THREADS must be at least 1 (libtest reads 0 as an error)" >&2; exit 1
    fi
    export RUST_TEST_THREADS=$threads

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
        # The wall time, process start to exit, goes to `out/<n>.time` for
        # the slowest-binaries list; bash's `time` keyword because bash 3.2
        # has no sub-second clock of its own. libtest's `finished in` leaves
        # out the binary's start-up and anything it does outside the harness.
        # `time` is aimed at a function, not at the subshell: bash 3.2 prints
        # nothing for `{ time ( a; b ); } 2>file` (checked 2026-10-15, #920).
        run_binary() { ( cd "$dir" && exec "$exe" </dev/null ) > "$log" 2>&1; }
        local TIMEFORMAT=%R LC_NUMERIC=C
        { time run_binary; } 2> "$KNOBAS_GATE_DIR/out/$n.time" || status=$?
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

    echo "test: $count binaries, $jobs at a time with $threads test threads each, doc-tests alongside; one Postgres for all of them"
    run_start=$SECONDS
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
    echo "test: run $((SECONDS - run_start)) s"

    # The ten slowest binaries by wall time (#920), slowest first: what the
    # pool's length is made of. A binary with no time file never ran and is
    # reported as a red below, not here.
    echo "test: slowest binaries"
    n=0
    while IFS=$'\t' read -r _ pkg target _ _; do
        n=$((n + 1))
        t=$(cat "$gate/out/$n.time" 2>/dev/null || true)
        [ -z "$t" ] || printf '%s\t%s\t%s\n' "$t" "$pkg" "$target"
    done < "$gate/tests" \
      | sort -t $'\t' -k1,1 -rn | sed -n 1,10p \
      | awk -F'\t' '{ printf "  %7.1f s  %-24s %s\n", $1, $2, $3 }'

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
# fixed budget, `just check` runs beside other gates and agents' builds,
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
# starts a postmaster per test binary, so it counts against the gate
# ceiling and a gate waits on it.
search-perf:
    #!/usr/bin/env bash
    set -euo pipefail
    # How many tests the two targets are pinned to run under `--ignored`: six
    # in `tests/perf.rs` and one in `tests/coverage.rs`. It is a pin like the
    # inventory's, and moving it is the price of adding a benchmark --
    # `test-inventory.txt`'s `knobas-search<TAB>test/perf` and
    # `test/coverage` lines are where the names themselves live.
    expected=7
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

# Like `search-perf` it takes a gate slot (it starts a postmaster), prints the
# head and the load average before and after, and goes red when the harness
# ran other than the one pinned test -- a renamed or un-ignored test is a green
# over nothing measured otherwise.
#
# The builds-of-a-commit reading (#732): `knobas-app`'s one ignored test in
# `tests/it/builds_of_a_commit_perf.rs`, 50,000 builds seeded into a scratch
# database, the lookup's plan checked and the fastest of ten `explain analyze`
# runs held to 5 ms. Ignored, and so here rather than in `check`, for the
# reason `search-perf` is: a wall-clock budget inside the gate measures the
# machine. The gate keeps the plan assertion, at 3,000 builds.
# Take the builds-of-a-commit reading at 50,000 builds.
builds-perf:
    #!/usr/bin/env bash
    set -euo pipefail
    expected=1
    log=
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    cleanup() { [ -z "$log" ] || rm -f "$log"; }
    trap 'cleanup; release_slot' EXIT
    trap 'cleanup; release_slot; trap - INT; kill -INT $$' INT
    trap 'cleanup; release_slot; trap - TERM; kill -TERM $$' TERM
    hold_gate_slot builds-perf
    log=$(mktemp "${TMPDIR:-/tmp}/knobas-builds-perf.XXXXXX")
    mark() {
        local when=$1 tree=clean
        [ -z "$(git status --porcelain)" ] || tree=dirty
        echo "builds-perf: $when head=$(git rev-parse HEAD) tree=$tree" \
             "load1=$(uptime | sed -E 's/.*load averages?:[[:space:]]*//; s/[,[:space:]].*//')" \
             "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }
    mark before
    status=0
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test it builds_of_a_commit_perf \
      -- --ignored --nocapture --test-threads=1 2>&1 | tee "$log" || status=$?
    mark after
    ran=$(awk '/^test result:/ { for (i = 1; i < NF; i++) {
                   if ($(i+1) == "passed;") p += $i
                   if ($(i+1) == "failed;") f += $i } }
               END { print p + f }' "$log")
    echo "builds-perf: cargo exited $status; $ran of $expected pinned tests ran"
    if [ "$ran" -ne "$expected" ]; then
        echo "error: $ran tests ran under --ignored, $expected is pinned." >&2
        [ "$status" -ne 0 ] || status=1
    fi
    exit "$status"

# The suggestion pass with nothing new (#745): `knobas-core`'s one ignored test
# in `tests/it/similar_text_perf.rs` -- 10,000 mirrored items seeded into a
# scratch database, one pass to settle them, then the fastest of five passes
# over the unchanged mirror held to one second, each rule's time printed.
# Ignored, and so here rather than in `check`, for `search-perf`'s reason. The
# gate keeps the behaviour, `similar_text_incremental`. Takes a gate slot and
# prints the head and the load average before and after.
# Take the nothing-new suggestion pass reading at 10,000 items.
suggest-perf:
    #!/usr/bin/env bash
    set -euo pipefail
    expected=1
    log=
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    cleanup() { [ -z "$log" ] || rm -f "$log"; }
    trap 'cleanup; release_slot' EXIT
    trap 'cleanup; release_slot; trap - INT; kill -INT $$' INT
    trap 'cleanup; release_slot; trap - TERM; kill -TERM $$' TERM
    hold_gate_slot suggest-perf
    log=$(mktemp "${TMPDIR:-/tmp}/knobas-suggest-perf.XXXXXX")
    mark() {
        local when=$1 tree=clean
        [ -z "$(git status --porcelain)" ] || tree=dirty
        echo "suggest-perf: $when head=$(git rev-parse HEAD) tree=$tree" \
             "load1=$(uptime | sed -E 's/.*load averages?:[[:space:]]*//; s/[,[:space:]].*//')" \
             "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }
    mark before
    status=0
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-core --test it similar_text_perf \
      -- --ignored --nocapture --test-threads=1 2>&1 | tee "$log" || status=$?
    mark after
    ran=$(awk '/^test result:/ { for (i = 1; i < NF; i++) {
                   if ($(i+1) == "passed;") p += $i
                   if ($(i+1) == "failed;") f += $i } }
               END { print p + f }' "$log")
    echo "suggest-perf: cargo exited $status; $ran of $expected pinned tests ran"
    if [ "$ran" -ne "$expected" ]; then
        echo "error: $ran tests ran under --ignored, $expected is pinned." >&2
        [ "$status" -ne 0 ] || status=1
    fi
    exit "$status"

# The UI pool under a syncing estate (#745): `knobas-app`'s one ignored test in
# `tests/it/ui_pool_probe.rs` -- five slow sources syncing through the real
# scheduler for three minutes while a frontend loop searches, opens, lists and
# notes on a five-connection pool, `pg_stat_activity` sampled every second.
# Red when the UI pool ran out of free connections in any sample, or any UI
# call failed or took over a second. Like `builds-perf` it takes a gate slot,
# prints the head and the load average before and after, and goes red when
# other than the one pinned test ran. Sizes: `HP_SOURCES`, `HP_ITEMS`,
# `HP_SECS` in the environment (defaults 5, 1000, 180).
# Run the UI-pool starvation probe: five sources syncing, three minutes.
ui-pool-probe:
    #!/usr/bin/env bash
    set -euo pipefail
    expected=1
    log=
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    cleanup() { [ -z "$log" ] || rm -f "$log"; }
    trap 'cleanup; release_slot' EXIT
    trap 'cleanup; release_slot; trap - INT; kill -INT $$' INT
    trap 'cleanup; release_slot; trap - TERM; kill -TERM $$' TERM
    hold_gate_slot ui-pool-probe
    log=$(mktemp "${TMPDIR:-/tmp}/knobas-ui-pool-probe.XXXXXX")
    mark() {
        local when=$1 tree=clean
        [ -z "$(git status --porcelain)" ] || tree=dirty
        echo "ui-pool-probe: $when head=$(git rev-parse HEAD) tree=$tree" \
             "load1=$(uptime | sed -E 's/.*load averages?:[[:space:]]*//; s/[,[:space:]].*//')" \
             "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }
    mark before
    status=0
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test it ui_pool_probe \
      -- --ignored --nocapture --test-threads=1 2>&1 | tee "$log" || status=$?
    mark after
    ran=$(awk '/^test result:/ { for (i = 1; i < NF; i++) {
                   if ($(i+1) == "passed;") p += $i
                   if ($(i+1) == "failed;") f += $i } }
               END { print p + f }' "$log")
    echo "ui-pool-probe: cargo exited $status; $ran of $expected pinned tests ran"
    if [ "$ran" -ne "$expected" ]; then
        echo "error: $ran tests ran under --ignored, $expected is pinned." >&2
        [ "$status" -ne 0 ] || status=1
    fi
    exit "$status"

# What the reader waits on (#753): `knobas-app`'s one ignored test in
# `tests/it/responsiveness_probe.rs` -- three Jira-shaped sources of 5,000
# tickets, 2,000 pages and 5,000 builds seeded into a scratch database, and the
# seeded Gitea synced when `testenv/seed --env` answers -- then every read the
# shell makes for the common interactions timed once cold and seven times warm
# on the app's five-connection UI pool, one `RP` line each. A table for a PR
# beside the load average, never a budget: it is red only when the read fails
# or other than the one pinned test ran. Sizes: `RP_TICKETS`, `RP_PAGES`,
# `RP_BUILDS`, `RP_RUNS`. A write-surface section closes it (#775): what each
# dialog waited on before it closed and what it waits on now, every send
# simulated as `RP_SEND_MS` (300). Takes a gate slot and prints the head and the load
# average before and after.
#
# The built database is kept (#781) under `$TMPDIR/knobas-probe-snapshots/`, one
# copy never analyzed and one analyzed, keyed by the migrations plus the seed
# parameters; a run clones it (`cp -c`) instead of seeding again. `RP_FRESH=1`
# rebuilds, `RP_ITEMS=2500` is the small size to iterate on (its own snapshot,
# default 22000). Snapshots over 7 days old or of other migrations are deleted.
# Time every read the common interactions make, against a seeded mirror.
responsiveness-probe:
    #!/usr/bin/env bash
    set -euo pipefail
    expected=1
    log=
    # shellcheck source=scripts/gate-slot.sh disable=SC1091
    . ./scripts/gate-slot.sh
    cleanup() { [ -z "$log" ] || rm -f "$log"; }
    trap 'cleanup; release_slot' EXIT
    trap 'cleanup; release_slot; trap - INT; kill -INT $$' INT
    trap 'cleanup; release_slot; trap - TERM; kill -TERM $$' TERM
    hold_gate_slot responsiveness-probe
    log=$(mktemp "${TMPDIR:-/tmp}/knobas-responsiveness-probe.XXXXXX")
    mark() {
        local when=$1 tree=clean
        [ -z "$(git status --porcelain)" ] || tree=dirty
        echo "responsiveness-probe: $when head=$(git rev-parse HEAD) tree=$tree" \
             "load1=$(uptime | sed -E 's/.*load averages?:[[:space:]]*//; s/[,[:space:]].*//')" \
             "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }
    # The seeded Gitea's address and token, when there is a seed to read.
    # Captured first: `eval "$(...)"` would swallow a refused token's exit
    # (#1377), and cargo must not start on a token Gitea answers 401 to.
    if [ -r testenv/seed-state.json ]; then
        seed_env=$(cd testenv && ./seed --env) || exit 1
        eval "$seed_env"
    fi
    mark before
    status=0
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test it responsiveness_probe \
      -- --ignored --nocapture --test-threads=1 2>&1 | tee "$log" || status=$?
    mark after
    ran=$(awk '/^test result:/ { for (i = 1; i < NF; i++) {
                   if ($(i+1) == "passed;") p += $i
                   if ($(i+1) == "failed;") f += $i } }
               END { print p + f }' "$log")
    echo "responsiveness-probe: cargo exited $status; $ran of $expected pinned tests ran"
    if [ "$ran" -ne "$expected" ]; then
        echo "error: $ran tests ran under --ignored, $expected is pinned." >&2
        [ "$status" -ne 0 ] || status=1
    fi
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

# The frontend suite with the wall clock at TIME today, in the suite's zone
# (Europe/Berlin): a test that builds a local date from "now" and compares it
# with a UTC one fails only in the first hours after midnight, and that is when
# nobody runs it (#1031, #1150, #1155 were one bug filed three times). The pin
# is `test-setup.ts`'s, read from `KNOBAS_TEST_CLOCK`; only `Date` is faked.
test-midnight time="00:10": deps
    cd app && KNOBAS_TEST_CLOCK={{time}} npx vitest run

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

# Screenshots of the real app against live products (#1483): starts `just demo`
# with KNOBAS_SECRET_STORE=memory, sources <steps> (shot, press, focus, type,
# key, click-at, settle; testenv/live-shot.sh has each) and writes the knobas
# window only, scaled from Retina, to <out>. LIVE_SHOT_PRODUCT=gitea (or another
# testenv/live product) brings one up under its lock first and exports its
# `./seed --env`; LIVE_SHOT_AEROSPACE=1 fullscreens the window in AeroSpace.
# Takes the screen and the desktop witness's lock; never part of a gate.
live-shot out steps:
    testenv/live-shot.sh {{ quote(out) }} {{ quote(steps) }}

# The single-instance witness (#748): launches the debug binary twice against a
# temp HOME and an unreachable database, and checks the second never reaches
# database startup. `--demo-beside` checks the demo profile still runs beside a
# normal instance. Opens a window; not part of `just check`.
single-instance-witness mode="":
    env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app
    scripts/single-instance-witness.sh {{ mode }}

# Where the three Gitea-backed live recipes get their gate variables, in one
# place because all three obtain them the same way: the lines this text names
# are literally the lines above each guard call. Kept as a variable rather than
# a second private recipe -- what the three share is a message and a variable
# list, not behaviour, and a forwarding recipe would be an indirection with
# nothing in it.
gitea_live_env := "testenv/live-lib.sh, which is what the line above this one does:
  live_session gitea   # docker compose up -d --wait gitea, ./seed-gitea.sh,
                       # eval \"$(./seed --env)\"; Gitea stays up after the
                       # run -- only 'just live-down gitea' stops it (#633, #698)"

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
# FORGEJO (#1195, spec #1192 story 18): `just gitea-live forgejo` is the one
# switch. It runs this same recipe -- the adapter suite, the three start-work
# tests and the clone test -- against the pinned Forgejo container (profile
# `forgejo`, port 3002, its own volume and seed state, `seed-forgejo.sh`)
# instead of Gitea. Forgejo is stopped again at the end if the run started it;
# Gitea is left alone. It takes the Gitea lock. `gitea-live-capped` and
# `start-work-live` have no Forgejo form. No gate runs this form, so a change
# this recipe exercises, a test or the adapter code under one, is run here as
# well as against Gitea (#2034: a test added after #1195 was green on Gitea and
# had never met Forgejo).
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
# being server-side and inclusive (Gitea's; Forgejo 17 ignores `since=` and
# `until=` on commits, #2034), and a revoked token's 401. It cannot certify
# what happens when the server answers FEWER records than the adapter asked for
# -- every corpus the seed creates fits in one page of 50 here -- and that is
# what `gitea-live-capped` below is for.
#
# Gitea and its seed only: `testenv/seed` also waits for uptime-kuma and mockd,
# which this suite never touches. Both steps are idempotent, so re-running this
# against an already-seeded environment just runs the tests.
#
# GITEA IS STARTED ON DEMAND (issue #633). `live_session gitea`
# (testenv/live-lib.sh) starts it if it is down, seeds it, exports what
# `./seed --env` prints, and leaves it up when the recipe ends, whoever started
# it: no run stops Gitea, only `just live-down gitea` does (#698) -- another
# worktree's run may be using it by then.
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
# the suite's branch sweep cannot tell a sibling's live branch from a corpse.
# The token is no longer one of the collisions: since #684 each worktree's
# seed keeps its own (`testenv/gitea-token.sh`), and never revokes another's.
# Nor are two runs of the Gitea recipes, since #724: `live_session gitea`
# holds the machine-wide Gitea lock for the whole run, the JVM lock's mechanism
# (testenv/live-lib.sh), so a second run from another worktree waits for the
# first instead of sweeping its branches. testenv/README.md, "One environment,
# one owner at a time".
#
# AND ONE APP-LEVEL TEST (#752): `a_write_answers_on_queue_and_lands_behind_the_reader`
# in `crates/knobas-app/tests/start_work_live.rs`, the write queue over this
# server -- a comment and a merge each answer on the queue insert and land in
# the background -- and the Submit-to-dialog-closed timings it prints. Its
# branches and pull requests carry the same `knobas-` prefix and are taken back
# by that file's `Litter`, never touching the default branch.
#
# AND TICKETS (#1206): `a_gitea_issue_is_on_the_tickets_board_beside_a_jira_ticket`
# syncs the seeded ledger-api issues and an in-process mockd Jira into one app,
# and reads the Tickets board over both. Read-only at Gitea.
#
# AND CLONE (#823): `crates/knobas-app/tests/clone_live.rs` clones a seeded
# repository over http (admitted by the test-only door; the app clones over
# https and ssh only, ADR-0023), refuses a folder that is not a clone and a bad
# name, and stops a clone. Read-only at Gitea: nothing to take back.
gitea-live forge="gitea":
    #!/usr/bin/env bash
    set -euo pipefail
    case {{ quote(forge) }} in gitea | forgejo) ;; *) echo "gitea-live: forge must be gitea or forgejo, not '{{ forge }}'" >&2; exit 2 ;; esac
    . testenv/live-lib.sh
    live_session {{ forge }} gitea-live
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-gitea --test live_gitea \
      -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
      -- --ignored --nocapture --test-threads=1 --exact a_write_answers_on_queue_and_lands_behind_the_reader \
      a_commit_over_a_file_that_moved_is_held_with_both_versions_at_gitea
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
      -- --ignored --nocapture --test-threads=1 --exact back_to_back_comments_land_in_order_unheld_and_a_merge_between_holds_the_second
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
      -- --ignored --nocapture --test-threads=1 --exact a_gitea_issue_is_on_the_tickets_board_beside_a_jira_ticket
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test clone_live \
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
# The uncap runs from `live_session`'s traps (`live_on_exit`), which put the
# product back (Gitea itself stays up, #698), and those traps are `INT` and `TERM` as
# well as `EXIT`, the rule `inventory` above states and `check-ports.sh`
# follows: bash need not run an `EXIT` trap when a signal it has no handler for
# terminates the shell, and this recipe's slow live run is
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
# share. It recreates the shared container capped to one record and back again,
# so a concurrent `just gitea-live` would read a server answering short pages
# and fail for a reason that is not in its own tree. The Gitea lock (#724)
# makes the other Gitea recipes wait; a TeamCity run, which takes no Gitea
# lock, still sees the capped server. testenv/README.md, "One environment, one
# owner at a time".
gitea-live-capped:
    #!/usr/bin/env bash
    set -euo pipefail
    . testenv/live-lib.sh
    live_session gitea gitea-live-capped
    # Before the overlay, not after: a run refused for a missing variable must
    # not be one that left the shared container capped on its way out.
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
    # Registered with the session, which runs it from its own EXIT, INT and
    # TERM traps -- so the container is uncapped whichever way this ends, and
    # left up after (no run stops Gitea, #698).
    uncap() { (cd "$LIVE_TESTENV" && docker compose --env-file images.env up -d --wait gitea >/dev/null); }
    live_on_exit uncap
    (cd "$LIVE_TESTENV" && docker compose --env-file images.env -f docker-compose.yml -f docker-compose.capped.yml up -d --wait gitea)
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-gitea --test live_gitea_capped \
      -- --ignored --nocapture --test-threads=1

# The start-work flow over the real Gitea: ticket -> branch -> pull request ->
# link -> In Progress, and back again when the pull request is merged (issue
# #44). The suite is `crates/knobas-app/tests/start_work_live.rs`: the round
# trip is one test, deliberately -- what is under test is the round trip. Beside
# it, read-only, is Gitea's app-level `under:` witness (#644), which syncs every
# fixture repository into a database of its own and asks the app's search.
#
# WHICH RECIPE CERTIFIES WHAT. `gitea-live` above certifies the *adapter*
# against the shapes interfaces §4.2 fixes. This one certifies the *flow* over
# it: the real orchestrator, the real write queue, the real adapters and a real
# database. Its sibling `tests/it/start_work.rs` proves the same sequence against a
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
# that writes to it. ONE ENVIRONMENT, ONE OWNER -- it writes branches and
# pull requests a concurrent run's sweep can take (the token is per worktree
# since #684, so seeding no longer 401s anyone), which is why it holds the Gitea
# lock (#724) through `live_session` and a concurrent Gitea run waits.
# testenv/README.md, "One environment, one owner at a time".
start-work-live:
    #!/usr/bin/env bash
    set -euo pipefail
    . testenv/live-lib.sh
    live_session gitea start-work-live
    just _require-live-env {{ quote(gitea_live_env) }} KNOBAS_GITEA_URL KNOBAS_GITEA_TOKEN
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
      -- --ignored --nocapture --test-threads=1

# Where the Uptime Kuma live recipe gets its gate variables. Kuma's own two,
# not the whole seed's: `./seed --env` refuses without `seed-state.json`, which
# is Gitea's, and a Kuma suite has no business starting and seeding Gitea
# (testenv/README.md, "One environment, one owner at a time"). `--env-kuma` is
# that half.
kuma_live_env := "testenv/, which is what the lines above this one do:
  docker compose --env-file images.env up -d --wait uptime-kuma
  ./seed-kuma.sh
  eval \"$(./seed --env-kuma)\""

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
# WHICH RECIPE CERTIFIES WHAT. `tests/it/contract.rs` runs the contract battery
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
# NO PRODUCT HAS TO BE UP. Three of the seeded monitors check TeamCity, Jira
# and Confluence at their published ports, and those products run on demand
# (#633), so the three are red whenever none is started. The suite asserts
# that every monitor carries *a* state Kuma published, never that a particular
# one is up, so a stopped product is not a red run here -- it is a red monitor,
# which is what a monitor is for.
#
# Serial and unparallelised: the runs share one server, and one test asserts
# how many monitors of each type the estate holds while another is adding one
# of its own.
#
# ONE ENVIRONMENT, ONE OWNER. `seed-kuma.sh` re-mints the API key on either of
# two triggers: the worktree it runs from holds no `kuma-api-key`, or it holds
# one the instance answers 401 or 403 to (#552). The key is this worktree's
# own, named `knobas-seed-<worktree>-<checksum>` (#1484), so a re-mint deletes
# only this tree's earlier key and never a sibling's: before #1484 every tree
# shared one name and two trees 401ed each other in turn. Only an answer the
# instance actually gave counts: `000` from a published port that is not
# answering, a 5xx, a proxy's 404 -- none of those is an answer, and the seed
# refuses rather than guess. The run still adds, pauses and deletes monitors
# and binds the canary on the one shared Kuma, so claim the environment before
# you run this.
# testenv/README.md, "One environment, one owner at a time".
kuma-live:
    #!/usr/bin/env bash
    set -euo pipefail
    # The repo root, captured before anything `cd`s: the trap below runs with
    # whatever directory the shell died in, and a relative path in it would
    # fail exactly when it is most needed.
    root=$PWD
    cd testenv
    docker compose --env-file images.env up -d --wait uptime-kuma
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
    # No pause between the binaries: Uptime Kuma allows twenty logins a
    # minute, and the adapter holds one logged-in session across writes rather
    # than logging in per write (#983), so the suite above no longer spends the
    # window this one needs. (#864 slept 61 s here.)
    #
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
    # The JVM lock (issue #690) although the default URL is not ours: a `.env`
    # pointed at the seeded container makes this a TeamCity run like any other.
    . testenv/live-lib.sh
    live_lock_traps
    live_lock_hold teamcity-live
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-teamcity --test live_teamcity \
      -- --ignored --nocapture --test-threads=1

# The TeamCity adapter against the **seeded, self-hosted** TeamCity
# (issue #266) -- the sibling `teamcity-live` cannot be: a corpus we own, so
# the suite asserts the seeded content by id, number, status and branch, runs
# the contract battery (clause 2 included), and queues one build through REST
# to watch the `sinceBuild` watermark move and then stand still. It deletes
# what it queued, and clears what a killed run left. The header of
# `tests/live_teamcity_seeded.rs` states the whole contract, including what it
# writes.
#
# THE PRODUCT IS STARTED HERE, ON THIS MACHINE (issue #633). `live_session
# teamcity` (testenv/live-lib.sh) starts Gitea -- the VCS roots clone it --
# and then the server and its agent, seeds all three, exports what `./seed
# --env` prints, and stops the server and agent again if it started them
# when the recipe ends, however it ends; Gitea stays up (no run stops it,
# only `just live-down gitea`, #698). A TeamCity already up (`just live-up teamcity`) is left
# up, which is the fast loop: the second run is the suites. It refuses to
# start while Jira or Confluence is up, naming it. *Stop*, never `down -v`:
# the TeamCity volumes hold the seeded builds, and the next run finds them.
#
# TWO SUITES, in two crates, and the second is not the adapter's (issue #389).
# `knobas-app`'s `teamcity_seeded_live` is M3.3's exit criterion for this
# source: a seeded build the mirror attributes to the reader is on the standup
# digest for the day it ran. The digest is an app-crate read, so `live_teamcity_seeded.rs`
# -- in the adapter's crate -- cannot make that assertion, and a live suite with
# no recipe is one nothing runs (testenv/README.md). It is strictly read-only
# and runs second, so it disturbs neither the exact-set assertions nor the
# leftover clearing of the suite above it. Since #683 it also syncs the seeded
# Gitea this recipe already brought up, so a build's change and revision are
# witnessed opening the mirrored commit.
#
# Serial and unparallelised for the same reason as `gitea-live`: one server,
# and one test that mutates it. One owner at a time: testenv/README.md, "One
# environment, one owner at a time".
teamcity-live-seeded:
    #!/usr/bin/env bash
    set -euo pipefail
    . testenv/live-lib.sh
    live_session teamcity teamcity-live-seeded
    just _require-live-env 'seed-state.json, via eval "$(./seed --env)", once
    seed-teamcity.sh has run -- which live_session teamcity, the line above
    this one, does.' \
      KNOBAS_TEAMCITY_URL KNOBAS_TEAMCITY_TOKEN
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-teamcity --test live_teamcity_seeded \
      -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test teamcity_seeded_live \
      -- --ignored --nocapture --test-threads=1

# Start one real product on this machine and seed it, or stop it again (issue
# #633): gitea, teamcity, jira or confluence. Every `*-live` recipe below does
# this around its own suites; these are for a session of several runs, since
# a product already up is left up by them. One JVM product at a time: `up`
# refuses TeamCity, Jira or Confluence while another of the three is up, and
# names it. `down` stops TeamCity and Gitea (their volumes hold the seeded
# corpus) and removes Jira and Confluence with their volumes (3-hour timebomb
# licences: a kept volume is an expired product). testenv/README.md, "Live
# products, on demand".
live-up product:
    testenv/live up {{ quote(product) }}

live-down product:
    testenv/live down {{ quote(product) }}

live-status:
    testenv/live status

# The Confluence half of `crates/knobas-app/tests/atlassian_live.rs`, by test
# name. That file holds both products' app-level tests, and since #633 the two
# products are never up at once -- so `jira-live` runs the file with these
# skipped and `confluence-live` runs exactly these. A test added to the file
# and not listed here runs with Jira: a Jira one passes there, and a Confluence
# one fails there loudly, on `wiki()`'s unset KNOBAS_CONFLUENCE_URL, which is
# the prompt to list it. `_atlassian-live-names` refuses a listed name the file
# no longer has, so a rename cannot quietly shrink the Confluence run.
atlassian_live_confluence_tests := "a_comment_that_mentions_me_becomes_an_inbox_mention the_launcher_finds_the_seeded_page_with_its_ancestor_path a_page_the_source_says_is_mine_is_on_the_digest_for_the_day_it_moved a_page_edited_through_knobas_is_on_the_digest_under_yesterday a_revoked_confluence_pat_reaches_the_credential_health_surface_and_the_mirror_survives a_protocol_is_published_under_standup_protocols_and_reads_back a_second_page_with_one_title_in_one_space_is_refused under_its_grandparent_finds_the_seeded_grandchild"

_atlassian-live-names:
    #!/usr/bin/env bash
    set -euo pipefail
    # stderr kept: a compile error here is this recipe's failure, and should
    # say so rather than end the run without a word.
    list=$(env -u RUSTUP_TOOLCHAIN cargo test -q -p knobas-app --test atlassian_live \
      -- --ignored --list) || { echo "error: could not list atlassian_live's tests" >&2; exit 1; }
    listed=$(sed -n 's/: test$//p' <<<"$list")
    for name in {{ atlassian_live_confluence_tests }}; do
      grep -qx -- "$name" <<<"$listed" || {
        echo "error: atlassian_live has no test '$name', which the justfile's" >&2
        echo "  atlassian_live_confluence_tests names. Rename or remove it there." >&2
        exit 1; }
    done

# The real Jira, on this machine, inside one licence window (issues #275,
# #633; ADR-0013): fetch the timebomb keys, start `jira-db` and `jira`, walk
# the setup wizard, seed the Tidewater content (twice -- the second run must
# write nothing, #396) and prove it by reading it back, run every live suite
# gated on the Jira URL, and remove the pair with its volumes -- from a trap,
# so the teardown runs when a step fails too. All of that is `live_session jira`
# (testenv/live-lib.sh); a Jira already up from `just live-up jira` is used
# as it is and left up.
#
# THREE HOURS IS THE WINDOW. The licence is a 10-user timebomb key that expires
# three hours after it is applied and that a restart does not renew; the
# product is disposable by design, which is why a run that started it ends in
# removing it with its volumes. The suites take seconds.
#
# MEASURED on this machine (12 cores, the 8 GB Docker VM), from empty volumes
# with the images pulled, 2026-10-12 (#633): 299 s in all, seeded and verified
# at 244-263 s, the three suites ~15 s once compiled. `atlassian-live` below
# keeps the pair's earlier history.
#
# THE SUITES. `live_jira_seeded` (#276) -- sync, payload, cursor, the real
# 401; the Jira half of the app crate's `atlassian_live` -- credential health
# end to end, the three write ops read back out of Jira, and `under:` through
# the app's search on the seeded epic (#644, read-only); and `share_exit`
# (#455), M4.2's exit witness, every call of which is a read. Serial and
# unparallelised: one server, and tests that write to it. Each suite takes
# back what it wrote from a `Drop` that checks rather than assumes, and a
# killed run's leftovers are put back by the adapter suite's next run, which
# works from `seed-state.json`. What none can undo is the `PAY` key counter,
# which is why nothing downstream may assume the fixture's keys are the
# highest ones.
#
# ONE JVM PRODUCT AT A TIME. The session refuses to start while TeamCity or
# Confluence is up, and names it (8 GB VM: Jira wants ~4 GB). It never stops
# another product itself: that is somebody's session going away under them.
jira-live:
    #!/usr/bin/env bash
    set -euo pipefail
    just _atlassian-live-names
    . testenv/live-lib.sh
    t0=$(date +%s)
    live_session jira jira-live
    just _require-live-env 'seed-state.json, via eval "$(./seed --env)", once
    seed-atlassian.sh and seed-atlassian-content.sh have run for jira -- which
    live_session jira, the line above this one, does.' \
      KNOBAS_JIRA_URL KNOBAS_JIRA_USER KNOBAS_JIRA_PASSWORD
    echo "jira-live: seeded and verified after $(( $(date +%s) - t0 ))s; KNOBAS_JIRA_URL=$KNOBAS_JIRA_URL"
    skips=()
    for name in {{ atlassian_live_confluence_tests }}; do skips+=(--skip "$name"); done
    # ---- live suites gated on the Jira URL: one line each, added here ----
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-jira --test live_jira_seeded -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test atlassian_live -- --ignored --nocapture --test-threads=1 --exact "${skips[@]}"
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test share_exit -- --ignored --nocapture --test-threads=1
    echo "jira-live: every Jira-gated live suite green (3 suites); $(( $(date +%s) - t0 ))s so far"

# The real Confluence, on this machine, the same shape as `jira-live` above:
# its timebomb key fetched before the container's first start (Confluence
# reads it there), `confluence-db` and `confluence` started, the wizard
# walked, the content seeded twice and verified, every Confluence-gated suite
# run, and the pair removed with its volumes from a trap. Refuses while
# TeamCity or Jira is up.
#
# THE SUITES. `live_confluence_seeded` (#284) -- the **only** witness that
# adapter has, since ADR-0013 refused a mockd half for a product with no
# machine-readable spec; the Confluence half of `atlassian_live` -- the M3.2
# exit criterion (a comment that mentions me becomes an inbox item) and
# Confluence's credential health; and `confluence_live`. They write one
# title, comments, a page edit and a page under the standup parent, and take
# each back from a checking `Drop`; a killed run's are put back from
# `seed-state.json` by the next.
confluence-live:
    #!/usr/bin/env bash
    set -euo pipefail
    just _atlassian-live-names
    . testenv/live-lib.sh
    t0=$(date +%s)
    live_session confluence confluence-live
    just _require-live-env 'seed-state.json, via eval "$(./seed --env)", once
    seed-atlassian.sh and seed-atlassian-content.sh have run for confluence --
    which live_session confluence, the line above this one, does.' \
      KNOBAS_CONFLUENCE_URL KNOBAS_CONFLUENCE_USER KNOBAS_CONFLUENCE_PASSWORD
    echo "confluence-live: seeded and verified after $(( $(date +%s) - t0 ))s; KNOBAS_CONFLUENCE_URL=$KNOBAS_CONFLUENCE_URL"
    # ---- live suites gated on the Confluence URL: one line each, added here ----
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-confluence --test live_confluence_seeded -- --ignored --nocapture --test-threads=1
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test atlassian_live -- --ignored --nocapture --test-threads=1 --exact {{ atlassian_live_confluence_tests }}
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test confluence_live -- --ignored --nocapture --test-threads=1
    echo "confluence-live: every Confluence-gated live suite green (3 suites); $(( $(date +%s) - t0 ))s so far"

# Both Atlassian products, one after the other: `jira-live`, whose session
# removes Jira again, then `confluence-live`. Never the two at once -- the
# 8 GB VM holds one JVM product (issue #633) -- so this is the two recipes in
# sequence and nothing more; a failure in the first stops the second.
#
# HISTORY, for the numbers. Until #633 this recipe brought both products up
# itself, Jira first and Confluence once Jira was RUNNING, and kept both up
# through the suites: measured 2026-09-03 between 295 s and 392 s in all from
# empty volumes, of which the wizards and the content seed were ~265 and the
# suites ~20; both JVMs starting *together* is what died twice under load
# (#313 -> #314). From 2026-09-06 to 2026-10-12 it ran against three Hetzner
# servers instead, decommissioned since (#634; `testenv/hetzner/` is in git
# history).
atlassian-live:
    #!/usr/bin/env bash
    set -euo pipefail
    # One hold of the JVM lock (issue #690) for both: the two recipes below
    # run under it (`KNOBAS_LIVE_LOCK_HOLDER`), so no other JVM run slips in
    # between Jira's removal and Confluence's start.
    . testenv/live-lib.sh
    live_lock_traps
    live_lock_hold atlassian-live
    just jira-live
    just confluence-live

# **Both importers** against the real systems (issues #509, #510 and #634).
# `crates/knobas-app/tests/estate_live.rs` is the suite and it is two tests:
# the hcloud producer's round trip over the real Hetzner project, and the
# Docker producer against the notebook's engine, the one `testenv/estate.json`
# names.
#
# WHAT IT CERTIFIES, AND WHY IT IS THE ONLY THING THAT CAN. Both producers are
# run against a fake in `just check` (`tests/it/assets_ipc.rs`: a recorded hcloud
# answer, and a stub docker executable), which certifies shape -- the decode,
# the file, the origin-key match, the argument rule, the landing. A recording
# and a stub are green forever, so these facts have no witness but this recipe:
#
#   1. that Hetzner still answers in that shape, and that the docker CLI still
#      writes one JSON object per line;
#   2. that the hcloud importer reads back what it wrote: produce, land what is
#      new at the top of the estate, apply, produce again -- all known, no
#      changes, every server matched by the `hcloud_id` the API returned;
#   3. that the `orbstack` context reaches the engine the estate file says;
#   4. that every **running** container on it is written down in that file.
#
# THE SERVER COUNT IS DECLARED: `ESTATE_LIVE_HCLOUD_SERVERS=<n> just
# estate-live`, required, and a different count is red (the deputy's ruling on
# #634). The knobas Hetzner project is empty once its three servers are
# decommissioned (#634), so an ordinary run is `=0`: a real authenticated
# request, its envelope and its pagination, and proof nothing is left billing.
# The parse of a server needs one to exist: a PR that touches the hcloud
# producer (`crates/knobas-app/src/assets/hcloud.rs` or the client it uses)
# owes a run at `=1`, and that run is Björn's -- he creates the smallest server
# in the knobas project, runs it, and deletes the server. No agent and no recipe
# creates or deletes a server.
#
# **A RED DOCKER RUN NAMING A CONTAINER IS READ FIRST AS `testenv/estate.json`
# BEING BEHIND THE ENGINE**, and only then as a bug in the producer.
#
# NO SSH, NO TUNNEL, NO `hcloud` CLI. The hcloud half is HTTPS `GET
# /v1/servers` with the token in the repo-root `.env`; the Docker half spawns
# `docker --context orbstack ps`, the only context the estate file names. Every
# call is a read; the imports write into scratch databases of the run's own.
#
# The variables are **gated rather than skipped on**, `teamcity-live`'s rule
# and issue #351's: a suite that skipped by name on an unset variable is one
# libtest counts as a pass, and a green line certifying nothing is worse than a
# refusal. The docker CLI is gated the same way and for the same reason.
#
# Serial and unparallelised: two tests, one rate-limiter budget.
estate-live:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f .env ]; then set -a; . ./.env; set +a; fi
    just _require-live-env 'HETZNER_API_TOKEN is in the repo-root .env, which is gitignored:
      cp .env.example .env   # then paste the read-only token of the knobas Hetzner project in
    ESTATE_LIVE_HCLOUD_SERVERS is how many servers that project holds, said on the
    command line -- 0 once the servers are decommissioned (#634):
      ESTATE_LIVE_HCLOUD_SERVERS=0 just estate-live' \
      HETZNER_API_TOKEN ESTATE_LIVE_HCLOUD_SERVERS
    if ! command -v docker >/dev/null 2>&1; then
      echo "error: the docker CLI is not on the PATH." >&2
      echo "  The Docker half of this recipe spawns it against the notebook's engine" >&2
      echo "  (\`docker --context orbstack ps --format json\`), so without it the" >&2
      echo "  suite would refuse the engine and report a fault that is this" >&2
      echo "  machine's rather than the estate's." >&2
      exit 1
    fi
    env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test estate_live \
      -- --ignored --nocapture --test-threads=1

# The generated IPC mirror is current (#1325, contract §2.6 as amended):
# regenerate `app/src/lib/ipc/generated/` from the Rust types and fail when
# the run changed the working tree (`scripts/ipc-mirror-check.sh`: the
# reference is the tree before the run, not HEAD, so a mirror `pre-gate` just
# regenerated is not reported stale). The
# writer is `knobas-app`'s `ipc_mirror` unit test; to regenerate by hand run
# `cargo test -p knobas-app --lib ipc_mirror` and commit the result.
#
# It runs the lib test binary `_test-executables` names, from its package
# directory as `test` does, rather than `cargo test -p knobas-app`: `-p`
# resolves features for one package, so it would build a second copy of the
# crate's test binary beside the workspace one `inventory` has just built
# (25 s, measured 2026-10-19).
ipc-mirror:
    #!/usr/bin/env bash
    set -euo pipefail
    line=$(just _test-executables | awk -F'\t' '$2 == "knobas-app" && $3 == "lib/knobas_app"')
    if [ -z "$line" ]; then
        echo "error: cargo built no knobas-app lib test executable" >&2
        exit 1
    fi
    exe=$(cut -f4 <<< "$line")
    dir=$(cut -f5 <<< "$line")
    scripts/ipc-mirror-check.sh app/src/lib/ipc/generated \
      env -C "$dir" "$exe" --exact ipc_mirror::the_generated_ipc_mirror_is_written --quiet

# Move loose section 10.8 entries into docs/contract/10.8/ and regenerate the index (#737).
contract-index:
    python3 -I scripts/contract-index.py
