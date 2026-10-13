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
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/debug/knobas-app"
[ -x "$bin" ] || { echo "build first: env -u RUSTUP_TOOLCHAIN cargo build -p knobas-app" >&2; exit 2; }
sock=/tmp/dev_knobas_desktop_si.sock
[ -e "$sock" ] && { echo "refusing: $sock exists, a knobas is running" >&2; exit 2; }
tmp=$(mktemp -d)
mode=${1:-second}
export HOME="$tmp" KNOBAS_DB_URL="postgres://x:x@127.0.0.1:1/x"
marker="using an externally managed postgres"
first_pid="" ; second_pid=""
cleanup() { [ -n "$second_pid" ] && kill "$second_pid" 2>/dev/null; [ -n "$first_pid" ] && kill "$first_pid" 2>/dev/null; wait 2>/dev/null; rm -rf "$tmp"; rm -f "$sock"; }
trap cleanup EXIT
fail() { echo "FAIL: $*"; echo "--- first"; cat "$tmp/first.log"; echo "--- second"; cat "$tmp/second.log" 2>/dev/null; exit 1; }

"$bin" >"$tmp/first.log" 2>&1 & first_pid=$!
for _ in $(seq 100); do grep -q "$marker" "$tmp/first.log" && [ -S "$sock" ] && break; sleep 0.2; done
grep -q "$marker" "$tmp/first.log" || fail "first instance never reached bring-up"
[ -S "$sock" ] || fail "first instance holds no single-instance socket"

if [ "$mode" = "--demo-beside" ]; then
  "$bin" --demo >"$tmp/second.log" 2>&1 & second_pid=$!
  # The demo profile ignores KNOBAS_DB_URL and would provision its own scratch
  # postgres; it is killed on the cleanup trap right after it reaches setup.
  for _ in $(seq 50); do grep -q "provisioning postgres\|starting the embedded" "$tmp/second.log" && break; sleep 0.2; done
  grep -q "provisioning postgres\|starting the embedded" "$tmp/second.log" || fail "demo launch was stopped by the guard"
  kill -0 "$second_pid" 2>/dev/null || fail "demo launch exited"
  echo "PASS: --demo ran beside the normal instance (reached its own bring-up, still running)"
  exit 0
fi

"$bin" >"$tmp/second.log" 2>&1 & second_pid=$!
for _ in $(seq 50); do kill -0 "$second_pid" 2>/dev/null || break; sleep 0.2; done
kill -0 "$second_pid" 2>/dev/null && fail "second launch is still running"
wait "$second_pid"; rc=$?; second_pid=""
[ "$rc" = 0 ] || fail "second launch exited $rc"
grep -q "$marker" "$tmp/second.log" && fail "second launch reached database startup"
grep -q "profile" "$tmp/second.log" && fail "second launch reached setup"
kill -0 "$first_pid" 2>/dev/null || fail "first instance died"
[ "$(grep -c "$marker" "$tmp/first.log")" = 1 ] || fail "first instance started the database more than once"
echo "PASS: second launch exited 0 before setup/database startup; first still running"
