#!/usr/bin/env bash
# The desktop witness's own unit tests (issue #500). No screen, no bundle and
# no signing identity, which is why `just check` can carry them and `just
# desktop-witness` cannot. Most of them need nothing but bash; the four at the
# foot compile `ax.swift` and ask the helper its two questions, and skip
# themselves where macOS and `swiftc` are not both present.
#
# What is under test is `desktop-witness-lib.sh`: the path comparison the
# harness makes against Launch Services' answer, the reading of the
# accessibility probe, and the pieces `open-in-editor` is built out of. The
# first two are small enough to look correct and have been wrong in this exact
# shape before -- the notification prototype lost a run to a bundle path that
# differed by where it was copied from, and an unanswered TCC prompt is
# recorded as a *denial*, so "no news" from a probe is the one reading that
# must never come back as "granted".
#
# The probe fixtures below are recorded output from `ax probe` on the dev Mac
# on 2026-09-08, not invented shapes: the `screen-locked` one is what the first
# run of the harness actually printed.

set -euo pipefail
cd "$(dirname "$0")"
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. ./desktop-witness-lib.sh

failures=0
checks=0

# check <what> <expected> <actual>
check() {
    checks=$((checks + 1))
    if [ "$2" = "$3" ]; then return 0; fi
    printf 'desktop-witness-test: FAIL %s\n  expected: %s\n  actual:   %s\n' "$1" "$2" "$3" >&2
    failures=$((failures + 1))
}

# check_matches <what> <expected-outcome: yes|no> <path a> <path b>
check_matches() {
    local outcome=no
    paths_match "$3" "$4" && outcome=yes
    check "$1" "$2" "$outcome"
}

# check_contains <what> <needle> <haystack> -- so that a passing assertion is
# counted like any other. An assertion that only reports itself when it fails
# makes the run's own total move around, which is the one number a reader uses
# to tell a suite that shrank from a suite that passed.
check_contains() {
    local outcome=no
    case $3 in
    *"$2"*) outcome=yes ;;
    esac
    check "$1" yes "$outcome"
}

# --- normalise_app_path -----------------------------------------------------

built=/Users/dev/knobas/target/debug/bundle/macos/knobas.app

check "a plain bundle path is left alone" \
    "$built" "$(normalise_app_path "$built")"
check "Launch Services' trailing slash is dropped" \
    "$built" "$(normalise_app_path "$built/")"
check "several trailing slashes are dropped" \
    "$built" "$(normalise_app_path "$built///")"
check "doubled separators collapse" \
    "$built" "$(normalise_app_path "/Users/dev//knobas/target/debug//bundle/macos/knobas.app")"
check "a root path survives" "/" "$(normalise_app_path /)"

# --- paths_match ------------------------------------------------------------

check_matches "the same path matches itself" yes "$built" "$built"
check_matches "Launch Services' directory URL matches the built path" yes \
    "$built" "$built/"

# The failure this comparison exists for: the copy in /Applications that a
# release install leaves behind, which is what `dev.knobas.desktop` resolved to
# on the dev Mac on 2026-09-08. Launching the debug bundle while *that* is the
# registered one is how the notification prototype got two knobas processes.
check_matches "an installed copy does not match the built bundle" no \
    "$built" "/Applications/knobas.app"

# Two worktrees build the same bundle name at two paths, and the difference is
# in the middle of the string rather than at either end.
check_matches "another worktree's bundle does not match" no \
    "$built" "/Users/dev/knobas/.worktrees/issue-500/target/debug/bundle/macos/knobas.app"

# A prefix is not a match: `.../knobas.app` and `.../knobas.app.old` differ by
# a suffix that a `case`-style prefix test would swallow.
check_matches "a longer path with the same prefix does not match" no \
    "$built" "$built.old"

# `*` is legal in a filename, and an unquoted `[[ == ]]` right-hand side would
# read it as a glob that matches the built path.
check_matches "a wildcard in a path is compared literally, not matched" no \
    "$built" "/Users/dev/knobas/target/debug/bundle/macos/*"

# --- the copies the opt-in step unregisters (#1176) -------------------------
#
# A fake `lsregister -dump`, in the shape the real one prints (path before
# identifier inside a record, records split by dashes, ` (0x..)` after the
# path), with: the bundle under test, the root checkout's debug and release
# bundles, a sibling worktree, an installed copy, a lookalike directory that
# merely shares the root's name as a prefix, and a record of another app.
root=/Users/dev/knobas
under_test=$root/.worktrees/issue-1176/target/debug/bundle/macos/knobas.app
fake_dump="--------------------------------------------------------------------------------
bundle id:                  knobas (0x1)
path:                       $root/target/debug/bundle/macos/knobas.app (0x5c24)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  knobas (0x2)
path:                       $root/target/release/bundle/macos/knobas.app (0x5c25)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  knobas (0x3)
path:                       $under_test (0x5c26)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  knobas (0x4)
path:                       $root/.worktrees/issue-1084/target/debug/bundle/macos/knobas.app (0x5c27)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  knobas (0x5)
path:                       /Applications/knobas.app (0x5c28)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  knobas (0x6)
path:                       ${root}-old/target/knobas.app (0x5c29)
identifier:                 dev.knobas.desktop
--------------------------------------------------------------------------------
bundle id:                  Other (0x7)
path:                       $root/.worktrees/other/Other.app (0x5c2a)
identifier:                 com.example.other
--------------------------------------------------------------------------------
bundle id:                  NoIdentifier (0x8)
path:                       $root/.worktrees/x/Odd.app (0x5c2b)
"

check "only this repo's other copies are unregistered, never the bundle under test" \
    "$root/.worktrees/issue-1084/target/debug/bundle/macos/knobas.app
$root/target/debug/bundle/macos/knobas.app
$root/target/release/bundle/macos/knobas.app" \
    "$(copies_to_unregister "$fake_dump" dev.knobas.desktop "$under_test" "$root")"
check "an installed copy and a lookalike prefix are left, and reported" \
    "${root}-old/target/knobas.app
/Applications/knobas.app" \
    "$(copies_to_leave "$fake_dump" dev.knobas.desktop "$under_test" "$root" | sort -r)"
check "a Launch Services trailing slash on the bundle under test still keeps it" \
    "no" \
    "$(copies_to_unregister "$fake_dump" dev.knobas.desktop "$under_test/" "$root" | grep -q issue-1176 && echo yes || echo no)"
check "a dump with no copy of the identifier selects nothing" \
    "" "$(copies_to_unregister "$fake_dump" dev.knobas.nothing "$under_test" "$root")"

# --- classify_readiness -----------------------------------------------------

granted=$'trusted\t1\npost-events\t1\nscreen-locked\t0'
locked=$'trusted\t1\npost-events\t1\nscreen-locked\t1'
untrusted=$'trusted\t0\npost-events\t1\nscreen-locked\t0'
no_events=$'trusted\t1\npost-events\t0\nscreen-locked\t0'

check "a drivable desktop reads as granted" granted "$(classify_readiness "$granted")"
check "a locked screen is named" screen-locked "$(classify_readiness "$locked")"
check "a terminal without Accessibility is named" not-trusted "$(classify_readiness "$untrusted")"
check "a process that cannot post events is named" cannot-post-events \
    "$(classify_readiness "$no_events")"

# Precedence: a Mac that is both untrusted and locked is reported as untrusted,
# because granting Accessibility is the thing to do first and the lock will be
# gone by the time anyone reads the second message.
check "an untrusted terminal outranks a locked screen" not-trusted \
    "$(classify_readiness $'trusted\t0\npost-events\t0\nscreen-locked\t1')"

# The reading that must never be `granted`. A helper that failed to compile,
# an empty pipe, or a probe whose output shape drifted all arrive here.
check "empty probe output is unreadable, not granted" unreadable "$(classify_readiness "")"
check "a probe missing one field is unreadable" unreadable \
    "$(classify_readiness $'trusted\t1\npost-events\t1')"
check "a probe with the wrong field names is unreadable" unreadable \
    "$(classify_readiness $'AXIsProcessTrusted\t1\npost\t1\nlocked\t0')"
# Space-separated rather than tab-separated: the shape this would drift into
# if the helper's `print` ever lost its `\t`.
check "a probe that lost its tabs is unreadable" unreadable \
    "$(classify_readiness 'trusted 1
post-events 1
screen-locked 0')"

# --- readiness_message ------------------------------------------------------

for classification in not-trusted cannot-post-events screen-locked unreadable; do
    check_contains "the $classification message names the README section" \
        'testenv/README.md, "The desktop witness (macOS)"' \
        "$(readiness_message "$classification")"
done

# Each refusal names the permission by the string somebody has to search for
# in System Settings. "Permission denied" would send a reader to the wrong
# pane: two TCC grants are in play, and this witness needs Accessibility and
# deliberately avoids Automation.
check_contains "the not-trusted message names Accessibility" \
    "Accessibility" "$(readiness_message not-trusted)"
check_contains "the screen-locked message says the screen is locked" \
    "screen is locked" "$(readiness_message screen-locked)"
check_contains "the cannot-post-events message names keyboard events" \
    "post keyboard events" "$(readiness_message cannot-post-events)"

# --- tsv_field --------------------------------------------------------------

probe_output=$'trusted\t1\npost-events\t0\nscreen-locked\t1'
check "a field's value is read" 1 "$(tsv_field "$probe_output" trusted)"
check "a later field's value is read" 1 "$(tsv_field "$probe_output" screen-locked)"
check "an absent field is empty" "" "$(tsv_field "$probe_output" nonesuch)"
# An accessibility description is a sentence, which is the reason the helper
# answers in tab-separated lines at all.
check "a value containing spaces survives" "Search or act" \
    "$(tsv_field $'role\tAXTextField\ndescription\tSearch or act' description)"
check "an empty value reads as empty, not as the next line" "" \
    "$(tsv_field $'description\t\nrole\tAXTextField' description)"

# --- the label the driver asserts on ---------------------------------------

# The driver reads the launcher's box out of the accessibility tree by the
# `aria-label` on it. Nothing in the app knows the driver exists, so an edit
# to that attribute would leave a witness that fails against a correct app --
# and the failure would arrive on somebody's screen, minutes into a run, with
# no clue in it. This is the cheap pin, in the shape the repo already uses for
# a value mirrored across two files.
query_box=../app/src/lib/launcher/QueryBox.svelte
if grep -q 'aria-label="Search or act"' "$query_box"; then
    check "the launcher's query box still carries the label the driver asserts on" yes yes
else
    check "the launcher's query box still carries the label the driver asserts on" yes no
    printf '  %s no longer has aria-label="Search or act";\n' "$query_box" >&2
    printf '  testenv/desktop-witness/drivers/launcher-hotkey.sh asserts it.\n' >&2
fi

# --- the process name the harness counts on --------------------------------

# `desktop-witness.sh` finds the running app with `pgrep -x knobas-app`, and
# that name is the cargo binary name rather than the bundle's `productName`
# ("knobas"). If it drifted, the harness's "no knobas is already running"
# check would pass on a machine with one running and its single-instance
# assertion would never see a second -- both vacuous, both green. The same
# cheap pin as the label above, in the other direction: a false pass rather
# than a false failure.
manifest=../crates/knobas-app/Cargo.toml
if grep -q '^name = "knobas-app"$' "$manifest"; then
    check "the app crate still builds a binary called knobas-app" yes yes
else
    check "the app crate still builds a binary called knobas-app" yes no
    printf '  %s no longer declares name = "knobas-app";\n' "$manifest" >&2
    printf '  testenv/desktop-witness.sh counts processes by that name.\n' >&2
fi

# --- the open-in-editor driver's pure parts (#501) --------------------------

record='/tmp/knobas witness/record'
check "the stub records one argument per line, into the path it was given" \
    "$(printf '#!/bin/sh\nprintf %s "$@" > %s\nmv %s %s\n' "'%s\\n'" \
        "'$record.tmp'" "'$record.tmp'" "'$record'")" \
    "$(stub_script "$record")"
# The record path is single-quoted in the script, so a directory with a space
# in it -- which `mktemp -d` under a Mac's TMPDIR routinely is not, and a
# worktree under "My Code" routinely is -- stays one word.
check_contains "the recorded path is quoted in the script" \
    "mv '$record.tmp' '$record'" "$(stub_script "$record")"

# --- the path the driver types into a command template (#547) ---------------
#
# A driver may not type a quote: macOS rewrites `"` into `“` and `”` on its way
# into a WebKit field, and the template that reaches the database then names a
# program nothing can start. So the stub's path goes in unquoted, and whether
# it *can* is checked before it is typed rather than discovered as a spawn that
# failed.

# check_one_word <what> <yes|no> <path>
check_one_word() {
    local outcome=no
    path_is_one_word "$3" && outcome=yes
    check "$1" "$2" "$outcome"
}

check_one_word "a scratch path with nothing special in it is one word" yes \
    "/var/folders/ph/p63g3rb537b55_4cf_536s7r0000gn/T/knobas-open-in-editor.HKDuLm/stub"
# What `mktemp -d` answers under a TMPDIR with a space, which is the case the
# quotes were there for and is now a refusal with a reason instead.
check_one_word "a path with a space in it is not one word" no \
    "/Users/mara/My Code/knobas-open-in-editor.HKDuLm/stub"
check_one_word "a path carrying a double quote is not one word" no \
    '/tmp/knobas-"witness"/stub'
check_one_word "a path carrying a single quote is not one word" no \
    "/tmp/mara's-witness/stub"
# A tab is whitespace too, and it is the one a `case` written with a literal
# space would miss.
check_one_word "a path with a tab in it is not one word" no \
    "$(printf '/tmp/knobas\twitness/stub')"
check_one_word "no path at all is not one word" no ""

config=$(git_config "https://tidewater.example/tidewater/payout-service")
# The section header git itself writes, and the one
# `knobas_core::checkout::origin_url` looks for. A config naming the remote
# under any other section would make the scan miss the clone the driver put
# there, and the driver would report a *no checkout* as a failure of the
# button.
check_contains "the config names the origin remote the scan reads" \
    '[remote "origin"]' "$config"
check_contains "the config carries the remote it was given" \
    "url = https://tidewater.example/tidewater/payout-service" "$config"

check "one recorded argument is the answer" "/Users/mara/src/payout-service" \
    "$(sole_argument "/Users/mara/src/payout-service")"
# The two answers that must be empty, and the reason the function exists: a
# reader that took the first line would pass on a command that was handed the
# checkout *and something else*, which is the failure ADR-0016 is about.
check "two recorded arguments are not a sole argument" "" \
    "$(sole_argument "$(printf '%s\n%s' --wait /Users/mara/src/payout-service)")"
check "no recorded argument is not a sole argument" "" "$(sole_argument "")"
check "a recorded path containing a space survives whole" "/Users/mara/My Code/payout service" \
    "$(sole_argument "/Users/mara/My Code/payout service")"
check "trailing blank lines are not a second argument" "/src/x" \
    "$(sole_argument "$(printf '/src/x\n\n')")"

# --- the wait the driver breaks on (#531) -----------------------------------
#
# The driver polls for the stub's output, and until this ticket it broke on
# `[ -s "$record" ]` -- any non-empty file. Its Rust twin, `recorded()` in
# `crates/knobas-app/tests/it/checkout_ipc.rs`, waits for the trailing newline,
# which a one-line write has when it has *finished*; that a two-line one has
# finished too is the stub's rename (#583). A wait on non-emptiness can hand the
# comparison half a line, and half a line is reported as *the command was not
# given the checkout path*: the feature blamed for a race in the harness, on
# the one run the witness exists for.
#
# So the condition itself is what is checked here, on real files, rather than
# the driver's spelling of it: the driver calls `wait_for_record` and holds no
# copy of the rule.

# check_is_record <what> <yes|no> <path>
check_is_record() {
    local outcome=no
    record_is_complete "$3" && outcome=yes
    check "$1" "$2" "$outcome"
}

# check_waits <what> <yes|no> <path> <seconds>
check_waits() {
    local outcome=no
    wait_for_record "$3" "$4" && outcome=yes
    check "$1" "$2" "$outcome"
}

# Removed at the foot of the section, and by a trap until then: `check` never
# exits, but `wait` on the background writer below does under `set -e`, and
# that is an exit between the `mktemp` and the `rm -rf`. The helper section at
# the foot of this file replaces this trap with its own, by which point this
# directory is already gone.
records=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-records.XXXXXX")
trap 'rm -rf "$records"' EXIT

printf '/Users/mara/src/payout-service\n' >"$records/whole"
printf '/Users/mara/src/payout-serv' >"$records/half"
: >"$records/empty"

check_is_record "a record that ends in a newline is a record" yes "$records/whole"
# The whole ticket, in one line.
check_is_record "a record without its trailing newline is not yet a record" no \
    "$records/half"
# `tail -c 1` of an empty file prints nothing, exactly as it does for a file
# ending in a newline -- so this is the check that keeps the `-s` in.
check_is_record "the empty file the stub's redirect opens is not a record" no \
    "$records/empty"
check_is_record "a path with nothing at it is not a record" no "$records/absent"
# Two arguments, and a path with a space in it: the condition reads where a
# record ends, not what it says. (That a two-line record ending in a newline is
# *all* of it is the stub's rename, not this condition: #583.)
printf '%s\n%s\n' --wait '/Users/mara/My Code/payout service' >"$records/two"
check_is_record "a two-line record that ends in a newline is a record" yes \
    "$records/two"

# A budget of 0 is one look, and it is how the two checks below stay instant.
check_waits "the wait answers at once for a record already on disk" yes \
    "$records/whole" 0
check_waits "the wait gives up when nothing is ever written" no \
    "$records/absent" 0
check_waits "the wait gives up on a record that never finished" no \
    "$records/half" 0

# And that it is a *wait*: a record written after the polling starts is waited
# for, and what the caller then reads is the whole of it. The writer leaves
# half a line on disk first, which is the state the old condition would have
# returned.
late=$records/late
(
    printf '/Users/mara/src/payout-serv' >"$late"
    sleep 0.6
    printf '/Users/mara/src/payout-service\n' >"$late"
) &
writer=$!
if wait_for_record "$late" 10; then
    check "a record still being written is waited out, and read whole" \
        "/Users/mara/src/payout-service" "$(cat "$late")"
else
    check "a record still being written is waited out, and read whole" \
        "/Users/mara/src/payout-service" "(the wait gave up after 10 s)"
fi
wait "$writer"

rm -rf "$records"

# --- witness_guard, witness_poll (#1961) ---------------------------------------
#
# Every wait on the app polls for the state it needs under one guard sized past
# a loaded machine, and fails at once when the app has exited. The drivers,
# and live-shot's steps wait through both; the harness reads the guard for its
# own wait on the app's process.

check "the guard defaults to 60 s" 60 "$(env -u KNOBAS_WITNESS_WAIT_S bash -c '. ./desktop-witness-lib.sh; witness_guard')"
check "the guard takes whole seconds from KNOBAS_WITNESS_WAIT_S" 7 \
    "$(KNOBAS_WITNESS_WAIT_S=7 bash -c '. ./desktop-witness-lib.sh; witness_guard')"
for bad in 0 1.5 10s -3 07; do
    said=$(KNOBAS_WITNESS_WAIT_S=$bad bash -c '. ./desktop-witness-lib.sh; witness_guard' 2>&1 && echo "status 0" || echo "status $?")
    check_contains "the guard refuses '$bad', naming the variable" "KNOBAS_WITNESS_WAIT_S must be a whole number of seconds, got '$bad'" "$said"
    check_contains "and exits 2 for '$bad'" "status 2" "$said"
done

# A state that arrives late is waited for: the guard, not a count of tries,
# is what runs out (the old fixed budgets were 5 s and 8 s in the drivers and
# 10 s in live-shot's reach and the harness's launch).
poll_tmp=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-poll.XXXXXX")
sleep 600 &
alive=$!
(sleep 2; : >"$poll_tmp/drawn") &
drawer=$!
status=0
witness_poll "$alive" 30 test -e "$poll_tmp/drawn" || status=$?
check "a state 2 s late is waited for" 0 "$status"
wait "$drawer"
# One that never comes runs out on the guard, and says so (1).
status=0
witness_poll "$alive" 2 test -e "$poll_tmp/never" || status=$?
check "a state that never comes runs out on the guard" 1 "$status"
kill "$alive" 2>/dev/null || true
wait "$alive" 2>/dev/null || true
# An app that has exited fails the wait at once (3), not at the guard.
sleep 1 &
gone=$!
wait "$gone" || true
from=$SECONDS status=0
witness_poll "$gone" 30 test -e "$poll_tmp/never" || status=$?
check "an exited app fails the wait" 3 "$status"
check "and fails it at once" yes "$([ $((SECONDS - from)) -lt 5 ] && echo yes || echo "no ($((SECONDS - from)) s)")"
# A state already there answers even when the app has just gone: what was
# waited for happened.
: >"$poll_tmp/drawn"
status=0
witness_poll "$gone" 30 test -e "$poll_tmp/drawn" || status=$?
check "a state already reached answers whatever the app does next" 0 "$status"
rm -rf "$poll_tmp"

# --- profile_postmaster, stop_left_postgres (#1967) ----------------------------
#
# A run that cannot quit the app it launched kills it, and a killed knobas runs
# no exit handler: its embedded PostgreSQL stays up on the profile's data
# directory. Every script that launches the app stops what it left. The
# stand-ins are the shapes checked against the real 18.6 binaries on
# 2026-10-24: `pg_ctl start` leaves a postmaster whose parent is pid 1, whose
# command line is `<bin>/postgres -D <data dir> ...`, which writes its pid on
# the first line of `<data dir>/postmaster.pid` and removes the file on a fast
# shutdown (SIGINT, what `pg_ctl stop -m fast` sends).
# Canonical, as `pg_ctl` would hand it on (a `$TMPDIR` ending in `/` doubles it).
pg_tmp=$(pg_path "$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-pg.XXXXXX")")
mkdir -p "$pg_tmp/bin" "$pg_tmp/other"
pg_data="$pg_tmp/Application Support/db/data"
mkdir -p "$pg_data"
# postgres -D <data dir>: a postmaster. PG_STANDIN=fast (the default) stops on
# SIGINT, deaf only on SIGQUIT (immediate), stuck on neither.
cat >"$pg_tmp/bin/postgres" <<'EOF'
#!/usr/bin/env bash
data=$2
case ${PG_STANDIN:-fast} in
fast) trap 'rm -f "$data/postmaster.pid"; exit 0' INT ;;
deaf) trap '' INT; trap 'rm -f "$data/postmaster.pid"; exit 0' QUIT ;;
stuck) trap '' INT QUIT ;;
esac
printf '%s\n%s\n' "$$" "$data" >"$data/postmaster.pid"
while :; do sleep 0.1; done
EOF
# initdb --pgdata <data dir>: what a killed app leaves in flight, which goes
# on to start a postmaster 2 s later (a `pg_ctl start` the app had begun).
cat >"$pg_tmp/bin/initdb" <<'EOF'
#!/usr/bin/env bash
[ "${PG_INITDB:-}" != hang ] || while :; do sleep 0.1; done
sleep 2
set -m
"$(dirname "$0")/postgres" -D "$2" >/dev/null 2>&1 &
echo $! >"$2/../started"
until [ -s "$2/postmaster.pid" ]; do sleep 0.1; done
EOF
chmod +x "$pg_tmp/bin/postgres" "$pg_tmp/bin/initdb"
# A background job of a script starts with SIGINT ignored, which no trap can
# undo; job control gives the stand-in the default a real postmaster sets.
postmaster_standin() { # <data dir> [mode]: starts one, prints its pid
    set -m
    PG_STANDIN=${2:-fast} "$pg_tmp/bin/postgres" -D "$1" >/dev/null 2>&1 &
    set +m
    local pid=$!
    until [ -s "$1/postmaster.pid" ]; do sleep 0.1; done
    printf '%s\n' "$pid"
}
alive() { kill -0 "$1" 2>/dev/null && echo alive || echo gone; }

check "no postmaster.pid: no postmaster" "" "$(profile_postmaster "$pg_data")"
said=$(stop_left_postgres "$pg_data" "" 5 2>&1) && status=0 || status=$?
check "no postmaster.pid: nothing to stop" 0 "$status"
check "and nothing said" "" "$said"

pid=$(postmaster_standin "$pg_data")
check "a live postmaster on the data dir is found" "$pid" "$(profile_postmaster "$pg_data")"
# `pg_ctl` canonicalises the directory before the postmaster gets it (checked
# against 18.6: `-D .../T//x` runs as `postgres -D .../T/x`), so a caller's
# spelling with a doubled or trailing slash finds the same server.
check "under a doubled and a trailing slash too" "$pid" "$(profile_postmaster "${pg_tmp}//Application Support/./db/data/")"
check "pg_path spells a path as pg_ctl does" "/a/b/c" "$(pg_path "/a//b/./c/")"
check "pg_path leaves the root alone" "/" "$(pg_path "/")"
from=$SECONDS
said=$(stop_left_postgres "$pg_data" "" 10 2>&1) && status=0 || status=$?
check "the postmaster the app left is stopped" 0 "$status"
# With nothing in flight the stop does not wait for the guard: the postmaster
# itself is not a start in flight (the real one stops in a quarter second).
check "at once" yes "$([ $((SECONDS - from)) -lt 5 ] && echo yes || echo "no ($((SECONDS - from)) s)")"
check "it is gone" gone "$(alive "$pid")"
check "its lock is gone" no "$([ -e "$pg_data/postmaster.pid" ] && echo yes || echo no)"
check_contains "and the stop says which" "stopped the PostgreSQL the app left on $pg_data (pid $pid, SIGINT)" "$said"

# Running before the launch: not this run's, so it is left as it was, even
# though the app adopted it.
pid=$(postmaster_standin "$pg_data")
said=$(stop_left_postgres "$pg_data" "$pid" 5 2>&1) && status=0 || status=$?
check "a postmaster running before the launch is not stopped" "0 alive" "$status $(alive "$pid")"
check_contains "and the run says it left it" "left alone: pid $pid was serving $pg_data before this run" "$said"
kill -INT "$pid"; wait "$pid" 2>/dev/null || true

# A postmaster.pid whose pid is gone, or now names another process (a pid
# recycled after a reboot, another cluster's postgres): nothing to stop.
printf '999999\n%s\n' "$pg_data" >"$pg_data/postmaster.pid"
check "a dead pid is no postmaster" "" "$(profile_postmaster "$pg_data")"
sleep 600 &
stranger=$!
printf '%s\n%s\n' "$stranger" "$pg_data" >"$pg_data/postmaster.pid"
check "a recycled pid is no postmaster" "" "$(profile_postmaster "$pg_data")"
stop_left_postgres "$pg_data" "" 5 >/dev/null 2>&1 && status=0 || status=$?
check "and is not signalled" "0 alive" "$status $(alive "$stranger")"
kill "$stranger"; wait "$stranger" 2>/dev/null || true
other=$(postmaster_standin "$pg_tmp/other")
cp "$pg_tmp/other/postmaster.pid" "$pg_data/postmaster.pid"
check "another data dir's postmaster is no postmaster of this one" "" "$(profile_postmaster "$pg_data")"
check "nor a start in flight on it" "" "$(starts_in_flight "$pg_data" "")"
check "while it is one on its own" "$other" "$(starts_in_flight "$pg_tmp/other" "")"
stop_left_postgres "$pg_data" "" 5 >/dev/null 2>&1 || true
check "and is not signalled" alive "$(alive "$other")"
kill -INT "$other"; wait "$other" 2>/dev/null || true
rm -f "$pg_data/postmaster.pid"

# One that ignores the fast shutdown gets the immediate one.
pid=$(postmaster_standin "$pg_data" deaf)
said=$(stop_left_postgres "$pg_data" "" 2 2>&1) && status=0 || status=$?
check "a postmaster deaf to SIGINT is stopped" "0 gone" "$status $(alive "$pid")"
check_contains "with SIGQUIT" "(pid $pid, SIGQUIT)" "$said"
# One that stops for neither is a failure that names it, not a silent pass.
pid=$(postmaster_standin "$pg_data" stuck)
said=$(stop_left_postgres "$pg_data" "" 1 2>&1) && status=0 || status=$?
check "a postmaster that will not stop fails the step" 1 "$status"
check_contains "naming it and the hand stop" "still running on $pg_data (pid $pid): pg_ctl -D" "$said"
kill -KILL "$pid"; wait "$pid" 2>/dev/null || true
rm -f "$pg_data/postmaster.pid"

# The app killed mid bring-up: what it started goes on without it, and the
# postmaster it brings up is waited for and stopped, not missed.
set -m
"$pg_tmp/bin/initdb" --pgdata "$pg_data" >/dev/null 2>&1 &
set +m
starter=$!
from=$SECONDS
said=$(stop_left_postgres "$pg_data" "" 20 2>&1) && status=0 || status=$?
check "a start in flight is waited out" yes "$([ $((SECONDS - from)) -ge 2 ] && echo yes || echo "no ($((SECONDS - from)) s)")"
check "and the postmaster it started is stopped" "0 gone gone" \
    "$status $(alive "$starter") $(alive "$(cat "$pg_data/../started")")"
check "its lock is gone too" no "$([ -e "$pg_data/postmaster.pid" ] && echo yes || echo no)"
check_contains "and the stop says which" "stopped the PostgreSQL the app left" "$said"
# A start that never finishes outlasts the guard: a failure that names it,
# since the server it may still bring up would be left running.
set -m
PG_INITDB=hang "$pg_tmp/bin/initdb" --pgdata "$pg_data" >/dev/null 2>&1 &
set +m
starter=$!
until [ -n "$(starts_in_flight "$pg_data" "")" ]; do sleep 0.1; done
said=$(stop_left_postgres "$pg_data" "" 1 2>&1) && status=0 || status=$?
check "a start that outlasts the guard fails the step" 1 "$status"
check_contains "naming what is still starting" "still starting on $pg_data" "$said"
kill -KILL "$starter" 2>/dev/null || true
wait "$starter" 2>/dev/null || true

check "the demo profile's data dir" \
    "/Users/mara/Library/Application Support/dev.knobas.desktop/demo/db/data" "$(demo_data_dir /Users/mara)"
# That path is three files' layout; a drift would make every stop look in an
# empty directory and pass.
if grep -q '"identifier": "dev.knobas.desktop"' ../crates/knobas-app/tauri.conf.json &&
    grep -q 'app_data_dir.join("demo")' ../crates/knobas-app/src/profile.rs &&
    grep -q 'self.dir.join("db")' ../crates/knobas-app/src/profile.rs &&
    grep -q 'settings.data_dir = root_dir.join("data");' ../crates/knobas-db/src/embedded.rs; then
    check "the app still keeps the demo database where demo_data_dir looks" yes yes
else
    check "the app still keeps the demo database where demo_data_dir looks" yes no
fi
# Whatever a failing check above left running: only this test's stand-ins.
pkill -KILL -f -- "$pg_tmp/bin/" 2>/dev/null || true
rm -rf "$pg_tmp"

# The twin, pinned in the direction #531's divergence ran: two files writing
# and waiting on one stub's record, and only a reader of both can see them
# disagree. What makes a record whole on both sides is the rename (#583), so
# that is what is pinned: the spelling of the Rust stub's rename, which is all
# a shell test can see of it -- the same cheap cross-file pin as the labels
# below, and for the same reason.
twin=../crates/knobas-app/tests/it/checkout_ipc.rs
if grep -qF "mv '{0}.tmp' '{0}'" "$twin"; then
    check "the Rust twin still renames its record into place" yes yes
else
    check "the Rust twin still renames its record into place" yes no
    printf "  %s no longer contains mv '{0}.tmp' '{0}' anywhere;\n" "$twin" >&2
    printf "  recording_stub's rename is what that spelling stands for, and\n" >&2
    printf "  stub_script's is the shell copy of it (#583).\n" >&2
fi

# --- the accessible names the open-in-editor driver acts on -----------------
#
# The same cheap pin #500 put on the launcher's `aria-label`, in the same
# direction: nothing in the app knows a driver exists, so an edit to one of
# these would fail a *correct* app minutes into a run with no clue in it.

pin_label() {
    if grep -q "$2" "$1"; then
        check "$3" yes yes
    else
        check "$3" yes no
        printf '  %s no longer carries %s;\n' "$1" "$2" >&2
        printf '  a driver under testenv/desktop-witness/drivers/ presses it by name.\n' >&2
    fi
}

# Ported to the ticket-first shell by #1084: Settings › Checkouts and Open in…
# (ADR-0023's clones root and the reader's own applications) and the job's
# Code section with its *Open in…* menu. The new shell uppercases none of these,
# so each is pinned as the driver spells it.
tf=../app/src/lib/tf
pin_label $tf/TfTopBar.svelte 'aria-label="Settings"' \
    "the top bar's Settings button still has the name both drivers press"
pin_label $tf/settings/settings.ts '"Checkouts and Open in…"' \
    "Settings still lists the section the open-in-editor driver opens"
pin_label $tf/settings/TfCheckoutsSection.svelte '>Change…</button>' \
    "the clones root row still offers Change…"
pin_label $tf/settings/TfCheckoutsSection.svelte 'aria-label="Clones root"' \
    "the clones root field still has its accessible name"
pin_label $tf/settings/TfCheckoutsSection.svelte 'onclick={() => void saveTyped()}>Save</button>' \
    "the clones root field is still stored by a button called Save"
pin_label $tf/settings/TfCheckoutsSection.svelte '>Edit applications…</button>' \
    "the Open in… table still offers Edit applications…"
pin_label $tf/openin/TfAppsManage.svelte '>Add an application…</button>' \
    "the applications dialog still offers Add an application…"
pin_label $tf/openin/TfAppsManage.svelte 'onclick={onclose}>Done</button>' \
    "the applications dialog still closes on Done"
pin_label $tf/openin/TfAppDialog.svelte '<label for="tf-app-name">Name</label>' \
    "the application dialog's name field is still labelled Name"
pin_label $tf/openin/TfAppDialog.svelte '<label for="tf-app-command">Command</label>' \
    "the application dialog's command field is still labelled Command"
pin_label $tf/openin/TfAppDialog.svelte '"Add application"' \
    "the application dialog still adds with a button called Add application"
pin_label $tf/openin/TfOpenIn.svelte '}}>Open in… <span class="tf-dim" aria-hidden="true">▾</span></button' \
    "the checkout's menu button is still named Open in…, its arrow hidden"
pin_label $tf/openin/TfOpenIn.svelte 'role="menuitem" onclick={() => run(' \
    "the menu's applications are still menu items, which the driver walks to"
pin_label $tf/repo/TfRepo.svelte 'Cloned at <span class="tf-mono">{sentence.path}</span>' \
    "the repository page still draws the clone's path as a reading of its own"
pin_label $tf/repo/TfRepo.svelte '<TfOpenIn entityId={repoId}' \
    "the repository page still offers the Open in… the driver presses"
# **What a pin cannot say, which is #547's whole finding.** A pin of a source
# string measures that the driver's constant is not stale; it measures nothing
# whatever about what the accessibility API answers, and the only thing that
# does is a run.
#
# And the remote the driver writes into its fake clone: the scan matches it
# against the demo repo's own URL, which this constant is the stem of.
pin_label ../crates/knobas-source-mock/src/lib.rs 'https://tidewater.example' \
    "the demo corpus still lives under the host the driver's clone points at"

# --- names are rendered text, and the new shell transforms none ------------
#
# Measured on the dev Mac on 2026-09-08: WebKit names an element by its
# **rendered** text, so the rooms UI's `.lab` labels arrived upper case and a
# `rendered_label` helper spelled them so. Neither driver reads a transformed
# name since #1084: `tf.css` uppercases nothing, and this pin is what says so
# if it ever starts -- every constant in both drivers would then be stale.
# Exit 1 only: grep's 2 (a missing or unreadable tf.css) must not read as
# "nothing uppercases".
transform=0
grep -q 'text-transform' ../app/src/lib/tf/tf.css || transform=$?
if [ "$transform" = 1 ]; then
    check "tf.css uppercases nothing the drivers name" yes yes
else
    check "tf.css uppercases nothing the drivers name" yes no
    printf '  app/src/lib/tf/tf.css has a text-transform or cannot be read (grep %s);\n' "$transform" >&2
    printf '  the drivers spell names as the markup does.\n' >&2
fi

# --- the capture driver's pure parts (#503) ---------------------------------

check "the title is the first line" "Retry storm" \
    "$(capture_title "$(printf 'Retry storm\nthe queue backs up')")"
check "a one-line capture is all title" "Ask Mara about the cutover" \
    "$(capture_title "Ask Mara about the cutover")"
# The three cases the frontend's `split` also carries, because the driver types
# a paragraph at a real window and has to know what the note will be called; two
# implementations of one rule are two chances to drift, and these are what make
# the drift a red gate rather than a run that fails minutes in.
check "a leading blank line is skipped rather than becoming the title" "Retry storm" \
    "$(capture_title "$(printf '\n\n  Retry storm  \nthe queue backs up')")"
check "surrounding whitespace is not part of the title" "Retry storm" \
    "$(capture_title "   Retry storm   ")"
check "nothing but whitespace is no title" "" "$(capture_title "$(printf '   \n\t\n')")"

# `has_reading` matches a **whole** line, and these two cases are the whole
# reason it exists: every reading a capture draws has a longer one containing
# it, and the longer one is what the *other* end of the same link reads. A
# substring test would let a driver standing on the context's panel report the
# note's.
readings=$(printf 'Linked items\ncaptured from here\nSEPA migration\n')
check_matches_reading() {
    local outcome=no
    has_reading "$3" "$2" && outcome=yes
    check "$1" "$4" "$outcome"
}
check_matches_reading "a whole line is found" "captured from here" "$readings" yes
check_matches_reading "a line that only contains the reading is not a match" \
    "captured from" "$readings" no
check_matches_reading "a reading nothing on screen carries is not a match" \
    "captured in" "$readings" no
check_matches_reading "an indented line still matches" "captured in" \
    "$(printf '   captured in   \n')" yes

# `notes_total` (#1084) reads the note count off Settings › Notes and capture,
# which is where *Open in knobas* lands while notes hide (D10): the driver reads
# it before the shortcut and after the button, and the second must be one more.
# The lines are `notesLine`'s own words (`tf/settings/notes.ts`).
check "no notes is zero" 0 "$(notes_total "$(printf 'Your notes\nNo notes\n')")"
check "one note, with its date" 1 "$(notes_total "$(printf 'Your notes\n1 note, from 18 October\n')")"
check "many notes, with the last one's date" 37 \
    "$(notes_total "$(printf 'Your notes\n37 notes, the last from 12 August 2025\n')")"
check "a count with no date" 2 "$(notes_total "$(printf '2 notes\n')")"
# The refusals: `x`, never 0, because 0 is a count the driver compares against.
check "a screen with no notes line is unreadable, not zero" x \
    "$(notes_total "$(printf 'Your notes\nExport notes as Markdown\nMove 3 notes\n')")"
check "an empty screen is unreadable" x "$(notes_total "")"
check "a sentence that only contains *No notes* is not the count" 3 \
    "$(notes_total "$(printf 'No notes were moved\n3 notes, from 2 May\n')")"
check "two notes lines are ambiguous, not the first" x \
    "$(notes_total "$(printf '1 note, from 1 May\n4 notes\n')")"

# --- what makes a wait a witness (#547) -------------------------------------
#
# `open-in-editor` waited for a *name* the accessibility API never answers with
# -- `<span class="lab">Checkout</span>` names nothing and carries its words in
# `AXValue` -- and reported a branch detail that was open on screen, with its
# three buttons on it, as never having opened. The fix reads the heading as a
# reading, and reads it **twice**: once before the Return that is supposed to
# produce it and once after. These are the two decisions in that, which is all
# of it that can be tested without a screen.

# The values fixture is the shape `ax values` answers in: one string per line,
# empty ones dropped. The `CHECKOUTS` line is the settings pane's own section
# heading, which is on screen at the moment the before-reading is taken, and it
# is one character away from the panel's.
launcher_up=$(printf 'All work\nCHECKOUTS\nDIRECTORY\n3 matches\npayout-service\n')
detail_up=$(printf 'All work\nCHECKOUT\n/tmp/clones/payout-service\nLINKED ITEMS\n')

check "the panel's heading is not on screen while the launcher's list is" no \
    "$(reading_verdict "$launcher_up" CHECKOUT)"
check "the panel's heading is on screen once the detail is open" yes \
    "$(reading_verdict "$detail_up" CHECKOUT)"
# The settings pane's `CHECKOUTS` must not answer for the panel's `CHECKOUT`:
# `has_reading` matches whole lines, and this is the case that says so in the
# direction this driver runs.
check "the settings pane's CHECKOUTS is not the panel's CHECKOUT" no \
    "$(reading_verdict "$(printf 'CHECKOUTS\n')" CHECKOUT)"
# The reading that must never be `no`. `ax values` prints nothing when it can
# see no window and exits 0 anyway, so an empty answer read as "absent" would
# make the before-half of every waypoint pass on a helper that had stopped
# working -- a check that cannot fail.
check "an empty answer is unreadable, not an absent heading" unreadable \
    "$(reading_verdict "" CHECKOUT)"

check "absent before the step and present after it is a witness" witnessed \
    "$(waypoint_verdict no yes)"
# The green that would witness nothing, and the reason this function exists: a
# wait already satisfiable before the keystroke passes just as well on a
# keystroke that did nothing.
check "a reading that was already there witnesses nothing" too-early \
    "$(waypoint_verdict yes yes)"
check "a reading that was there and went away is still too early" too-early \
    "$(waypoint_verdict yes no)"
check "a reading that never arrived is named as that" never-appeared \
    "$(waypoint_verdict no no)"
# Precedence, in the direction that matters: an unreadable *after* cannot
# rescue a before-reading that had already broken the witness.
check "too early outranks an unreadable after-reading" too-early \
    "$(waypoint_verdict yes unreadable)"
check "an unreadable before-reading is not a witness" unreadable \
    "$(waypoint_verdict unreadable yes)"
check "an unreadable after-reading is not a witness" unreadable \
    "$(waypoint_verdict no unreadable)"
# A word neither function ever produces must not fall through to `witnessed`.
check "a verdict this rule does not know is unreadable, not a pass" unreadable \
    "$(waypoint_verdict no maybe)"

# --- counting windows rather than names (#548) ------------------------------
#
# `capture.sh` waited for `exactly_one` element called `Capture`, and the
# capture window carries that word on three at once -- its `AXWindow` title,
# its `AXWebArea` description and its `AXTextArea` name, from three files, each
# of them right. The count was 3 for as long as the window was up, so the
# driver refused the window it had just opened. These are the two decisions in
# the fix, which is all of it that can be tested without a screen.
#
# The fixtures are the shape `ax windows` answers in: one line per window, the
# key repeated, the title in the second tab-separated field. The two-window one
# is what the dump on #548 measured -- the capture window and the main window,
# in that order.

capture_up=$(printf 'window\tCapture\nwindow\tknobas\n')
main_only=$(printf 'window\tknobas\n')

check "the capture window is counted while it is open" 1 \
    "$(window_count "$capture_up" Capture)"
check "no window is titled Capture before the shortcut" 0 \
    "$(window_count "$main_only" Capture)"
check "the main window answers to its own title" 1 "$(window_count "$capture_up" knobas)"
# Two of them, which is the input to the driver's separate exactly-one
# assertion: a count that could only say *some* or *none* would report a
# shortcut handled twice as the same answer as one handled once.
check "two windows of that title are counted as two" 2 \
    "$(window_count "$(printf 'window\tCapture\nwindow\tknobas\nwindow\tCapture\n')" Capture)"
# Whole-field equality on the title. A substring test is how a window that is
# not the capture window answers for the one that is.
check "a window whose title merely contains the word is not a match" 0 \
    "$(window_count "$(printf 'window\tCapture notes\n')" Capture)"
# An untitled window prints its key and an empty field, so a list cannot lose
# its last entry to the trailing-newline strip and *no windows* stays
# distinguishable from *one window with no title*.
check "an untitled window is a window, and not a missing answer" 0 \
    "$(window_count "$(printf 'window\t\n')" Capture)"
# The answer that must never be 0. The helper refuses when it can see no window
# -- a locked screen, a quit app -- and read as 0 that would satisfy this
# driver's closing assertion, which is that the capture window has *gone*.
check "an empty answer is unreadable, not a window that closed" x \
    "$(window_count "" Capture)"
check "an answer in some other shape is unreadable, not zero" x \
    "$(window_count "$(printf 'ax: pid 95163 exposes no window\n')" Capture)"

check "no window of that name reads as absent" no "$(count_verdict 0)"
check "one reads as present" yes "$(count_verdict 1)"
# How many is a separate assertion, made separately by the driver: two capture
# windows and none are two different faults.
check "two read as present too" yes "$(count_verdict 2)"
check "the unreadable count is not an absent window" unreadable "$(count_verdict x)"
check "an empty count is unreadable" unreadable "$(count_verdict "")"

# The two composed, which is how the driver reads them: a count on each side of
# the keystroke, and one word for the pair.
check "no capture window before the shortcut and one after it is a witness" witnessed \
    "$(waypoint_verdict "$(count_verdict 0)" "$(count_verdict 1)")"
# The green that would witness nothing, and the reason the before-count is
# taken at all: a window already open cannot have been opened by the keystroke.
check "a capture window already open before the shortcut witnesses nothing" too-early \
    "$(waypoint_verdict "$(count_verdict 1)" "$(count_verdict 1)")"
check "a shortcut that opened no window is named as that" never-appeared \
    "$(waypoint_verdict "$(count_verdict 0)" "$(count_verdict 0)")"
check "an unreadable before-count is not a witness" unreadable \
    "$(waypoint_verdict "$(count_verdict x)" "$(count_verdict 1)")"
check "an unreadable after-count is not a witness" unreadable \
    "$(waypoint_verdict "$(count_verdict 0)" "$(count_verdict x)")"

check "the frontmost bundle is read out of the helper's own shape" com.apple.finder \
    "$(frontmost_bundle "$(printf 'pid\t431\nbundle\tcom.apple.finder\n')")"
check "a frontmost answer with no bundle reads as nothing" "" \
    "$(frontmost_bundle "$(printf 'pid\t431\nbundle\t\n')")"

# --- the accessible names the capture driver acts on ------------------------

pin_label ../app/src/lib/capture/CaptureWindow.svelte 'aria-label="Capture"' \
    "the capture window's box still has its accessible name"
# The **window's own title**, which is what `ax windows` answers with and what
# the driver's step 4 now waits on. It is a different fact from the box's
# `aria-label` above, from a different file, and the two happen to be the same
# word -- which is exactly how one wait came to be asked of both.
#
# **And this pin is not the witness of it**, said here because #547 was billed
# for the opposite belief. It measures that the driver's constant is not stale
# against the Rust string. Whether the accessibility API answers `AXTitle` with
# that string on that window is a question only a run can put, and this file
# cannot: `just witness-unit` has no window server. The run is `just
# desktop-witness capture`, and the transcript is in testenv/README.md.
pin_label ../crates/knobas-app/src/capture.rs '.title("Capture")' \
    "the capture window is still built under the title the driver waits for"
pin_label ../app/src/lib/capture/CaptureWindow.svelte 'aria-label="Open in knobas"' \
    "the capture window's button still has its accessible name"
# The lines the driver reads back off the screen. Each is a *value* rather
# than a name, which is why `ax values` exists at all, and each has a sibling
# state whose line is different -- *Nothing is kept until you type.* -- so a
# pin on the wrong one would make the run pass on a feature that had failed.
pin_label ../app/src/lib/capture/CaptureWindow.svelte '>Saved as a note.<' \
    "the capture window still says *Saved as a note.* once the row exists"

# Settings › Notes and capture, where the driver sets the shortcut through the
# *Change…* dialog (#1086) and where *Open in knobas* lands while notes hide
# (#1085). Ported by #1084. The three help lines are one row's three states,
# each a different whole line, so the driver tells hidden, registered and
# refused apart.
notes=$tf/settings/TfNotesSection.svelte
pin_label $tf/settings/settings.ts '"Notes and capture"' \
    "Settings still lists the section the capture driver opens"
pin_label $notes 'aria-label="Show notes"' "the notes switch still has its accessible name"
pin_label $notes '"Inactive while notes are hidden."' \
    "the shortcut row still says notes are hidden in the words the driver reads"
pin_label $notes '"Opens a small window over whatever you are doing."' \
    "the shortcut row still says the shortcut holds in the words the driver reads"
pin_label $notes '<span>Not set</span>' "the shortcut row still reads *Not set* with none stored"
pin_label $notes 'aria-label="Change capture shortcut"' "the shortcut's Change… still has its accessible name"
pin_label $notes 'aria-label="Capture shortcut"' "the shortcut field still has its accessible name"
pin_label $notes 'onclick={() => void saveShortcut(null)}>Clear</button>' \
    "the shortcut dialog still clears with a button called Clear"
pin_label $notes 'onclick={() => void saveShortcut(typed)}>Save</button' \
    "the shortcut dialog still stores with a button called Save"
pin_label $notes '>Your notes</div>' "the section still has the row the driver lands on"
pin_label $tf/settings/notes.ts 'return "No notes"' "the notes line still reads *No notes* at zero, which notes_total reads"
# shellcheck disable=SC2016 # the template literal is the text pinned, not an expansion
pin_label $tf/settings/notes.ts '`${total} notes`' "the notes line still leads with the count, which notes_total reads"
pin_label $tf/job/TfCodeSection.svelte 'openLabel="Open Code view"' \
    "the job's Code section still has the link the driver knows a job page by"
# The shortcut field is found by the focus, because the dialog and its heading
# answer to its name too (measured 2026-10-18): `TfDialog` focuses its first field.
pin_label $tf/settings/TfDialog.svelte 'querySelector<HTMLElement>("input:not(\[disabled\]):not(\[type=radio\])")' \
    "a dialog still puts the caret in its first field, where the capture driver types"
# The way to a job the capture driver takes, from Tickets (the demo profile's
# launcher found no ticket on 2026-10-18).
pin_label $tf/route.ts 'tickets: "Tickets"' "the top bar's Tickets link keeps its name"
pin_label $tf/tickets/TicketsPage.svelte 'placeholder: "Filter by key, summary, epic or person"' \
    "the Tickets filter keeps the name the capture driver fills"
for go in 'label: "Start work"' 'label: "Open job"' 'label: "Resume work"'; do
    pin_label $tf/tickets/tickets.ts "$go" "the ticket preview still words its job link $go"
done
pin_label $tf/tickets/TicketPreview.svelte 'href={jobHash(card.entity_id)}>{action.label}</a>' \
    "the ticket preview's button is still a link to the job"
pin_label ../fixtures/tidewater/work.json '"summary": "Retry failed SEPA payouts"' \
    "the demo's PAY-231 still has the summary the capture driver waits for"

# --- where the drivers land (#1968) -----------------------------------------
#
# A pin above measures that a name is still spelled where a driver presses it;
# it cannot say a step still lands where the driver waits. The pin that stood
# here (`TfNote.svelte` still says `#/settings/notes`) stayed green for months
# while *Open in knobas* had moved to the note's own page (#1504), because the
# string survived as the hidden-notes branch. So a driver waits on a landing
# through `lands`/`lands_named`, and `app/src/App.test.svelte.ts` replays each
# in the DOM against the driver's own constants. Checked here: every driver
# declares a landing, and every landing has its replay.

landings_app_test=../app/src/App.test.svelte.ts
for driver_path in desktop-witness/drivers/*.sh; do
    driver_name=$(basename "$driver_path" .sh)
    driver_landings=$(landings_of "$(cat "$driver_path")")
    if [ -n "$driver_landings" ]; then
        check "$driver_name.sh waits on its landings through lands or lands_named" yes yes
    else
        check "$driver_name.sh waits on its landings through lands or lands_named" yes no
        printf '  %s calls neither, so no gate holds its landing pages to the app.\n' "$driver_path" >&2
    fi
    # `lands` runs the driver's own `wait_until`, so a driver that lands defines one.
    if [ -n "$driver_landings" ]; then
        if grep -q '^wait_until() {' "$driver_path"; then
            check "$driver_name.sh defines the wait_until lands calls" yes yes
        else
            check "$driver_name.sh defines the wait_until lands calls" yes no
            printf '  %s calls lands with no wait_until() of its own.\n' "$driver_path" >&2
        fi
    fi
    while IFS= read -r driver_landing; do
        [ -n "$driver_landing" ] || continue
        replay="landing(\"$driver_name\", \"$driver_landing\")"
        if grep -qF "$replay" "$landings_app_test"; then
            check "$driver_name.sh's landing $driver_landing is replayed in the DOM" yes yes
        else
            check "$driver_name.sh's landing $driver_landing is replayed in the DOM" yes no
            printf '  %s has no %s.\n' "$landings_app_test" "$replay" >&2
        fi
    done <<<"$driver_landings"
done

# `landings_of` itself: the first word after the verb, each landing once, and
# neither a comment nor a longer word that starts with the verb.
# shellcheck disable=SC2016 # driver source, quoted as a driver spells it
check "landings_of reads each landing once, in order" "$(printf 'note\njob')" \
    "$(landings_of "$(printf 'lands note "$A"\n    lands_named job "$B"\nlands note "$A"\n# lands gone "$C"\nlandsfoo x\n')")"
# shellcheck disable=SC2016 # driver source, quoted as a driver spells it
check "landings_of reads nothing from a driver without one" "" "$(landings_of 'wait_until "x" reads "$A"')"

# `lands` and `lands_named` against a stub helper: they pass on a screen that
# draws every reading and name, and stop at the first one missing, naming it.
lands_tmp=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-lands.XXXXXX")
cat >"$lands_tmp/ax" <<'STUB'
#!/bin/sh
case $1 in
values) printf '  Retry storm  \nCaptured from here\n' ;;
find) [ "$3" = "Open Code view" ] && echo 1 || echo 0 ;;
esac
STUB
chmod +x "$lands_tmp/ax"
lands_outcome() { # <lands|lands_named> <landing> <value...>: ok, or what wait_until was told
    (
        # shellcheck disable=SC2034 # read by lands_reading and lands_name
        ax=$lands_tmp/ax pid=$$
        # shellcheck disable=SC2329 # called by lands and lands_named
        wait_until() { "${@:2}" || { echo "$1"; exit 0; }; }
        "$@"
        echo ok
    )
}
check "lands passes on a whole line the screen reads" ok "$(lands_outcome lands note 'Retry storm')"
check "lands stops at a line the screen reads only inside a longer one" \
    "the step did not land on note: the screen does not read 'Captured from'" \
    "$(lands_outcome lands note 'Retry storm' 'Captured from')"
check "lands_named passes on a name the screen carries" ok "$(lands_outcome lands_named job 'Open Code view')"
check "lands_named stops at a name nothing carries" \
    "the step did not land on job: nothing on screen is called 'Your notes'" \
    "$(lands_outcome lands_named job 'Your notes')"
rm -rf "$lands_tmp"

# --- the helper's own two answers -------------------------------------------
#
# The registered-path lookup and the permission probe are the two pieces the
# ticket names, and both live in ax.swift rather than in shell: what is above
# tests how their answers are *read*, and this tests that they answer at all.
# Skipped where they cannot run -- this file is otherwise portable, and a gate
# that needs a Mac is a gate that stops being run.

if [ "$(uname -s)" = Darwin ] && command -v swiftc >/dev/null; then
    helper=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-ax.XXXXXX")
    trap 'rm -rf "$helper"' EXIT
    swiftc -O -o "$helper/ax" desktop-witness/ax.swift

    # Not `granted`: this asserts the probe answers in a shape the harness can
    # read, which is true whether or not this particular Mac is drivable. A
    # locked screen must not turn the gate red.
    probe_live=$("$helper/ax" probe)
    classification=$(classify_readiness "$probe_live")
    if [ "$classification" = unreadable ]; then
        check "the live probe answers in a shape classify_readiness understands" yes no
        printf '  ax probe said:\n%s\n' "$probe_live" >&2
    else
        check "the live probe answers in a shape classify_readiness understands" yes yes
    fi

    # Finder, because every Mac has one and nothing about this repo has to be
    # installed for it to answer. The lookup is the same call the harness makes
    # about dev.knobas.desktop.
    finder=$("$helper/ax" registered-path com.apple.finder)
    check_contains "the registered-path lookup finds Finder" ".app" "$finder"
    if [ -d "$finder" ]; then
        check "the path the lookup returns exists" yes yes
    else
        check "the path the lookup returns exists" yes no
    fi

    # The window list, against a pid that is not an application at all -- this
    # script's own shell. **The refusal is the assertion**: `ax windows` prints
    # nothing and exits non-zero when it can see no window, which is what
    # `window_count` reads as `x`, and a version that printed an empty list and
    # exited 0 would make `capture.sh`'s closing *the capture window has gone*
    # pass on an app that had died mid-run.
    if "$helper/ax" windows $$ >/dev/null 2>&1; then
        check "the window list refuses a process with no window" yes no
    else
        check "the window list refuses a process with no window" yes yes
    fi

    # `close-window` closes exactly one window or nothing (#1084): against a
    # process with none it must refuse rather than report a close.
    if "$helper/ax" close-window $$ >/dev/null 2>&1; then
        check "close-window refuses a process with no window" yes no
    else
        check "close-window refuses a process with no window" yes yes
    fi

    # An identifier nothing claims: the lookup must refuse rather than print a
    # path the harness would then try to launch.
    if "$helper/ax" registered-path dev.knobas.nothing-claims-this >/dev/null 2>&1; then
        check "the lookup refuses an unregistered identifier" yes no
    else
        check "the lookup refuses an unregistered identifier" yes yes
    fi

    # live-shot's five (#1483), against the same windowless shell: a window
    # id it cannot find, and a visible match it cannot see, are refusals; an
    # n that is not a number is the usage block, not a guess.
    "$helper/ax" window-id $$ >/dev/null 2>&1 && answer=0 || answer=$?
    check "window-id refuses a process with no window" 1 "$answer"
    "$helper/ax" press-visible $$ Anything >/dev/null 2>&1 && answer=0 || answer=$?
    check "press-visible refuses when nothing is visible" 1 "$answer"
    "$helper/ax" click-at $$ Anything 2 >/dev/null 2>&1 && answer=0 || answer=$?
    check "click-at refuses when the process is not frontmost" 1 "$answer"
    "$helper/ax" wait-content $$ 0.5 >/dev/null 2>&1 && answer=0 || answer=$?
    check "wait-content refuses a process whose window never draws" 1 "$answer"
    "$helper/ax" focus-visible $$ Anything second >/dev/null 2>&1 && answer=0 || answer=$?
    check "an n that is no number is a usage error" 2 "$answer"
    # The window-size step's (#2056): no window to widen is a refusal, and a
    # size that is no number is the usage block.
    "$helper/ax" resize $$ 1241 800 >/dev/null 2>&1 && answer=0 || answer=$?
    check "resize refuses a process with no window" 1 "$answer"
    "$helper/ax" resize $$ wide 800 >/dev/null 2>&1 && answer=0 || answer=$?
    check "a resize width that is no number is a usage error" 2 "$answer"
    "$helper/ax" resize $$ 0 800 >/dev/null 2>&1 && answer=0 || answer=$?
    check "a resize width of zero is a usage error" 2 "$answer"
else
    printf 'desktop-witness-test: skipping the helper checks (needs macOS and swiftc)\n'
fi

# --- live-shot (#1483) --------------------------------------------------------
#
# The key names its `key` command takes, and the refusals it makes before it
# takes anything: the screen run itself is `just live-shot`, never a gate's.

check "key_code: a letter is its ANSI position" 40 "$(key_code k)"
check "key_code: b skips the code ANSI has no key for" 11 "$(key_code b)"
check "key_code: a digit" 29 "$(key_code 0)"
check "key_code: a named key" 36 "$(key_code return)"
check "key_code: an arrow" 125 "$(key_code down)"
check "key_code: a code passes through" 117 "$(key_code 117)"
check "key_code: an unknown word is refused" refused "$(key_code Return || echo refused)"
check "key_code: nothing is refused" refused "$(key_code '' || echo refused)"

if [ "$(uname -s)" = Darwin ]; then
    shot_tmp=$(mktemp -d "${TMPDIR:-/tmp}/knobas-live-shot-test.XXXXXX")
    : >"$shot_tmp/steps.sh"
    refusal() { # <env...> -- <args...>: the status and the message
        local envs=()
        while [ "$1" != -- ]; do envs+=("$1"); shift; done
        shift
        env TMPDIR="$shot_tmp/" LIVE_SHOT_DRY_RUN=1 LIVE_SHOT_PRODUCT= ${envs[@]+"${envs[@]}"} ./live-shot.sh "$@" 2>&1 && echo "status 0" || echo "status $?"
    }
    said=$(refusal -- "$shot_tmp/out")
    check_contains "live-shot: one argument is a usage refusal" "usage: just live-shot" "$said"
    said=$(refusal -- "$shot_tmp/out" "$shot_tmp/none.sh")
    check_contains "live-shot: a missing steps file is refused" "no steps file at" "$said"
    said=$(refusal KNOBAS_WITNESS_WAIT_S=10s -- "$shot_tmp/out" "$shot_tmp/steps.sh")
    check_contains "live-shot: the step guard takes whole seconds only" \
        "KNOBAS_WITNESS_WAIT_S must be a whole number of seconds, got '10s'" "$said"
    check_contains "live-shot: and a bad one exits 2" "status 2" "$said"
    # Another run holds the witness's lock: refused, and that run's lock is
    # still there after (the refusal's exit must not clean up what it never took).
    mkdir "$shot_tmp/knobas-desktop-witness.lock"
    said=$(refusal -- "$shot_tmp/out" "$shot_tmp/steps.sh")
    check_contains "live-shot: a held witness lock is refused" "one at a time" "$said"
    check_contains "live-shot: the refusal exits 1" "status 1" "$said"
    check "live-shot: the holder's lock survives the refusal" yes \
        "$([ -d "$shot_tmp/knobas-desktop-witness.lock" ] && echo yes || echo no)"
    # The lock free: taken, and given back on the way out.
    rmdir "$shot_tmp/knobas-desktop-witness.lock"
    said=$(refusal -- "$shot_tmp/out" "$shot_tmp/steps.sh")
    check_contains "live-shot: a free lock is taken" "dry run: the lock is held" "$said"
    check "live-shot: the lock is given back" no \
        "$([ -d "$shot_tmp/knobas-desktop-witness.lock" ] && echo yes || echo no)"
    rm -rf "$shot_tmp"
fi

# --- the window's size, before any driver or steps file (#2056) --------------
#
# A tiling window manager hands a new window whatever share of the screen is
# left, and below Tickets' 1240 px breakpoint the preview the capture driver
# opens its job from is never drawn. `witness_window_size` is the one step the
# harness and live-shot run before their driver: fullscreen under AeroSpace,
# widen by accessibility elsewhere, then refuse below the minimum, naming both.
# Against a stub `ax` and a stub `aerospace` on a PATH that holds no real one.

# The minimum is the breakpoint's, read from the stylesheet that hides the
# preview, so moving the breakpoint without the minimum turns this red.
breakpoint=$(awk '/@media \(max-width: [0-9]+px\)/ { match($0, /[0-9]+/); w = substr($0, RSTART, RLENGTH) }
    /\.tf-tickets \.tf-preview \{ display: none; \}/ { print w; exit }' ../app/src/lib/tf/css/tickets.css)
check "the minimum width is one past the breakpoint that hides Tickets' preview" \
    "$((breakpoint + 1))" "$WITNESS_MIN_WIDTH"

check "window_width_verdict: a window at the minimum is ok" ok \
    "$(window_width_verdict 1241 820 1241)"
said=$(window_width_verdict 860 1084 1241 && echo "status 0" || echo "status $?")
check_contains "window_width_verdict: a narrow window names its size" "860x1084" "$said"
check_contains "window_width_verdict: and the minimum" "1241" "$said"
check_contains "window_width_verdict: and refuses" "status 1" "$said"
said=$(window_width_verdict '' '' 1241 && echo "status 0" || echo "status $?")
check_contains "window_width_verdict: an unreadable size is refused" "status 1" "$said"

size_tmp=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-size.XXXXXX")
mkdir "$size_tmp/bin" "$size_tmp/state"
cat >"$size_tmp/ax" <<'STUB'
#!/usr/bin/env bash
state=$(dirname "$0")/state
case $1 in
window-id)
    [ -e "$state/width" ] || exit 1
    printf '4242\t%s\t%s\n' "$(cat "$state/width")" "$(cat "$state/height")" ;;
resize)
    echo "resize $*" >>"$state/calls"
    [ -e "$state/resizable" ] || exit 1
    echo "$3" >"$state/width"; echo "$4" >"$state/height" ;;
*) exit 2 ;;
esac
STUB
cat >"$size_tmp/bin/aerospace" <<'STUB'
#!/usr/bin/env bash
state=$(dirname "$0")/../state
case $1 in
list-workspaces) [ -e "$state/aerospace-up" ] ;;
fullscreen)
    echo "aerospace $*" >>"$state/calls"
    [ -e "$state/fullscreens" ] || exit 1
    echo 1728 >"$state/width"; echo 1084 >"$state/height" ;;
*) exit 2 ;;
esac
STUB
chmod +x "$size_tmp/ax" "$size_tmp/bin/aerospace"
# sized <width|none> <height> <flags...>: what the step said, its status, and
# the calls it made, against a window of that size.
sized() {
    rm -f "$size_tmp/state/"*
    [ "$1" = none ] || { echo "$1" >"$size_tmp/state/width"; echo "$2" >"$size_tmp/state/height"; }
    shift 2
    local flag
    for flag in "$@"; do : >"$size_tmp/state/$flag"; done
    PATH="$size_tmp/bin:/usr/bin:/bin" witness_window_size "$size_tmp/ax" $$ 0 2>&1 && echo "status 0" || echo "status $?"
    cat "$size_tmp/state/calls" 2>/dev/null || true
}

said=$(sized 860 1084 aerospace-up fullscreens)
check_contains "AeroSpace: the tiled window is fullscreened by its window id" \
    "aerospace fullscreen on --window-id 4242 --no-outer-gaps" "$said"
check_contains "AeroSpace: the log shows the size the window manager gave" "window 4242: 860x1084 points as the window manager placed it" "$said"
check_contains "AeroSpace: the log shows the size the driver runs at" "window: 1728x1084" "$said"
check_contains "AeroSpace: and the step passes" "status 0" "$said"
said=$(sized 1280 820 aerospace-up fullscreens)
check_contains "AeroSpace: a window wide enough is fullscreened too, so it holds still" \
    "aerospace fullscreen on" "$said"
said=$(sized 860 1084 aerospace-up)
check_contains "AeroSpace: a refused fullscreen is a refusal" "AeroSpace refused to fullscreen window 4242" "$said"
check_contains "AeroSpace: which fails the step" "status 1" "$said"

said=$(sized 1280 820 fullscreens)
check_contains "no AeroSpace running: a wide window runs as it is" "window: 1280x820" "$said"
check_contains "no AeroSpace running: and passes" "status 0" "$said"
check "no AeroSpace running: nothing is fullscreened or resized" no \
    "$(case $said in (*aerospace\ fullscreen* | *resize*) echo yes ;; (*) echo no ;; esac)"
said=$(sized 900 700 resizable)
check_contains "no AeroSpace: a narrow window is widened to the minimum" "resize $$ 1241 700" "$said"
check_contains "no AeroSpace: and runs at that size" "window: 1241x700" "$said"
check_contains "no AeroSpace: and passes" "status 0" "$said"
said=$(sized 900 700)
check_contains "a window that stays narrow is refused, naming its size" "900x700" "$said"
check_contains "and the minimum" "narrower than the 1241" "$said"
check_contains "and fails the step" "status 1" "$said"
said=$(sized none 0)
check_contains "a window the window server cannot find is refused" "no window on screen" "$said"
check_contains "and fails the step" "status 1" "$said"
rm -rf "$size_tmp"

# --- verdict ----------------------------------------------------------------

if [ "$failures" -ne 0 ]; then
    printf 'desktop-witness-test: FAILED (%d of %d checks)\n' "$failures" "$checks" >&2
    exit 1
fi
printf 'desktop-witness-test: ok (%d checks)\n' "$checks"
