#!/usr/bin/env bash
# Recent's first open on a cold start (#2569b, testenv/perf/README.md, *Opening a page*): starts the windowless backend
# afresh for each round, loads the page the moment it answers, and presses Recent as soon as the nav draws
# (`builds-recent-start`), so its first read meets bring-up's background work and every start-up read. `open-budget`'s
# places reload the page on a backend that has been up for minutes, which no start of the app does. Checked against
# `builds-recent-start`'s lines.
#
#   testenv/perf/open-cold.sh --backend <headless_ipc> --home <a copy of the saved home> [--rounds 5] [--throttle 1]
#                             [--port 5490] [--out <dir>]
#
# --backend  a release `headless_ipc` (`cargo build --release -p knobas-app --features test-util --example headless_ipc`)
# --home     a copy of the saved scale home, its dev-secrets.json moved out (README, *On a real database*); never the
#            original: each round starts its database
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
backend="" home="" rounds=5 throttle=1 port=5490 out=""
while [ $# -gt 0 ]; do
  case $1 in
    --backend) backend=$2; shift 2 ;;
    --home) home=$2; shift 2 ;;
    --rounds) rounds=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --port) port=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -x "$backend" ] && [ -d "$home" ] || { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-cold.XXXXXX")}
mkdir -p "$out"
app_port=${KNOBAS_PERF_PORT:-5362}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
data="$home/Library/Application Support/dev.knobas.desktop"
be=""
trap '[ -z "$be" ] || kill "$be" 2>/dev/null' EXIT
status=0
for round in $(seq 1 "$rounds"); do
  dir="$out/round-$round"
  mkdir -p "$dir"
  # No update check from a release build started this often (ADR-0052).
  HOME="$home" KNOBAS_UPDATE=off "$backend" --data-dir "$data" --port "$port" --timings "$dir/ipc.jsonl" > "$dir/backend.log" 2>&1 &
  be=$!
  for _ in $(seq 1 1200); do grep -q 'answering on' "$dir/backend.log" && break; kill -0 "$be" 2>/dev/null || break; sleep 0.05; done
  grep -q 'answering on' "$dir/backend.log" || { cat "$dir/backend.log" >&2; exit 1; }
  echo "open-cold: round $round, load $(sysctl -n vm.loadavg 2>/dev/null || cut -d' ' -f1-3 /proc/loadavg)"
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$app_port" node "$here/open.mjs" \
    --app "http://127.0.0.1:$app_port/?fake-ipc&fake-hooks&fake-real=$port#/time" --out "$dir/open.json" \
    --throttle "$throttle" --only builds-recent-start --rounds first --max-minutes 10 \
    --check "$here/budget-open.json" --lines real || status=1
  curl -s -X POST "http://127.0.0.1:$port/quit" > /dev/null
  for _ in $(seq 1 300); do kill -0 "$be" 2>/dev/null || break; sleep 0.1; done
  be=""
done
echo "open-cold: results in $out"
exit $status
