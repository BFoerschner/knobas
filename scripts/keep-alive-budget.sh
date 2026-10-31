#!/usr/bin/env bash
# Keep-alive's budget and memory witness (#2250): starts the `?fake-ipc` dev server on
# 5300 + slot (or reuses one already there, as scripts/shot.sh does), runs
# scripts/keep-alive-budget.mjs in headless Chrome on debug port 9400 + slot, and
# stops the server it started. Exit 1 when the tab return is over budget.
#
# Usage: scripts/keep-alive-budget.sh <slot>
# Env:   KEEP_ALIVE_RETURNS, KEEP_ALIVE_BUDGET_MS (scripts/keep-alive-budget.mjs),
#        SHOT_START_WAIT_S (the dev server's start guard, default 120 s).
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: $0 <slot>" >&2; exit 2; }
slot=$1
case $slot in ''|*[!0-9]*) echo "keep-alive: the slot must be a number, got '$slot'" >&2; exit 2 ;; esac
port=$((5300 + slot)); wait_s=${SHOT_START_WAIT_S:-120}
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
profile="${TMPDIR:-/tmp}/knobas-keep-alive-profile-$slot"
mkdir -p "$profile"
up() { curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null; }
vite_pid=
stop() { [ -z "$vite_pid" ] || { pkill -TERM -P "$vite_pid" 2>/dev/null || true; kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true; }; }
trap stop EXIT
if ! up; then
  log="${TMPDIR:-/tmp}/knobas-keep-alive-vite-$slot.log"
  (cd "$root/app" && exec npx vite --port "$port" --strictPort >"$log" 2>&1) &
  vite_pid=$!
  from=$SECONDS
  until up; do
    kill -0 "$vite_pid" 2>/dev/null || { echo "keep-alive: the dev server on :$port exited before it answered; log: $log" >&2; exit 1; }
    [ $((SECONDS - from)) -lt "$wait_s" ] || { echo "keep-alive: the dev server on :$port did not answer within $wait_s s; log: $log" >&2; exit 1; }
    sleep 0.5
  done
fi
"$root/scripts/machine-load.sh" || true
node "$root/scripts/keep-alive-budget.mjs" "$port" $((9400 + slot)) "$profile"
