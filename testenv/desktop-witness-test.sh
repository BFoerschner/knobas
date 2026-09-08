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
