#!/usr/bin/env bash
# The keys budget (testenv/perf/README.md, *A broad key*): builds the app as the probes measure it, serves it on loopback
# for the length of the run, presses the keys that keep most of a page (`keys.mjs`) in a headless Chrome at each
# throttle, and fails when a key's rows are drawn later than its line in `budget-keys.json`, or anything drawn moved.
#
#   testenv/perf/keys-budget.sh --real <port> [--throttle 1,4,6] [--only a,b] [--repeat 7] [--sizes "WxH@s ..." | none]
#                               [--out <dir>]
#
# --real <port>  the windowless backend on the saved scale database (examples/headless_ipc.rs): the keys are its
#                projects and priorities, so there is no fixture run
# --sizes        the keys pressed again at these sizes, at 4x and 6x, against the same lines (the sizes pass,
#                2026-11-15): by default a 4K screen at 200 % (1920x1080@2, the most device pixels) and a 1440p screen
#                at 125 % (2560x1440@1.25, the most CSS pixels at a fractional scale); `none` runs 1440x868@1 alone
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="1,4,6" only="" out="" repeat=7 sizes="1920x1080@2 2560x1440@1.25"
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --repeat) repeat=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --sizes) sizes=$2; shift 2 ;;
    *) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
[ -n "$real" ] || { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-keys-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5363}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/keys.mjs" \
  --app "http://127.0.0.1:$port/?fake-ipc&fake-real=$real#/tickets" --out "$out/keys.json" \
  --throttle "$throttle" --repeat "$repeat" ${only:+--only "$only"} --check "$here/budget-keys.json" --lines real
status=$?
# The full-screen sizes at the throttles they are held at, 4x and 6x, of those asked for.
size_throttle=$(echo "$throttle" | tr ',' '\n' | grep -xE '4|6' | paste -sd, -)
[ "$sizes" != none ] && [ -n "$size_throttle" ] && for size in $sizes; do
  echo "keys-budget: $size at ${size_throttle//,/x, }x"
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/keys.mjs" --size "$size" \
    --app "http://127.0.0.1:$port/?fake-ipc&fake-real=$real#/tickets" --out "$out/keys-$size.json" \
    --throttle "$size_throttle" --repeat "$repeat" ${only:+--only "$only"} --check "$here/budget-keys.json" --lines real || status=1
done
echo "keys-budget: results in $out"
exit $status
