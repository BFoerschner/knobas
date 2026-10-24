#!/usr/bin/env bash
# The single-instance witness (#748): a second launch of the dev binary must
# exit before it reaches database startup.
#
# Hermetic: HOME points at a temp dir (so app_data_dir is not the real one) and
# KNOBAS_DB_URL at a closed port (so no database is touched). The observable is
# the bring-up line `KNOBAS_DB_URL is set: using an externally managed
# postgres`, which `spawn_bring_up` logs first and which a guarded second
# launch must never print. Opens a window (the debug build); not in the gate.
#
# Usage: scripts/single-instance-witness.sh [--demo-beside]
#   default       first launch normal; second normal must exit 0, no bring-up.
#   --demo-beside first launch normal; a --demo launch must NOT be refused
#                 (it reaches its own bring-up and keeps running).
# Env:  KNOBAS_SI_WAIT_S=120  the guard on each wait below, whole seconds
#       KNOBAS_SI_BIN         the binary (default: this checkout's debug build)
#
# Each wait polls for the state it needs and fails at once when the launch it
# watches has exited; only a launch that hangs runs out the guard (#1961). A
# loaded machine starts the binary slowly, and the old fixed 10 s and 20 s
# failed runs with nothing wrong in the app.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
bin=${KNOBAS_SI_BIN:-$root/target/debug/knobas-app}
wait_s=${KNOBAS_SI_WAIT_S:-120}
case $wait_s in ''|0*|*[!0-9]*) echo "KNOBAS_SI_WAIT_S must be a whole number of seconds, got '$wait_s'" >&2; exit 2 ;; esac
[ -x "$bin" ] || { echo "build first: env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app" >&2; exit 2; }
sock=/tmp/dev_knobas_desktop_si.sock
[ -e "$sock" ] && { echo "refusing: $sock exists, a knobas is running" >&2; exit 2; }
tmp=$(mktemp -d)
mode=${1:-second}
export HOME="$tmp" KNOBAS_DB_URL="postgres://x:x@127.0.0.1:1/x"
marker="using an externally managed postgres"
first_pid="" ; second_pid=""
# The --demo launch starts an embedded PostgreSQL on its own profile under the
# temp HOME, and the kill below leaves it up: a killed app runs no exit
# handler (#1967). It is stopped before the HOME it lives in is deleted.
# shellcheck source=testenv/desktop-witness-lib.sh disable=SC1091
. "$root/testenv/desktop-witness-lib.sh"
pg_data=""
cleanup() {
  local status=$? said
  [ -n "$second_pid" ] && kill "$second_pid" 2>/dev/null; [ -n "$first_pid" ] && kill "$first_pid" 2>/dev/null; wait 2>/dev/null
  if [ -n "$pg_data" ]; then
    said=$(stop_left_postgres "$pg_data" "" "$wait_s" 2>&1) || status=1
    [ -z "$said" ] || echo "$said"
  fi
  rm -rf "$tmp"; rm -f "$sock"
  exit "$status"
}
trap cleanup EXIT
fail() { echo "FAIL: $*"; echo "--- $sock: $([ -S "$sock" ] && echo a socket || echo absent)"; echo "--- first"; cat "$tmp/first.log"; echo "--- second"; cat "$tmp/second.log" 2>/dev/null; exit 1; }
# poll <what> <pid> <predicate...>: until the predicate holds. Fails at once
# when <pid> has exited first, and at the guard otherwise; both name <what>.
poll() {
  local what=$1 pid=$2 from=$SECONDS rc
  shift 2
  until "$@"; do
    if ! kill -0 "$pid" 2>/dev/null; then
      "$@" && return 0
      wait "$pid"; rc=$?
      fail "$what: it exited (status $rc) first"
    fi
    [ $((SECONDS - from)) -lt "$wait_s" ] || fail "$what within $wait_s s (KNOBAS_SI_WAIT_S)"
    sleep 0.2
  done
}
first_up() { grep -q "$marker" "$tmp/first.log" && [ -S "$sock" ]; }
demo_up() { grep -q "provisioning postgres\|starting the embedded" "$tmp/second.log"; }

"$bin" >"$tmp/first.log" 2>&1 & first_pid=$!
poll "the first instance did not reach bring-up and hold its socket" "$first_pid" first_up

if [ "$mode" = "--demo-beside" ]; then
  pg_data=$(demo_data_dir "$tmp")
  "$bin" --demo >"$tmp/second.log" 2>&1 & second_pid=$!
  # The demo profile ignores KNOBAS_DB_URL and would provision its own scratch
  # postgres; it is killed on the cleanup trap right after it reaches setup.
  poll "the demo launch did not reach its own bring-up (refused as a second instance?)" "$second_pid" demo_up
  kill -0 "$second_pid" 2>/dev/null || fail "demo launch exited"
  echo "PASS: --demo ran beside the normal instance (reached its own bring-up, still running)"
  exit 0
fi

"$bin" >"$tmp/second.log" 2>&1 & second_pid=$!
# Until it exits; one that gets past the guard fails as soon as it says so.
from=$SECONDS
while kill -0 "$second_pid" 2>/dev/null; do
  grep -q "$marker" "$tmp/second.log" && fail "second launch reached database startup"
  grep -q "profile" "$tmp/second.log" && fail "second launch reached setup"
  [ $((SECONDS - from)) -lt "$wait_s" ] || fail "second launch is still running after $wait_s s (KNOBAS_SI_WAIT_S)"
  sleep 0.2
done
wait "$second_pid"; rc=$?; second_pid=""
[ "$rc" = 0 ] || fail "second launch exited $rc"
grep -q "$marker" "$tmp/second.log" && fail "second launch reached database startup"
grep -q "profile" "$tmp/second.log" && fail "second launch reached setup"
kill -0 "$first_pid" 2>/dev/null || fail "first instance died"
[ "$(grep -c "$marker" "$tmp/first.log")" = 1 ] || fail "first instance started the database more than once"
echo "PASS: second launch exited 0 before setup/database startup; first still running"
