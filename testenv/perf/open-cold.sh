#!/usr/bin/env bash
# Recent's first open on a cold start (#2569b, testenv/perf/README.md, *Opening a page*): starts the windowless backend
# afresh for each round, loads the page the moment it answers, and presses Recent as soon as the nav draws
# (`builds-recent-start`), so its first read meets bring-up's background work and every start-up read. `open-budget`'s
# places reload the page on a backend that has been up for minutes, which no start of the app does. Checked against
# `builds-recent-start`'s lines.
#
#   testenv/perf/open-cold.sh --backend <headless_ipc> --home <a copy of the saved home> [--rounds 5] [--throttle 1]
#                             [--port 5490] [--out <dir>] [--live] [--after <seconds>]
#
# --backend  a release `headless_ipc` (`cargo build --release -p knobas-app --features test-util --example headless_ipc`)
# --home     a copy of the saved scale home, its dev-secrets.json moved out (README, *On a real database*); never the
#            original: each round starts its database
# --live     a start with live sources syncing (#2612): adds `perf-gitea` and `perf-teamcity` to the copy, on the local
#            Gitea and TeamCity this checkout seeded (`testenv/seed-state.json`), each at its adapter's shortest
#            interval, and waits for their first syncs; every round's start then finds them due
# --after    load the page this many seconds after the backend answers (0); its runs start 2 s after it does
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
backend="" home="" rounds=5 throttle=1 port=5490 out="" live="" after=0
while [ $# -gt 0 ]; do
  case $1 in
    --backend) backend=$2; shift 2 ;;
    --home) home=$2; shift 2 ;;
    --rounds) rounds=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --port) port=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --live) live=1; shift ;;
    --after) after=$2; shift 2 ;;
    *) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -x "$backend" ] && [ -d "$home" ] || { sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-cold.XXXXXX")}
mkdir -p "$out"
app_port=${KNOBAS_PERF_PORT:-5362}
data="$home/Library/Application Support/dev.knobas.desktop"
be=""
trap '[ -z "$be" ] || kill "$be" 2>/dev/null' EXIT

# One command to the backend on $port: `ipc <cmd> [<args json>]`.
ipc() {
  local args=${2:-'{}'}
  jq -cn --arg c "$1" --argjson a "$args" '{cmd: $c, args: $a}' |
    curl -s -X POST -H 'content-type: application/json' --data-binary @- "http://127.0.0.1:$port/"
}
start_backend() { # <log> [--timings <file>]
  local log=$1; shift
  # No update check from a release build started this often (ADR-0052).
  HOME="$home" KNOBAS_UPDATE=off "$backend" --data-dir "$data" --port "$port" "$@" > "$log" 2>&1 &
  be=$!
  for _ in $(seq 1 1200); do grep -q 'answering on' "$log" && break; kill -0 "$be" 2>/dev/null || break; sleep 0.05; done
  grep -q 'answering on' "$log" || { cat "$log" >&2; exit 1; }
}
stop_backend() {
  curl -s -X POST "http://127.0.0.1:$port/quit" > /dev/null
  for _ in $(seq 1 300); do kill -0 "$be" 2>/dev/null || break; sleep 0.1; done
  be=""
}
# Until no run is in flight (30 s at most), so the next start finds the sources due rather than a run left open.
runs_settle() {
  for _ in $(seq 1 60); do
    [ "$(ipc sync_status | jq '[.ok[]? | select(.started_at != null)] | length' 2>/dev/null || echo 0)" = 0 ] && return
    sleep 0.5
  done
}

if [ -n "$live" ]; then
  state=$root/testenv/seed-state.json
  gitea_token=$(jq -r '.gitea.token // empty' "$state" 2>/dev/null)
  teamcity_token=$(jq -r '.teamcity.token // empty' "$state" 2>/dev/null)
  owner=$(jq -r '.gitea.org // "tidewater"' "$state" 2>/dev/null)
  [ -n "$gitea_token" ] && [ -n "$teamcity_token" ] ||
    { echo "open-cold: --live needs Gitea and TeamCity seeded from this checkout: just live-up teamcity" >&2; exit 2; }
  { curl -sf -o /dev/null -m 5 http://127.0.0.1:3000/api/healthz && curl -s -o /dev/null -m 5 http://127.0.0.1:8111/; } ||
    { echo "open-cold: --live needs the local Gitea (3000) and TeamCity (8111) answering: just live-up teamcity" >&2; exit 2; }
  start_backend "$out/live-setup.log"
  for source in \
    "$(jq -cn --arg t "$gitea_token" --arg o "$owner" '{id: "perf-gitea", adapter_kind: "gitea",
        display_name: "Gitea (syncing at start)", base_url: "http://127.0.0.1:3000", auth_kind: "Pat",
        config: {owners: [$o], username: "knobas"}, secret: {value: $t}, sync_interval_secs: 60, enabled: true}')" \
    "$(jq -cn --arg t "$teamcity_token" '{id: "perf-teamcity", adapter_kind: "teamcity",
        display_name: "TeamCity (syncing at start)", base_url: "http://127.0.0.1:8111", auth_kind: "Pat",
        config: {}, secret: {value: $t}, sync_interval_secs: 20, enabled: true}')"; do
    id=$(jq -r .id <<< "$source")
    if ! ipc add_source "$(jq -cn --argjson s "$source" '{input: $s}')" | jq -e .ok > /dev/null; then
      # There already: the token this checkout holds now.
      ipc set_source_secret "$(jq -cn --arg id "$id" --argjson s "$source" '{id: $id, secret: $s.secret}')" |
        jq -e .ok > /dev/null || { echo "open-cold: $id refused its credential" >&2; exit 1; }
    fi
  done
  for _ in $(seq 1 120); do
    [ "$(ipc sync_status | jq '[.ok[]? | select((.source_id | startswith("perf-")) and .last_finished_at != null
        and .started_at == null)] | length')" = 2 ] && break
    sleep 5
  done
  ipc sync_status | jq -r '.ok[]? | select(.source_id | startswith("perf-")) |
    "open-cold: \(.source_id) first sync \(.last_outcome // "not finished")"'
  stop_backend
fi

KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
status=0
for round in $(seq 1 "$rounds"); do
  dir="$out/round-$round"
  mkdir -p "$dir"
  start_backend "$dir/backend.log" --timings "$dir/ipc.jsonl"
  answered=$(python3 -c 'import time; print(time.time() * 1000)')
  echo "open-cold: round $round, load $(sysctl -n vm.loadavg 2>/dev/null || cut -d' ' -f1-3 /proc/loadavg)"
  sleep "$after"
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$app_port" node "$here/open.mjs" \
    --app "http://127.0.0.1:$app_port/?fake-ipc&fake-hooks&fake-real=$port#/time" --out "$dir/open.json" \
    --throttle "$throttle" --only builds-recent-start --rounds first --max-minutes 10 \
    --check "$here/budget-open.json" --lines real || status=1
  if [ -n "$live" ]; then
    runs_settle
    ipc list_sync_runs '{"limit": 20}' > "$dir/sync-runs.json"
  fi
  # Recent's first read at the backend: when it arrived, its time there, the pool it met, and the runs in flight.
  python3 -I - "$dir" "$answered" <<'EOF'
import json, sys
from datetime import datetime
d, answered = sys.argv[1], float(sys.argv[2])
first = next((json.loads(l) for l in open(f"{d}/ipc.jsonl") if '"recent_runs"' in l), None)
if first is None:
    sys.exit(print("open-cold: no recent_runs reached the backend"))
syncing = []
try:
    for run in json.load(open(f"{d}/sync-runs.json")).get("ok", []):
        ms = lambda t: datetime.fromisoformat(t.replace("Z", "+00:00")).timestamp() * 1000
        if ms(run["started_at"]) <= first["arrived"] and (run["finished_at"] is None or first["arrived"] < ms(run["finished_at"])):
            syncing.append(run["source_id"])
except FileNotFoundError:
    syncing = None
print(f"open-cold: recent_runs arrived {(first['arrived'] - answered) / 1000:.2f} s after the backend answered,"
      f" {first['ms']:.1f} ms at the backend, pool {first.get('pool_idle')} of {first.get('pool_size')} idle"
      + ("" if syncing is None else f", syncing: {', '.join(syncing) or 'none'}"))
EOF
  stop_backend
done
echo "open-cold: results in $out"
exit $status
