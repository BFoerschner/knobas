#!/usr/bin/env bash
# The open budget (testenv/perf/README.md, *Opening a page*): builds the app as the probes measure it, serves it on
# loopback for the length of the run, presses every place's nav entry or row in a headless Chrome at each throttle, on
# its first visit, its return, its return after every other section, and its return after a sync, and fails when an
# open's first useful frame or its settling is over its line in `budget-open.json`, or a row moved after it drew.
#
#   testenv/perf/open-budget.sh --real <port> [--inner] [--forge <ms>] [--throttle 1,4] [--only a,b] [--out <dir>]
#
# --real <port>  the windowless backend on the saved scale database (examples/headless_ipc.rs); the places are its
#                pages (the SCLENG space, a TeamCity pipeline), so there is no fixture run
# --inner        the inner pages instead (`open.mjs --set inner`): a repository's landing, tabs and files, a commit,
#                Compare, a pull request's tabs and a run's tabs, against a live forge, checked against `inner` lines
# --forge <ms>   with --inner: hold each request to Gitea <ms> through `slow-forge.mjs` for the length of the run,
#                checked against `forge` lines; the profile's Gitea source must name https://127.0.0.1:3900
#                (README, *The inner pages*)
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4" only="" out="" inner="" forge=""
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --inner) inner=1; shift ;;
    --forge) forge=$2; shift 2 ;;
    *) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -n "$real" ] || { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[ -z "$forge" ] || [ -n "$inner" ] || { echo "open-budget: --forge measures the inner pages: add --inner" >&2; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5362}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time"
set_args=() lines=real
if [ -n "$inner" ]; then
  set_args=(--set inner --rounds first,return,synced)
  lines=inner
fi
proxy=""
if [ -n "$forge" ]; then
  lines=forge
  node "$here/slow-forge.mjs" --listen 3900 --upstream 127.0.0.1:3443 --delay "$forge" > "$out/slow-forge.log" 2>&1 &
  proxy=$!
  trap 'kill "$proxy" 2>/dev/null' EXIT
  sleep 1
fi
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/open.mjs" --app "$url" --out "$out/open.json" \
  --throttle "$throttle" ${only:+--only "$only"} "${set_args[@]}" --check "$here/budget-open.json" --lines "$lines"
status=$?
echo "open-budget: results in $out"
exit $status
