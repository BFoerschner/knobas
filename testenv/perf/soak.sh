#!/usr/bin/env bash
# The soak (testenv/perf/README.md, *The soak*): builds the app as the probes measure it, serves it on loopback for the
# length of the run, browses a repository's Code page as a person does for --minutes (`soak.mjs`) and then every other
# page (`soak.mjs --tour`), and fails when what a run's end or a return costs a hover grows with the browsing, or the
# nodes, listeners or heap kept do: `budget.json`'s `soak` and `soak-tour` lines.
#
#   testenv/perf/soak.sh --real <port> --repo <entity id> [--minutes 10] [--tour-laps 5] [--throttle 1,4,6]
#                        [--size WxH@s] [--out <dir>]
#
# --real <port>  the windowless backend (examples/headless_ipc.rs) on a copy of the saved scale database
# --repo         a repository of it with a clone and a tree of thousands of files (`soak-repo.sh` builds one over the
#                stand-in forge; the README's *The soak*)
# --minutes      how long the Code page is browsed: 10 by default, about 55 laps and 130 folders opened, where the
#                re-read of every folder took a hover from one frame to a third of a second; 30 to 60 for a long run
# --tour-laps    how many times every other page is visited; 0 leaves the tour out
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" repo="" minutes=10 tour=5 throttle="1,4,6" size="" out=""
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --repo) repo=$2; shift 2 ;;
    --minutes) minutes=$2; shift 2 ;;
    --tour-laps) tour=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -n "$real" ] && [ -n "$repo" ] || { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-soak.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5364}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/code"
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/soak.mjs" --app "$url" --repo "$repo" \
  --minutes "$minutes" --throttle "$throttle" ${size:+--size "$size"} --out "$out/soak.json" \
  --check "$here/budget.json" --lines real
status=$?
if [ "$tour" != 0 ]; then
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/soak.mjs" --tour --app "$url" --repo "$repo" \
    --laps "$tour" --throttle "$throttle" ${size:+--size "$size"} --out "$out/soak-tour.json" \
    --check "$here/budget.json" --lines real || status=1
fi
echo "soak: results in $out"
exit $status
