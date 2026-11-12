#!/usr/bin/env bash
# The open budget (testenv/perf/README.md, *Opening a page*): builds the app as the probes measure it, serves it on
# loopback for the length of the run, presses every place's nav entry or row in a headless Chrome at each throttle, on
# its first visit, its return, its return after every other section, and its return after a sync, and fails when an
# open's first useful frame or its settling is over its line in `budget-open.json`, or a row moved after it drew.
#
#   testenv/perf/open-budget.sh --real <port> [--throttle 1,4] [--only a,b] [--out <dir>]
#
# --real <port>  the windowless backend on the saved scale database (examples/headless_ipc.rs); the places are its
#                pages (the SCLENG space, a TeamCity pipeline), so there is no fixture run
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4" only="" out=""
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -n "$real" ] || { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5362}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time"
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/open.mjs" --app "$url" --out "$out/open.json" \
  --throttle "$throttle" ${only:+--only "$only"} --check "$here/budget-open.json" --lines real
status=$?
echo "open-budget: results in $out"
exit $status
