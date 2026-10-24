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
# The name the app's process has, which is the cargo binary name and not the
# bundle's `productName` ("knobas"): tauri leaves `Contents/MacOS/knobas-app`.
# `just witness-unit` pins it against `crates/knobas-app/Cargo.toml`, because
# every use of it below is a check that would go quietly vacuous if it drifted
# -- "no knobas is already running" would pass on a machine with two.
readonly APP_EXECUTABLE=knobas-app
readonly LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# The demo profile, not the default one: its own data directory, its own
# embedded PostgreSQL on its own port and its own keychain service (P13), so a
# witness run cannot mix fixture data into whatever corpus the person whose Mac
# this is has been working in.
readonly PROFILE_FLAG=--demo
# Generous, because it covers a first-run PostgreSQL provision, not a window
# being drawn.
readonly WINDOW_TIMEOUT=180
# The guard on the launch and on every wait the driver makes on the app
# (KNOBAS_WITNESS_WAIT_S, `witness_guard`, #1961), read here so a bad value is
# refused before the build rather than after it.
WAIT_S=$(witness_guard) || exit 2
readonly WAIT_S

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
# Newline-separated copies this run unregistered (opt-in, #1176) and puts back
# on the way out. A string, not an array: bash 3.2 and `set -u`.
unregistered=
cleanup() {
    local status=$?
    # Before anything else that can fail: the copies this run took out of
    # Launch Services go back, on success, failure and Ctrl-C alike, if the
    # bundle is still on disk (one that is gone needs no putting back).
    if [ -n "$unregistered" ]; then
        local path
        while IFS= read -r path; do
            [ -n "$path" ] || continue
            if [ ! -d "$path" ]; then
                note "not re-registering $path: it is gone from disk"
            elif "$LSREGISTER" -f "$path" 2>/dev/null; then
                note "re-registered $path"
            else
                note "could not re-register $path"
            fi
        done <<<"$unregistered"
        unregistered=
    fi
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
# `pgrep -x`, on the process name, rather than `pgrep -f` on the bundle path:
# `-f` matches the whole command line, so any process that merely *mentions*
# the path is counted, and one agent running `ps aux | grep
# knobas.app/Contents/MacOS` while this runs is enough. Measured 2026-09-08:
# an unrelated `perl` with that path in its arguments made `pgrep -f` answer 2
# where `pgrep -x knobas-app` answered 1 -- which here is a clean machine
# refused as "already running", and below is a correct run failing its
# single-instance assertion.
running=$(pgrep -x "$APP_EXECUTABLE" | tr '\n' ' ' || true)
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
if [ "${KNOBAS_WITNESS_UNREGISTER_OTHERS:-}" = 1 ]; then
    # Opt-in (#1176). Default stays the refusal below. Only copies inside this
    # repo's checkouts are touched; anything else (an installed knobas in
    # /Applications, a DMG mount) is reported and left registered, so the
    # refusal below still names it -- ADR-0016: the harness never moves an
    # installed knobas aside.
    step "unregistering the other repo copies of $BUNDLE_ID (KNOBAS_WITNESS_UNREGISTER_OTHERS=1)"
    repo_root=$(dirname "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)")
    dump=$("$LSREGISTER" -dump)
    left=$(copies_to_leave "$dump" "$BUNDLE_ID" "$app" "$repo_root")
    to_unregister=$(copies_to_unregister "$dump" "$BUNDLE_ID" "$app" "$repo_root")
    # Record before unregistering, so a Ctrl-C half way still restores them.
    unregistered=$to_unregister
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        if "$LSREGISTER" -u "$path" 2>/dev/null; then
            note "unregistered $path"
        else
            note "could not unregister $path"
        fi
    done <<<"$to_unregister"
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        note "left registered (outside $repo_root): $path"
    done <<<"$left"
fi

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
# Polled until the process is there, under the guard (#1961): `open` returns
# once Launch Services has the request, and a loaded machine can take far
# longer than the old 10 s to exec the binary.
app_pid=
launched_at=$SECONDS
until app_pid=$(pgrep -x "$APP_EXECUTABLE" | head -1) && [ -n "$app_pid" ]; do
    [ $((SECONDS - launched_at)) -lt "$WAIT_S" ] ||
        fail "nothing started: no $APP_EXECUTABLE process appeared within $WAIT_S s of 'open' (KNOBAS_WITNESS_WAIT_S)." \
            "last seen: pgrep -x $APP_EXECUTABLE found none."
    sleep 0.25
done
note "pid $app_pid"

step "waiting for a window (up to ${WINDOW_TIMEOUT} s)"
"$ax" wait-window "$app_pid" "$WINDOW_TIMEOUT" | sed 's/^/desktop-witness:   window titled /'

# And for its web view to draw (#1961): the window is up before the page in it,
# and a keystroke a driver sends before the page has mounted goes nowhere --
# no wait on the keystroke's effect brings it back. Seen on a loaded machine:
# ⌘K pressed the moment the window appeared, and the launcher never opened.
step "waiting for the web view to draw (up to ${WINDOW_TIMEOUT} s)"
"$ax" wait-content "$app_pid" "$WINDOW_TIMEOUT" ||
    fail "knobas' window stayed blank for ${WINDOW_TIMEOUT} s."

# `|| true` because `set -o pipefail` turns pgrep's "found nothing" exit 1
# into an aborted run, and "no instances" is a verdict this line has to be
# allowed to reach rather than a reason to die without saying so.
instances=$(pgrep -x "$APP_EXECUTABLE" | wc -l | tr -d ' ' || true)
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
# ⌘Q only once knobas is in front: a keystroke goes to whatever is, and on a
# Mac somebody is using that is their terminal or browser (seen on #1961's
# runs: `activate` refused while the person typed elsewhere).
if "$ax" activate "$app_pid"; then
    "$ax" key 12 command || true
else
    note "knobas would not come forward, so no ⌘Q: it would quit whatever is in front"
fi
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
