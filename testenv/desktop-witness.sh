#!/usr/bin/env bash
# The desktop witness (issue #500, ADR-0016): the signed bundle, launched from
# the path Launch Services has registered, driven by desktop automation.
#
# Three of knobas' features are the operating system's rather than the app's --
# open-in-editor, open-in-terminal and the ⌘K capture shortcut -- and there is
# no instance to run a suite against, so the witness for them is a scripted run
# against a real bundle on a real Mac. That is the v1.5 grilling's ruling, and
# its scope: OS-level features only. A rendered panel is witnessed by headless
# Chrome against the `?fake-ipc` dev server (the deputy's ruling of 2026-09-08
# on #496), not here, and a driver that reaches for one is in the wrong file.
#
# Usage: just desktop-witness <driver>
#        testenv/desktop-witness.sh <driver>
#
# where <driver> names a script in testenv/desktop-witness/drivers/. The
# prerequisites, and what to do about each refusal, are in testenv/README.md,
# "The desktop witness (macOS)".
#
# This recipe is NOT part of `just check`. It takes over the screen, it takes
# minutes, and it needs a signing identity and a TCC grant that a checkout does
# not have. What the gate carries is `desktop-witness-test.sh`, over the two
# pieces of this harness that are decisions rather than side effects.

set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD

# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. testenv/desktop-witness-lib.sh

readonly BUNDLE_ID=dev.knobas.desktop
readonly SIGNING_IDENTITY=knobas-dev
readonly LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# The demo profile, not the default one: its own data directory, its own
# embedded PostgreSQL on its own port and its own keychain service (P13), so a
# witness run cannot mix fixture data into whatever corpus the person whose Mac
# this is has been working in.
readonly PROFILE_FLAG=--demo
# Generous, because it covers a first-run PostgreSQL provision, not a window
# being drawn.
readonly WINDOW_TIMEOUT=180

step() { printf '\ndesktop-witness: %s\n' "$*"; }
note() { printf 'desktop-witness:   %s\n' "$*"; }
fail() {
    printf 'desktop-witness: FAILED -- %s\n' "$1" >&2
    shift
    for line in "$@"; do printf 'desktop-witness:   %s\n' "$line" >&2; done
    exit 1
}

[ "$(uname -s)" = Darwin ] || fail "this witness is macOS-only (uname says $(uname -s))." \
    "Launch Services, codesign and the accessibility API are all macOS." \
    "See ${WITNESS_README_SECTION}."

# The drivers this harness knows about, for the two messages that list them.
drivers() {
    local path name=()
    for path in testenv/desktop-witness/drivers/*.sh; do
        [ -e "$path" ] || continue
        name+=("$(basename "$path" .sh)")
    done
    # `${name[*]-}`, not `${name[*]}`: /usr/bin/env bash on this Mac is 3.2.57,
    # where an empty array expansion is an unbound variable under `set -u` --
    # so an empty drivers directory would kill the usage message instead of
    # printing an empty list.
    printf '%s\n' "${name[*]-}"
}

driver_name=${1-}
[ -n "$driver_name" ] || fail "no driver named." \
    "usage: just desktop-witness <driver>" \
    "drivers: $(drivers)"

driver=testenv/desktop-witness/drivers/${driver_name}.sh
[ -x "$driver" ] || fail "no driver called '${driver_name}' (looked for ${driver})." \
    "drivers: $(drivers)"

for tool in swiftc codesign security open pgrep; do
    command -v "$tool" >/dev/null \
        || fail "$tool is required and is not on PATH." "See ${WITNESS_README_SECTION}."
done
[ -x "$LSREGISTER" ] || fail "lsregister is not where it is expected ($LSREGISTER)."

# One witness at a time, enforced rather than asked for. Two runs on this
# machine would fight over one screen, one bundle identifier and one Launch
# Services registration, and the loser would report the winner's app as its
# own. `mkdir` because it is the atomic test-and-set that every shell has;
# `just test`'s slot guard is the same idea with more slots.
readonly LOCK="${TMPDIR:-/tmp}/knobas-desktop-witness.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
    printf 'desktop-witness: FAILED -- another desktop witness holds %s.\n' "$LOCK" >&2
    printf 'desktop-witness:   One runs at a time on this machine. If nothing is\n' >&2
    printf 'desktop-witness:   running, a killed run left it behind: rmdir it.\n' >&2
    exit 1
fi

# Per-invocation scratch, like every other recipe here: several worktrees and
# several agents share one /tmp on this machine.
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-desktop-witness.XXXXXX")
app_pid=
previous_registration=
cleanup() {
    local status=$?
    if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
        note "quitting the app that is still running (pid $app_pid)"
        kill -TERM "$app_pid" 2>/dev/null || true
    fi
    # Leave Launch Services as it was found. Which of several copies of one
    # identifier LS prefers is not a documented API, so this is best-effort and
    # says so: re-registering the copy that answered before this run is the
    # most a script can do about it.
    if [ -n "$previous_registration" ] && [ -d "$previous_registration" ]; then
        # The message follows the outcome rather than the attempt: a `note`
        # printed unconditionally after a command that may have failed is a
        # check that measures a representation of the thing.
        if "$LSREGISTER" -f "$previous_registration" 2>/dev/null; then
            note "re-registered $previous_registration with Launch Services"
        else
            note "could not re-register $previous_registration with Launch Services"
        fi
    fi
    rm -rf "$scratch"
    rmdir "$LOCK" 2>/dev/null || true
    exit "$status"
}
trap cleanup EXIT
trap 'trap - INT; kill -INT $$' INT
trap 'trap - TERM; kill -TERM $$' TERM

# ---------------------------------------------------------------------------
# 1. The accessibility helper, and whether this session can drive a desktop.
#
# Before the build, because a build takes minutes and mutates Launch Services
# on its way to a run that a locked screen was always going to refuse -- and
# again after it, because a screen can lock during those minutes and the only
# moment worth probing is the one just before the keystroke.
# ---------------------------------------------------------------------------
step "compiling the accessibility helper"
ax=$scratch/ax
swiftc -O -o "$ax" testenv/desktop-witness/ax.swift
note "$ax"

check_readiness() {
    local output classification
    output=$("$ax" probe 2>&1) || true
    printf '%s\n' "$output" | sed 's/^/desktop-witness:   /'
    classification=$(classify_readiness "$output")
    [ "$classification" = granted ] || {
        local message
        message=$(readiness_message "$classification")
        printf 'desktop-witness: FAILED -- %s\n' "$message" >&2
        exit 1
    }
}

step "checking that this session can drive the desktop"
check_readiness

# ---------------------------------------------------------------------------
# 2. The signing identity, and a machine with no knobas already on it.
# ---------------------------------------------------------------------------
step "checking the signing identity"
security find-identity -v -p codesigning | grep -q "\"$SIGNING_IDENTITY\"" \
    || fail "no code-signing identity called '$SIGNING_IDENTITY' in the keychain." \
        "The README's *Signed dev build (macOS)* section has the four openssl" \
        "commands that create it."
note "$SIGNING_IDENTITY"

# One instance, from one path, is the whole point of launching from the
# registered bundle; a copy already running would make every assertion below
# ambiguous about which process answered it.
running=$(pgrep -f 'knobas\.app/Contents/MacOS/' | tr '\n' ' ' || true)
if [ -n "${running// /}" ]; then
    fail "knobas is already running (pid ${running% })." \
        "Quit it first: this witness asserts that exactly one instance is up," \
        "and it is about to take the screen away from whatever is on it."
fi

# ---------------------------------------------------------------------------
# 3. Build and sign the debug bundle.
# ---------------------------------------------------------------------------
step "building and signing the debug bundle"
(
    cd crates/knobas-app
    APPLE_SIGNING_IDENTITY=$SIGNING_IDENTITY \
        env -u RUSTUP_TOOLCHAIN PATH="$repo/app/node_modules/.bin:$PATH" \
        tauri build --debug --bundles app
)

app=
for candidate in \
    "$repo/target/debug/bundle/macos/knobas.app" \
    "$repo/crates/knobas-app/target/debug/bundle/macos/knobas.app"; do
    [ -d "$candidate" ] && app=$candidate && break
done
[ -n "$app" ] || fail "the build produced no bundle at either of the two places" \
    "cargo puts one (workspace target/, crate target/)."
note "$app"

step "verifying the signature"
codesign --verify --deep --strict "$app"
codesign -dv --verbose=2 "$app" 2>&1 | tee "$scratch/codesign" | sed 's/^/desktop-witness:   /'
grep -q "^Authority=$SIGNING_IDENTITY$" "$scratch/codesign" \
    || fail "the bundle is not signed by '$SIGNING_IDENTITY'." \
        "codesign reports: $(grep '^Authority=' "$scratch/codesign" | head -1)"

# ---------------------------------------------------------------------------
# 4. Launch Services: launch the copy it has registered, not the copy we built.
#
# knobas' desktop notifications activate *the bundle Launch Services has
# registered for the identifier*, whichever copy that is. The notification
# prototype lost a run to exactly this: a click started a second instance from
# the registered path while the one it was launched from waited on. So the
# harness asks LS, and refuses rather than launching a copy LS does not name.
# ---------------------------------------------------------------------------
step "registering the bundle with Launch Services"
previous_registration=$("$ax" registered-path "$BUNDLE_ID" 2>/dev/null || true)
[ -n "$previous_registration" ] && note "was: $previous_registration"
"$LSREGISTER" -f "$app"
registered=$("$ax" registered-path "$BUNDLE_ID") \
    || fail "Launch Services has no application for $BUNDLE_ID even after registering one."
note "now: $registered"
paths_match "$app" "$registered" \
    || fail "Launch Services still resolves $BUNDLE_ID to another copy." \
        "registered: $registered" \
        "built:      $app" \
        "Which copy LS prefers among several with one identifier is not a" \
        "documented API. Move or delete the other copy (an installed knobas in" \
        "/Applications is the usual one) and run this again -- launching the" \
        "build anyway is how a notification click starts a second instance." \
        "See ${WITNESS_README_SECTION}."

step "re-checking that the desktop is still drivable after the build"
check_readiness

# ---------------------------------------------------------------------------
# 5. Launch it, and wait for a window.
# ---------------------------------------------------------------------------
step "launching $registered $PROFILE_FLAG"
open -a "$registered" --args "$PROFILE_FLAG"
app_pid=
for _ in $(seq 1 40); do
    app_pid=$(pgrep -f 'knobas\.app/Contents/MacOS/' | head -1 || true)
    [ -n "$app_pid" ] && break
    sleep 0.25
done
[ -n "$app_pid" ] || fail "nothing started: no knobas process appeared within 10 s of 'open'."
note "pid $app_pid"

step "waiting for a window (up to ${WINDOW_TIMEOUT} s)"
"$ax" wait-window "$app_pid" "$WINDOW_TIMEOUT" | sed 's/^/desktop-witness:   window titled /'

# `|| true` because `set -o pipefail` turns pgrep's "found nothing" exit 1
# into an aborted run, and "no instances" is a verdict this line has to be
# allowed to reach rather than a reason to die without saying so.
instances=$(pgrep -f 'knobas\.app/Contents/MacOS/' | wc -l | tr -d ' ' || true)
[ "$instances" = 1 ] || fail "$instances knobas processes are running, not one." \
    "That is the Launch Services caveat in the README: a second copy was" \
    "started from a path this run did not launch."
note "one instance"

# ---------------------------------------------------------------------------
# 6. The driver.
# ---------------------------------------------------------------------------
step "running driver '$driver_name'"
status=0
KNOBAS_WITNESS_AX=$ax \
    KNOBAS_WITNESS_PID=$app_pid \
    KNOBAS_WITNESS_APP=$registered \
    "$driver" || status=$?

# ---------------------------------------------------------------------------
# 7. Quit. ⌘Q rather than a signal: the app stops its embedded PostgreSQL on
#    the way out, and a SIGTERM'd Rust process runs no exit handler at all.
# ---------------------------------------------------------------------------
step "quitting"
"$ax" activate "$app_pid" || true
"$ax" key 12 command || true
for _ in $(seq 1 40); do
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.5
done
if kill -0 "$app_pid" 2>/dev/null; then
    note "⌘Q left it running; sending SIGTERM"
    kill -TERM "$app_pid" 2>/dev/null || true
    sleep 2
fi
kill -0 "$app_pid" 2>/dev/null || app_pid=

if [ "$status" -ne 0 ]; then
    fail "driver '$driver_name' exited $status."
fi
step "ok: driver '$driver_name' passed against $registered"
