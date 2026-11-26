#!/usr/bin/env bash
# The starts-of-git probe (testenv/perf/README.md, *Starts of git*): builds the app as the probes measure it, serves it
# on loopback for the length of the run, presses each action at a cloned repository in a headless Chrome at each
# throttle, and fails when a press a person waits on started `git` at all.
#
#   testenv/perf/git-starts.sh --real <port> --shim-log <file> [--delay <ms>] [--throttle 1,4,6] [--only a,b]
#                              [--items <file.json>] [--out <dir>] [--count]
#
# --real <port>   the windowless backend (examples/headless_ipc.rs) on a profile whose items' repositories are cloned,
#                 started with `testenv/perf/git-shim` first on its PATH (the README has the lines)
# --shim-log      the file that backend's shim appends to (its KNOBAS_GIT_SHIM_LOG)
# --delay <ms>    what its shim sleeps a start (its KNOBAS_GIT_SHIM_DELAY, in ms here), to tell starts at once from
#                 starts in a row
# --count         count and time only: no failure for a start (the before of a comparison)
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" log="" delay="" throttle="1,4,6" only="" items="" out="" check=--check
usage() { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --shim-log) log=$2; shift 2 ;;
    --delay) delay=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --items) items=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --count) check=""; shift ;;
    *) usage ;;
  esac
done
[ -n "$real" ] && [ -n "$log" ] || usage
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-git-starts.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5362}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time"
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/git-starts.mjs" --app "$url" --shim-log "$log" \
  --throttle "$throttle" --out "$out/git-starts.json" ${delay:+--delay "$delay"} ${only:+--only "$only"} \
  ${items:+--items "$items"} $check
status=$?
echo "git-starts: results in $out"
exit $status
