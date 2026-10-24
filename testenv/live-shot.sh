#!/usr/bin/env bash
# Screenshots of the real app against live products (#1483): `just demo` with
# KNOBAS_SECRET_STORE=memory, driven by a steps file through the desktop
# witness's accessibility helper, photographing the knobas window only.
#
# Usage: just live-shot <out dir> <steps file>
#        testenv/live-shot.sh <out dir> <steps file>
#
# The steps file is bash, sourced once the window is up, with these commands
# (and, under LIVE_SHOT_PRODUCT, `./seed --env`'s variables exported):
#
#   shot <name>            <out dir>/<name>.png: the knobas window only, scaled
#                          from Retina to the window's size in points
#   press <label> [n]      press the one visible element whose accessible
#                          description, title or placeholder is <label>, or the
#                          n-th in reading order when several are visible (with
#                          none in the window, one below the fold), scrolled
#                          into view first
#   focus <label> [n]      focus it
#   click-at <label> [n]   a pointer click at its centre: a control only a hover
#                          reveals, which accessibility cannot press, or words
#                          on screen (static text whose value is <label>: a
#                          radio's label, a list row's name)
#   type <text>            type into the focused element
#   key <key> [modifier..] one key: a letter, a digit, return, escape, tab,
#                          space, delete, up, down, left, right, or a key code;
#                          modifiers command, shift, option, control
#   settle [seconds]       wait for the app (default 1)
#
# `press`, `focus` and `click-at` retry until they land, under the guard
# KNOBAS_WITNESS_WAIT_S (60 s by default; at once when knobas has exited), as
# `just shot`'s `click:` does; the steps start once the web view has drawn
# something.
# Every command first checks that the screen is not locked, and `type`, `key`
# and `click-at` that knobas is frontmost (activating it once): keystrokes sent
# to another app are a person's work typed over. A failing command writes
# <out dir>/failed.png and failed-ax.txt (the window's accessibility tree) and
# quits the app.
#
# Env:  LIVE_SHOT_PRODUCT=gitea|forgejo|teamcity|jira|confluence  bring that
#         product up first through testenv/live's session: its lock (the Gitea
#         lock for Gitea, the JVM lock for a JVM product), its seed, and put
#         back after. Unset starts no product and takes no product lock.
#       LIVE_SHOT_DRY_RUN=1  stop once the lock is held (its unit test reads it).
#       LIVE_SHOT_AEROSPACE=1  fullscreen the window in AeroSpace, so a tiling
#         window manager does not resize it between shots. Off by default;
#         refused when `aerospace` is not on PATH.
#       KNOBAS_WITNESS_WAIT_S=60  the guard on `press`, `focus` and
#         `click-at`, whole seconds (testenv/desktop-witness-lib.sh).
#
# It takes the screen, as the desktop witness does, and holds the witness's
# lock: one of the two runs on this machine at a time. NOT part of any gate.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. "$here/desktop-witness-lib.sh"

say() { printf 'live-shot: %s\n' "$*"; }
app_pid=
# Once the app is up, a failure leaves what the window showed and its
# accessibility tree beside the shots: the next step to write reads them.
die() {
    printf 'live-shot: FAILED -- %s\n' "$*" >&2
    if [ -n "$app_pid" ] && [ -z "${dying:-}" ]; then
        dying=1
        local id
        local width
        if read -r id width _ < <("$ax" window-id "$app_pid" 2>/dev/null) \
            && screencapture -x -o -l "$id" "$out/failed.png" \
            && sips --resampleWidth "$width" "$out/failed.png" >/dev/null; then
            printf 'live-shot:   the window: %s\n' "$out/failed.png" >&2
        fi
        "$ax" dump "$app_pid" 40 >"$out/failed-ax.txt" 2>&1 || true
        printf 'live-shot:   its accessibility tree: %s\n' "$out/failed-ax.txt" >&2
    fi
    exit 1
}

[ $# -eq 2 ] || die "usage: just live-shot <out dir> <steps file>"
out=$1 steps=$2
[ -f "$steps" ] || die "no steps file at $steps"
steps=$(cd "$(dirname "$steps")" && pwd)/$(basename "$steps")
[ "$(uname -s)" = Darwin ] || die "macOS only: the accessibility API and screencapture are"
for tool in swiftc screencapture sips pgrep; do
    command -v "$tool" >/dev/null || die "$tool is required and is not on PATH"
done
product=${LIVE_SHOT_PRODUCT:-}
aerospace=${LIVE_SHOT_AEROSPACE:-}
case $aerospace in '' | 1) ;; *) die "LIVE_SHOT_AEROSPACE must be 1 or unset, got '$aerospace'" ;; esac
[ -z "$aerospace" ] || command -v aerospace >/dev/null \
    || die "LIVE_SHOT_AEROSPACE=1 but aerospace is not on PATH"
# The guard on `press`, `focus` and `click-at` (#1961).
wait_s=$(witness_guard) || exit 2
mkdir -p "$out"
out=$(cd "$out" && pwd)

# The product first: a JVM product can queue for its lock for 45 minutes, and
# that wait should not hold the screen. live_session arms its own traps, which
# `cleanup` below takes over and calls.
if [ -n "$product" ]; then
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    live_session "$product" live-shot
fi

# The desktop witness's lock: both take the screen.
readonly LOCK="${TMPDIR:-/tmp}/knobas-desktop-witness.lock"
mkdir "$LOCK" 2>/dev/null \
    || die "a desktop witness or another live-shot holds $LOCK; one at a time (a killed run left it behind if nothing runs: rmdir it)"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-live-shot.XXXXXX")
demo_pid=
# The demo profile's data directory, and the postmaster serving it before
# `just demo` (#1967): set at the start, read by `cleanup`.
pg_data=
pg_before=
cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
        say "quitting knobas (pid $app_pid)"
        # ⌘Q, as the witness does: the app stops its embedded PostgreSQL on
        # the way out, and a SIGTERM'd Rust process runs no exit handler.
        if "$ax" activate "$app_pid" 2>/dev/null; then "$ax" key 12 command 2>/dev/null || true; fi
        for _ in $(seq 1 40); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.5; done
        kill -TERM "$app_pid" 2>/dev/null || true
        # A SIGTERM ends it at once; one still up after the guard is hung, and
        # is killed outright so its server can be stopped below (#1967).
        local polled=0
        witness_poll "$app_pid" "$wait_s" false || polled=$?
        if [ "$polled" = 1 ]; then
            say "knobas outlived SIGTERM by $wait_s s (KNOBAS_WITNESS_WAIT_S); sending SIGKILL"
            kill -KILL "$app_pid" 2>/dev/null || true
            witness_poll "$app_pid" "$wait_s" false || true
        fi
    fi
    if [ -n "$demo_pid" ]; then
        for _ in $(seq 1 20); do kill -0 "$demo_pid" 2>/dev/null || break; sleep 0.5; done
        # Started under `set -m`, so its group is its pid; the group outlives a
        # leader that has exited (the dev server on :1420 is in it). The stop
        # below runs once the group is gone, SIGKILLed past the guard.
        pkill -TERM -g "$demo_pid" 2>/dev/null || true
        local deadline=$((SECONDS + wait_s))
        while pgrep -g "$demo_pid" >/dev/null 2>&1 && [ "$SECONDS" -lt "$deadline" ]; do sleep 0.5; done
        pkill -KILL -g "$demo_pid" 2>/dev/null || true
    fi
    # Either kill leaves the app's embedded PostgreSQL up on the demo profile,
    # the one the person's own `--demo` app uses: a killed app runs no exit
    # handler (#1967). After a ⌘Q that worked there is nothing here.
    if [ -n "$pg_data" ]; then
        local said stopped=0
        said=$(stop_left_postgres "$pg_data" "$pg_before" "$wait_s" 2>&1) || stopped=$?
        [ -z "$said" ] || say "$said"
        if [ "$stopped" -ne 0 ] && [ "$status" -eq 0 ]; then status=1; fi
    fi
    # What live_session's own EXIT trap runs, which this trap replaced.
    [ -z "$product" ] || { _live_restore; live_lock_release; }
    rm -rf "$scratch"
    rmdir "$LOCK" 2>/dev/null || true
    exit "$status"
}
trap cleanup EXIT
trap 'trap - INT; kill -INT $$' INT
trap 'trap - TERM; kill -TERM $$' TERM
# desktop-witness-test.sh's refusals run with this: past the lock it stops,
# so a broken lock cannot start the app inside a gate.
[ -z "${LIVE_SHOT_DRY_RUN:-}" ] || { say "dry run: the lock is held; not starting knobas"; exit 0; }

say "compiling the accessibility helper (the desktop witness's ax.swift)"
ax=$scratch/ax
swiftc -O -o "$ax" "$here/desktop-witness/ax.swift"
ready() {
    local verdict
    verdict=$(classify_readiness "$("$ax" probe 2>&1 || true)")
    [ "$verdict" = granted ] || die "$(readiness_message "$verdict")"
}
ready
running=$(pgrep -x knobas-app | tr '\n' ' ' || true)
[ -z "${running// /}" ] || die "knobas is already running (pid ${running% }); quit it first, a shot names one window"
! lsof -nP -iTCP:1420 -sTCP:LISTEN >/dev/null 2>&1 \
    || die "port 1420 is taken: another 'just demo' or 'just dev' is up"

frontmost() {
    [ "$(tsv_field "$("$ax" frontmost 2>/dev/null || true)" pid)" = "$app_pid" ] && return 0
    "$ax" activate "$app_pid" >/dev/null 2>&1 || true
    [ "$(tsv_field "$("$ax" frontmost 2>/dev/null || true)" pid)" = "$app_pid" ] \
        || die "knobas is not frontmost and would not come forward; not sending input to another app"
}
shot() {
    [ $# -eq 1 ] || die "usage: shot <name>"
    ready
    local id width file=$out/$1.png
    read -r id width _ < <("$ax" window-id "$app_pid") || die "shot $1: knobas has no window on screen"
    screencapture -x -o -l "$id" "$file"
    sips --resampleWidth "$width" "$file" >/dev/null
    say "shot: $file"
}
# reach <verb> <ax subcommand> <label> [n]: the subcommand, retried until it
# lands, as `just shot`'s click: waits, since a view draws after the step that
# asked for it. Under the guard (KNOBAS_WITNESS_WAIT_S, #1961), and at once
# when knobas has exited; the last attempt's complaint is the one printed.
reach_once() {
    local sub=$1
    shift
    "$ax" "$sub" "$app_pid" "$@" 2>"$scratch/reach.err"
}
reach() {
    local verb=$1 sub=$2 polled=0
    shift 2
    witness_poll "$app_pid" "$wait_s" reach_once "$sub" "$@" || polled=$?
    [ "$polled" != 0 ] || return 0
    cat "$scratch/reach.err" >&2
    [ "$polled" != 3 ] || die "$verb $*: knobas (pid $app_pid) exited while this waited"
    die "$verb $* (waited $wait_s s, KNOBAS_WITNESS_WAIT_S)"
}
press() { ready; reach press press-visible "$@"; }
focus() { ready; reach focus focus-visible "$@"; }
click-at() { ready; frontmost; reach click-at click-at "$@"; }
# Shadows bash's `type` builtin from here on, as the ticket names the command;
# nothing after this line, nor a steps file, may use the builtin.
type() { ready; frontmost; "$ax" type "$*" || die "type"; }
key() {
    [ $# -ge 1 ] || die "usage: key <key> [command|shift|option|control ...]"
    local code
    code=$(key_code "$1") || die "key: no key called '$1'"
    shift
    ready
    frontmost
    "$ax" key "$code" "$@" || die "key"
}
settle() { sleep "${1:-1}"; }

say "starting 'just demo' with KNOBAS_SECRET_STORE=memory (log: $out/demo.log)"
pg_data=$(demo_data_dir "$HOME")
pg_before=$(profile_postmaster "$pg_data")
# Its own process group, so the dev server and cargo go with it at the end.
set -m
env -C "$repo" KNOBAS_SECRET_STORE=memory just demo >"$out/demo.log" 2>&1 &
demo_pid=$!
set +m
for _ in $(seq 1 1800); do
    app_pid=$(pgrep -x knobas-app | head -1 || true)
    [ -n "$app_pid" ] && break
    kill -0 "$demo_pid" 2>/dev/null || die "'just demo' exited before knobas started; see $out/demo.log"
    sleep 0.5
done
[ -n "$app_pid" ] || die "no knobas process within 15 minutes; see $out/demo.log"
say "knobas is pid $app_pid; waiting for its window"
"$ax" wait-window "$app_pid" 300 >/dev/null || die "knobas (pid $app_pid) showed no window in 5 minutes"
"$ax" wait-content "$app_pid" 300 || die "knobas' window stayed blank for 5 minutes; see $out/demo.log"
"$ax" activate "$app_pid" || die "knobas would not come forward"
if [ -n "$aerospace" ]; then
    read -r window _ < <("$ax" window-id "$app_pid") || die "AeroSpace: knobas has no window on screen"
    aerospace fullscreen on --window-id "$window" --no-outer-gaps || die "AeroSpace refused to fullscreen window $window"
    say "AeroSpace: window $window fullscreen"
fi
settle 3

say "running $steps"
# shellcheck source=/dev/null
. "$steps"
say "done: $out"
