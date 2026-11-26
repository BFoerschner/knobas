#!/usr/bin/env bash
# The Monitors budget (testenv/perf/README.md, *Monitors at 239 Hz*; the monitors-page pass, 2026-11-19): builds the app
# as the probes measure it, serves it on loopback for the length of the run, and runs `monitors.mjs` over the windowless
# backend at each size, at 1x, 4x and 6x: the first open, a return, the page idle, a beat landing and four more, a scroll,
# a hover sweep and a monitor opened and closed, each under a Chrome trace. Fails when a moment is over its line in
# `budget-monitors.json`: its open, the main thread's tasks over a 239 Hz frame (4.2 ms), its work a presented frame,
# or the rows a beat drew again.
#
#   testenv/perf/monitors-budget.sh --real <port> [--sizes "WxH@s ..."] [--throttle 1,4,6] [--beat "<command>"]
#                                   [--attach <debug port>] [--out <dir>]
#
# --real <port>  the windowless backend on a copy of the saved scale database (examples/headless_ipc.rs), with a day
#                of beats in it (the README says how)
# --sizes        1440x868@1, 2560x1440@1.25 and 5120x1440@1 by default
# --beat         a command run before each beat's `sync:state`: one that appends a sample to every monitor, as a poll
#                does, so a beat draws what a poll would
# --attach       a headless Chrome already listening (on the WSL box through a tunnel), as `keys.mjs --attach`
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4,6" out="" beat="" attach="" sizes="1440x868@1 2560x1440@1.25 5120x1440@1"
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --sizes) sizes=$2; shift 2 ;;
    --beat) beat=$2; shift 2 ;;
    --attach) attach=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -n "$real" ] || { sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-monitors-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5364}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time"
status=0
for size in $sizes; do
  echo "monitors-budget: $size"
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/monitors.mjs" --app "$url" --out "$out/monitors-$size.json" \
    --size "$size" --throttle "$throttle" ${beat:+--beat "$beat"} ${attach:+--attach "$attach"} --check "$here/budget-monitors.json" || status=1
done
echo "monitors-budget: results in $out"
exit $status
