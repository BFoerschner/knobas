#!/usr/bin/env bash
# The frame budget (testenv/perf/README.md): builds the app as the probes measure it, serves it on loopback for the
# length of the run, drives it in a headless Chrome at each throttle, and fails when a list, a key or a page switch is
# over its line in budget.json, or when a box that scrolls has no opaque background.
#
#   testenv/perf/frame-budget.sh [--real <port>] [--throttle 1,4] [--only a,b | --all] [--out <dir>]
#
# By default the lists and moments the smoothness targets name (`only` below, about 15 minutes); --all runs every
# scenario of run.mjs, an hour or more on the fixture, whose own query engine runs in the page.
#
# --real <port>  the windowless backend on a real database (examples/headless_ipc.rs) instead of the fixture at
#                scale (?fake-scale); the budget's "real" lines then apply
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4" out=""
only="monitors,code-prs,tickets-list,inbox,code-filter,monitors-filter,#/inbox,#/code,#/monitors,#/docs,#/settings"
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --all) only=""; shift ;;
    --out) out=$2; shift 2 ;;
    *) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-frame-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5361}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
if [ -n "$real" ]; then corpus="fake-real=$real" lines=real; else corpus="fake-scale" lines=fake; fi
url="http://127.0.0.1:$port/?fake-ipc&$corpus#/inbox"
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/run.mjs" --app "$url" --out "$out/run.json" \
  --label "frame-budget $(git -C "$root" rev-parse --short HEAD)" --throttle "$throttle" --kinds scroll,type,nav ${only:+--only "$only"} || exit 1
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/scrollers.mjs" --app "$url" --strict --out "$out/scrollers.json" > "$out/scrollers.log"
scrollers=$?
tail -1 "$out/scrollers.log"
node "$here/budget.mjs" "$out/run.json" --lines "$lines" --md "$out/budget.md"
budget=$?
echo "frame-budget: results in $out"
[ $budget -eq 0 ] && [ $scrollers -eq 0 ]
