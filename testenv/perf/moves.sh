#!/usr/bin/env bash
# The movement gate (the nothing-moves pass of PR #2479, 2026-11-17; Björn: "Check if any other page moves after it's
# drawn. That's not good behavior."): builds the app as the probes measure it, serves it on loopback for the length of
# the run, opens every place a person can open (`moves.mjs`) on its first open and its return, and fails when anything
# drawn moves or changes size by more than 2 px in the 5 s after the place's first useful frame.
#
#   testenv/perf/moves.sh [--real <port>] [--fixture-only] [--throttle 1,4] [--sizes "1440x868@1 2560x1440@1.25"]
#                         [--sync] [--only a,b] [--workers 3] [--attach <debug port>] [--out <dir>]
#
# By default the fixture (`?fake-scale`, each read landing late with `fake-late`, so a late read shows as a move) at 1x
# and 4x at both sizes. `--real <port>` adds the windowless backend on the saved scale database
# (examples/headless_ipc.rs), the same throttles and sizes. `--sync`: every source ends a run that changed something
# every 1.5 s while each place records. `--workers` runs that many browsers at once, each a share of the places (a
# move is a frame order, not a time, so the share does not change what moves).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4" sizes="1440x868@1 2560x1440@1.25" sync="" only="" workers=3 out="" fixture=1 attach=""
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
    *) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-moves.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5363}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
late="list_entities:250,code_facts:400,entity_checkouts:400,list_sources:150,filter_suggestions:400,mini_board:200,inbox_items:250,get_entity:300,entity_view:350"
corpora=()
[ -z "$fixture" ] || corpora+=("fixture|http://127.0.0.1:$port/?fake-ipc&fake-scale&fake-sources-healthy&fake-hooks&fake-late=$late#/time")
[ -z "$real" ] || corpora+=("real|http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time")
status=0
run_all() {
  for corpus in "${corpora[@]}"; do
    name=${corpus%%|*} url=${corpus#*|}
    for size in $sizes; do
      echo "moves: $name at $size, ${throttle//,/x, }x${sync:+, syncing}"
      pids=()
      for shard in $(seq 1 "$workers"); do
        node "$here/moves.mjs" --app "$url" --out "$out/$name-$size-$shard.json" --size "$size" --throttle "$throttle" \
          --shard "$shard/$workers" ${sync:+$sync} ${only:+--only "$only"} ${attach:+--attach "$attach"} --check --max-minutes 120 \
          > "$out/$name-$size-$shard.log" 2>&1 &
        pids+=($!)
      done
      for pid in "${pids[@]}"; do wait "$pid" || status=1; done
      grep -hE "^(MOVED|ERROR)" "$out/$name-$size-"*.log
    done
  done
}
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" bash -c "$(declare -f run_all); $(declare -p corpora sizes throttle workers sync only attach out here); status=0; run_all; exit \$status" || status=1
echo "moves: results in $out"
[ "$status" -eq 0 ] && echo "moves: nothing moved" || echo "moves: something moved (MOVED lines above)"
exit "$status"
