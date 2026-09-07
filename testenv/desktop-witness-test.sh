#!/usr/bin/env bash
# The desktop witness's own unit tests (issue #500). No screen, no bundle, no
# signing identity, no macOS: they run wherever bash does, which is why
# `just check` can carry them and `just desktop-witness` cannot.
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

# --- verdict ----------------------------------------------------------------

if [ "$failures" -ne 0 ]; then
    printf 'desktop-witness-test: FAILED (%d of %d checks)\n' "$failures" "$checks" >&2
    exit 1
fi
printf 'desktop-witness-test: ok (%d checks)\n' "$checks"
