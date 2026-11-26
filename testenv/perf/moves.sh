#!/usr/bin/env bash
# The movement gate (the nothing-moves pass of PR #2479, 2026-11-17; Björn: "Check if any other page moves after it's
# drawn. That's not good behavior."): builds the app as the probes measure it, serves it on loopback for the length of
# the run, opens every place a person can open (`moves.mjs`) on its first open and its return, and fails when anything
# drawn moves or changes size by more than 2 px in the 5 s after the place's first useful frame.
#
#   testenv/perf/moves.sh [--real <port>] [--fixture-only] [--throttle 1,4] [--sizes "1440x868@1 2560x1440@1.25"]
#                         [--sync] [--only a,b] [--workers 3] [--attach <debug port>] [--out <dir>] [--no-columns]
#                         [--part k/m]
#
# By default the fixture (`?fake-scale`, each read landing late with `fake-late`, so a late read shows as a move) at 1x
# and 4x at both sizes. `--real <port>` adds the windowless backend on the saved scale database
# (examples/headless_ipc.rs), the same throttles and sizes. `--sync`: every source ends a run that changed something
# every 1.5 s while each place records. `--workers` runs that many browsers at once, each a share of the places (a
# move is a frame order, not a time, so the share does not change what moves). `--part k/m`: the k-th of m runs that
# share the places between them, so a run too long for one turn on a machine's queue is m runs (the final suite,
# 2026-11-22); the scroll case runs with the last.
#
# Then the scroll case (`columns.mjs`, the code-list-columns pass, 2026-11-20): each windowed list wheeled to its end
# and back, failing when a column's left edge or width changes while it scrolls. `--only` picks its lists too (their
# names start `cols-`); `--no-columns` leaves it out.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4" sizes="1440x868@1 2560x1440@1.25" sync="" only="" workers=3 out="" fixture=1 attach="" columns=1 part=1/1
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --fixture-only) real=""; shift ;;
    --no-fixture) fixture=""; shift ;;
    --throttle) throttle=$2; shift 2 ;;
    --sizes) sizes=$2; shift 2 ;;
    --sync) sync=--sync; shift ;;
    --only) only=$2; shift 2 ;;
    --workers) workers=$2; shift 2 ;;
    --attach) attach=$2; workers=1; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --no-columns) columns=""; shift ;;
    --part) part=$2; shift 2 ;;
    *) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
pk=${part%/*} pm=${part#*/}
[[ "$pk" =~ ^[0-9]+$ && "$pm" =~ ^[0-9]+$ && "$pk" -ge 1 && "$pk" -le "$pm" ]] || { echo "moves: --part is k/m with 1 <= k <= m" >&2; exit 2; }
[ "$pk" -eq "$pm" ] || columns=""
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-moves.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5363}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
late="list_entities:250,code_facts:400,entity_checkouts:400,list_sources:150,filter_suggestions:400,mini_board:200,inbox_items:250,get_entity:300,entity_view:350"
corpora=()
[ -z "$fixture" ] || corpora+=("fixture|http://127.0.0.1:$port/?fake-ipc&fake-scale&fake-sources-healthy&fake-hooks&fake-late=$late#/time")
[ -z "$real" ] || corpora+=("real|http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time")
status=0
# The built app on loopback for the length of the run, as `with-built.sh` serves it.
if curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; then echo "moves: something already answers on $port" >&2; exit 1; fi
python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$out/app" >/dev/null 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null' EXIT
for _ in $(seq 1 100); do curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null && break; sleep 0.1; done
for corpus in "${corpora[@]}"; do
  name=${corpus%%|*} url=${corpus#*|}
  for size in $sizes; do
    echo "moves: $name at $size, ${throttle//,/x, }x${sync:+, syncing}"
    pids=()
    for shard in $(seq 1 "$workers"); do
      node "$here/moves.mjs" --app "$url" --out "$out/$name-$size-$shard.json" --size "$size" --throttle "$throttle" \
        --shard "$(( (pk - 1) * workers + shard ))/$(( pm * workers ))" ${sync:+"$sync"} ${only:+--only "$only"} ${attach:+--attach "$attach"} --check --max-minutes 120 \
        > "$out/$name-$size-$shard.log" 2>&1 &
      pids+=($!)
    done
    for pid in "${pids[@]}"; do wait "$pid" || status=1; done
    grep -hE "^(MOVED|ERROR)" "$out/$name-$size-"*.log
    if [ -n "$columns" ]; then
      node "$here/columns.mjs" --app "$url" --out "$out/$name-$size-columns.json" --size "$size" --throttle "$throttle" \
        ${only:+--only "$only"} ${attach:+--attach "$attach"} --check > "$out/$name-$size-columns.log" 2>&1 || status=1
      grep -hE "^(MOVED|ERROR|FAILED)" "$out/$name-$size-columns.log"
    fi
  done
done
echo "moves: results in $out"
[ "$status" -eq 0 ] && echo "moves: nothing moved" || echo "moves: something moved (MOVED lines above)"
exit "$status"
