#!/usr/bin/env bash
# The desktop witness's own unit tests (issue #500). No screen, no bundle and
# no signing identity, which is why `just check` can carry them and `just
# desktop-witness` cannot. Most of them need nothing but bash; the four at the
# foot compile `ax.swift` and ask the helper its two questions, and skip
# themselves where macOS and `swiftc` are not both present.
#
# What is under test is `desktop-witness-lib.sh`: the path comparison the
# harness makes against Launch Services' answer, and the reading of the
# accessibility probe. Both are small enough to look correct and have been
# wrong in this exact shape before -- the notification prototype lost a run to
# a bundle path that differed by where it was copied from, and an unanswered
# TCC prompt is recorded as a *denial*, so "no news" from a probe is the one
# reading that must never come back as "granted".
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

built=/Users/dev/Projects/knobas/target/debug/bundle/macos/knobas.app

check "a plain bundle path is left alone" \
    "$built" "$(normalise_app_path "$built")"
check "Launch Services' trailing slash is dropped" \
    "$built" "$(normalise_app_path "$built/")"
check "several trailing slashes are dropped" \
    "$built" "$(normalise_app_path "$built///")"
check "doubled separators collapse" \
    "$built" "$(normalise_app_path "/Users/dev//Projects/knobas/target/debug//bundle/macos/knobas.app")"
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
    "$built" "/Users/dev/Projects/knobas/.worktrees/issue-500/target/debug/bundle/macos/knobas.app"

# A prefix is not a match: `.../knobas.app` and `.../knobas.app.old` differ by
# a suffix that a `case`-style prefix test would swallow.
check_matches "a longer path with the same prefix does not match" no \
    "$built" "$built.old"

# `*` is legal in a filename, and an unquoted `[[ == ]]` right-hand side would
# read it as a glob that matches the built path.
check_matches "a wildcard in a path is compared literally, not matched" no \
    "$built" "/Users/dev/Projects/knobas/target/debug/bundle/macos/*"

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
    "$(printf '#!/bin/sh\nprintf %s "$@" > %s\n' "'%s\\n'" "'$record'")" \
    "$(stub_script "$record")"
# The record path is single-quoted in the script, so a directory with a space
# in it -- which `mktemp -d` under a Mac's TMPDIR routinely is not, and a
# worktree under "My Code" routinely is -- stays one word.
check_contains "the recorded path is quoted in the script" \
    "> '$record'" "$(stub_script "$record")"

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
# `crates/knobas-app/tests/checkout_ipc.rs`, waits for the trailing newline,
# which is the write having *finished*. A wait on non-emptiness can hand the
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

# No trap: `check` never exits, so this directory is removed at the foot of the
# section, and the one way past that line is an error that ends the run -- when
# a directory under TMPDIR is the smallest of the problems.
records=$(mktemp -d "${TMPDIR:-/tmp}/knobas-witness-records.XXXXXX")

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
# Two arguments, and a path with a space in it: what makes a record finished is
# where it ends, not what it says.
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

# The twin, pinned in the direction this ticket's divergence ran: two files
# waiting on one stub, and only a reader of both can see them disagree. This
# pins the spelling of the Rust condition, which is all a shell test can see of
# it -- the same cheap cross-file pin as the labels below, and for the same
# reason.
twin=../crates/knobas-app/tests/checkout_ipc.rs
if grep -qF "text.ends_with('\\n')" "$twin"; then
    check "the Rust twin still waits for the trailing newline" yes yes
else
    check "the Rust twin still waits for the trailing newline" yes no
    printf '  %s no longer waits on text.ends_with in recorded();\n' "$twin" >&2
    printf '  record_is_complete is the shell copy of that condition (#531).\n' >&2
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

pin_label ../app/src/lib/shell/TopStrip.svelte 'aria-label="Settings"' \
    "the top strip's Settings button still has its accessible name"
pin_label ../app/src/lib/settings/CheckoutsSection.svelte 'aria-label="Save clones root"' \
    "the clones-root Save still has an accessible name of its own"
pin_label ../app/src/lib/settings/CheckoutsSection.svelte 'aria-label="Save {command.label}"' \
    "each open-command Save is named after its own command"
# The **visible label text**, not the `for=`/`id=` pair: the driver focuses
# this field by its accessible name, which the label element supplies, so a
# pin on the id would stay green through a rename and let the run fail minutes
# in. Same for the three command fields, whose names come from
# `knobas_core::checkout`'s `label()` and are pinned there.
pin_label ../app/src/lib/settings/CheckoutsSection.svelte 'for="clones-root">Directory<' \
    "the clones-root field is still labelled Directory"
# The button the driver presses and the settings field it fills are both named
# from one Rust string, so this is where the driver's `Open in VS Code` comes
# from -- not from either component.
pin_label ../crates/knobas-core/src/checkout.rs '"Open in VS Code"' \
    "the VS Code action is still labelled the way the driver asks for it"
# The Save that stored a template puts a *Reset* beside it, and the driver
# waits for that appearing as its proof the write landed -- so the reset
# button's accessible name is a name it acts on too.
pin_label ../app/src/lib/settings/CheckoutsSection.svelte 'aria-label="Reset {command.label}"' \
    "each open-command Reset is named after its own command"
# The panel's own heading, which is how the driver tells a repo detail from
# every other kind: a checkout panel is what a repo and a branch have and
# nothing else does.
pin_label ../app/src/lib/detail/CheckoutPanel.svelte '>Checkout<' \
    "the checkout panel still carries the heading the driver looks for"
# And the remote the driver writes into its fake clone: the scan matches it
# against the demo repo's own URL, which this constant is the stem of.
pin_label ../crates/knobas-source-mock/src/lib.rs 'https://tidewater.example' \
    "the demo corpus still lives under the host the driver's clone points at"

# --- rendered_label, and the two stylesheet rules behind it -----------------
#
# Measured on the dev Mac on 2026-09-08, in the first harness run that got past
# its probe: WebKit names an element by its **rendered** text, so a label the
# stylesheet uppercases is named in upper case and a driver looking for the
# markup's spelling finds nothing. Two drivers depend on that, so the rule is
# one function and the stylesheet rules behind it are pinned: if either
# `text-transform` goes, the constants in `capture.sh` and `open-in-editor.sh`
# have to change, and this is what says so.

check "a label's rendered name is its upper case" "SHORTCUT" "$(rendered_label Shortcut)"
check "a rendered name keeps its spaces" "OPEN IN VS CODE" "$(rendered_label 'Open in VS Code')"
check "a lower-case reading renders in upper case" "CAPTURED FROM" \
    "$(rendered_label 'captured from')"
check "an already-upper name is unchanged" "CHECKOUT" "$(rendered_label CHECKOUT)"
check "nothing renders as nothing" "" "$(rendered_label '')"

pin_label ../app/src/app.css '.lab{font:600 11px/1 var(--disp);text-transform:uppercase' \
    "the .lab class still uppercases, which is why the field names are upper case"
pin_label ../app/src/app.css '.row.hd{height:22px;cursor:default;color:var(--faint);font:500 10px var(--mono);text-transform:uppercase' \
    "the .row.hd class still uppercases, which is why the panel readings are upper case"

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

check "the frontmost bundle is read out of the helper's own shape" com.apple.finder \
    "$(frontmost_bundle "$(printf 'pid\t431\nbundle\tcom.apple.finder\n')")"
check "a frontmost answer with no bundle reads as nothing" "" \
    "$(frontmost_bundle "$(printf 'pid\t431\nbundle\t\n')")"

# --- the accessible names the capture driver acts on ------------------------

pin_label ../app/src/lib/capture/CaptureWindow.svelte 'aria-label="Capture"' \
    "the capture window's box still has its accessible name"
pin_label ../app/src/lib/capture/CaptureWindow.svelte 'aria-label="Open in knobas"' \
    "the capture window's button still has its accessible name"
pin_label ../app/src/lib/settings/CaptureSection.svelte 'aria-label="Save capture shortcut"' \
    "the capture shortcut's Save still has an accessible name of its own"
# The **visible label text**, not the `for=`/`id=` pair: the driver focuses this
# field by its accessible name, which the label element supplies, so a pin on
# the id would stay green through a rename and let the run fail minutes in.
pin_label ../app/src/lib/settings/CaptureSection.svelte 'for="capture-shortcut">Shortcut<' \
    "the capture shortcut field is still labelled Shortcut"
# The three lines the driver reads back off the screen. Each is a *value* rather
# than a name, which is why `ax values` exists at all, and each has a sibling
# state whose line is different -- *Not registered: …*, *Nothing is kept until
# you type.* -- so a pin on the wrong one would make the run pass on a feature
# that had failed.
pin_label ../app/src/lib/settings/CaptureSection.svelte '>Registered.<' \
    "the settings section still says *Registered.* when the shortcut holds"
pin_label ../app/src/lib/capture/CaptureWindow.svelte '>Saved as a note.<' \
    "the capture window still says *Saved as a note.* once the row exists"
pin_label ../app/src/lib/detail/LinksPanel.svelte '>Linked items<' \
    "the links panel still carries the heading the driver reads a detail by"
# The two readings the panel groups a capture's links under, from the note's
# own side (`readingOf`'s forward). The inverse readings -- *captured here*,
# *captured from here* -- are what the other end shows, and `has_reading`'s
# whole-line rule is what keeps them apart.
pin_label ../app/src/lib/detail/relations.ts 'forward: "captured from"' \
    "a captured-from link still reads *captured from* on the note"

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

    # An identifier nothing claims: the lookup must refuse rather than print a
    # path the harness would then try to launch.
    if "$helper/ax" registered-path dev.knobas.nothing-claims-this >/dev/null 2>&1; then
        check "the lookup refuses an unregistered identifier" yes no
    else
        check "the lookup refuses an unregistered identifier" yes yes
    fi
else
    printf 'desktop-witness-test: skipping the helper checks (needs macOS and swiftc)\n'
fi

# --- verdict ----------------------------------------------------------------

if [ "$failures" -ne 0 ]; then
    printf 'desktop-witness-test: FAILED (%d of %d checks)\n' "$failures" "$checks" >&2
    exit 1
fi
printf 'desktop-witness-test: ok (%d checks)\n' "$checks"
