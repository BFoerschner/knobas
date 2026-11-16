#!/usr/bin/env bash
# The open budget (testenv/perf/README.md, *Opening a page*): builds the app as the probes measure it, serves it on
# loopback for the length of the run, presses every place's nav entry or row in a headless Chrome at each throttle, on
# its first visit, its return, its return after every other section, and its return after a sync, and fails when an
# open's first useful frame or its settling is over its line in `budget-open.json`, or a row moved after it drew.
#
#   testenv/perf/open-budget.sh --real <port> [--inner [--no-big]] [--forge <ms>] [--syncing <source>] [--throttle 1,4,6]
#                               [--only a,b] [--sizes "WxH@s ..." | none] [--out <dir>]
#   testenv/perf/open-budget.sh --real <port> --moves [--keep-compares]
#
# --real <port>  the windowless backend on the saved scale database (examples/headless_ipc.rs); the places are its
#                pages (the SCLENG space, a TeamCity pipeline), so there is no fixture run
# --inner        the inner pages instead (`open.mjs --set inner`): a repository's landing, tabs and files, a commit,
#                Compare, a pull request's tabs and a run's tabs, against a live forge, checked against `inner` lines
# --no-big       with --inner: no big folders or big compares, which are made on the live Gitea for the run: for a
#                stand-in forge (`standin-forge.mjs --repos`, README, *The inner pages on the Mac*)
# --forge <ms>   with --inner: hold each request to Gitea <ms> through `slow-forge.mjs` for the length of the run,
#                checked against `forge` lines; the profile's Gitea source must name https://127.0.0.1:3900
#                (README, *The inner pages*)
# --syncing <source>  with --inner: keep a run of that source going for the length of the run (`syncing.mjs`),
#                so its sync fills the forge's quota while the pages are pressed (big folders left out), checked against
#                `syncing` lines (the priority lane, 2026-11-13; README, *With a sync running*)
# --extremes     the extreme pages instead (`open.mjs --set extremes`, the extremes pass, 2026-11-13; README, *The
#                extremes*): seeded on the live products for the run (`testenv/seed-extremes.sh`), synced through the
#                profile's sources and removed after it, checked against `extremes` lines; `--keep-extremes` leaves them
# --moves        moving through a big compare instead (`compare-moves.mjs`, the big-compare pass, 2026-11-16): a jump to
#                a file, Back to top, branch swaps, Open as diff and a jump in Files changed, on big-compare.mjs's
#                repositories made on the live Gitea for the run (a branch of 4,200 changed files and 300 commits, and its
#                pull request) and removed after it, at 1440x868@1 and the two full-screen sizes, against `moves` lines;
#                `--keep-compares` leaves them for the next run
# --sizes        the nav pages pressed again at these sizes, at 4x and 6x, against the same lines (the sizes pass,
#                2026-11-15): by default a 4K screen at 200 % (1920x1080@2, the most device pixels) and a 1440p screen
#                at 125 % (2560x1440@1.25, the most CSS pixels at a fractional scale); `none` runs 1440x868@1 alone.
#                The inner pages and the extremes run at 1440x868@1 unless --sizes names sizes for them
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
real="" throttle="" asked="" only="" out="" inner="" forge="" syncing="" extremes="" keep="" nobig="" sizes="" moves="" keepcmp=""
while [ $# -gt 0 ]; do
  case $1 in
    --real) real=$2; shift 2 ;;
    --throttle) throttle=$2; asked=1; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --sizes) sizes=$2; shift 2 ;;
    --inner) inner=1; shift ;;
    --no-big) nobig=1; shift ;;
    --forge) forge=$2; shift 2 ;;
    --syncing) syncing=$2; shift 2 ;;
    --extremes) extremes=1; shift ;;
    --keep-extremes) keep=1; shift ;;
    --moves) moves=1; shift ;;
    --keep-compares) keepcmp=1; shift ;;
    *) sed -n '2,34p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
  esac
done
# The nav pages and the inner pages at 1x, 4x and 6x (Björn's three lines, 2026-11-14; the inner pages' 6x items since
# the inner-6x pass, 2026-11-15); the extremes at 1x and 4x, the throttles their items name.
[ -n "$throttle" ] || { if [ -n "$extremes" ]; then throttle="1,4"; else throttle="1,4,6"; fi; }
[ -n "$real" ] || { sed -n '2,34p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[ -z "$extremes" ] || [ -z "$inner" ] || { echo "open-budget: --extremes and --inner are two runs" >&2; exit 2; }
[ -z "$forge" ] || [ -n "$inner" ] || { echo "open-budget: --forge measures the inner pages: add --inner" >&2; exit 2; }
[ -z "$nobig" ] || [ -n "$inner" ] || { echo "open-budget: --no-big leaves out the inner pages' big folders: add --inner" >&2; exit 2; }
[ -z "$syncing" ] || [ -n "$inner" ] || { echo "open-budget: --syncing measures the inner pages: add --inner" >&2; exit 2; }
[ -z "$syncing" ] || [ -z "$forge" ] || { echo "open-budget: --syncing and --forge are two runs" >&2; exit 2; }
out=${out:-$(mktemp -d "${TMPDIR:-/tmp}/knobas-open-budget.XXXXXX")}
mkdir -p "$out"
port=${KNOBAS_PERF_PORT:-5362}
KNOBAS_PERF=1 "$root/testenv/ui-checks/build-app.sh" "$root" "$out/app" || exit 1
url="http://127.0.0.1:$port/?fake-ipc&fake-hooks&fake-real=$real#/time"
if [ -n "$moves" ]; then
  # The big compare's moves (the big-compare pass): made on the live Gitea, measured at three sizes, removed.
  dir="$out/big-compare"
  node "$here/big-compare.mjs" up --port "$real" --dir "$dir" || exit 1
  [ -n "$keepcmp" ] || trap 'node "$here/big-compare.mjs" down --dir "$dir"' EXIT
  repo="live-gitea:knobas-perf-big-compare/cloned"
  # Its two pull requests, made in this order on a repository made for the run: `many-commits`, then the 4x twin's.
  pr_a="$repo#1" pr_b="$repo#2"
  status=0
  for size in 1440x868@1 1920x1080@2 2560x1440@1.25; do
    rates=1,4,6
    [ "$size" = 1440x868@1 ] || rates=4,6
    echo "open-budget: the big compare's moves at $size, ${rates//,/x, }x"
    "$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/compare-moves.mjs" --app "$url" --repo "$repo" \
      --pr "$pr_a" --pr-b "$pr_b" --throttle "$([ -n "$asked" ] && echo "$throttle" || echo "$rates")" --size "$size" --out "$out/moves-$size.json" --budget || status=1
  done
  echo "open-budget: results in $out"
  exit $status
fi
set_args=() lines=real
# open.mjs gives up after 45 minutes unless told otherwise; the nav pages at 1x, 4x and 6x outran that (2026-11-14), so they
# get 45 a throttle.
[ -n "$inner$extremes$forge" ] || set_args=(--max-minutes $((45 * $(echo "$throttle" | tr ',' '\n' | grep -c .))))
if [ -n "$inner" ]; then
  set_args=(--set inner --rounds "first,return,synced")
  [ -z "$nobig" ] || set_args+=(--no-big-folders --no-big-compares)
  lines=inner
fi
if [ -n "$extremes" ]; then
  set_args=(--set extremes --rounds "first,return")
  [ -z "$keep" ] || set_args+=(--keep-extremes)
  lines=extremes
fi
proxy="" load=""
trap 'kill $proxy $load 2>/dev/null' EXIT
if [ -n "$syncing" ]; then
  lines=syncing
  # The big folders wait for a run of their own to mirror two new repositories, which a run kept going cannot be.
  set_args+=(--no-big-folders)
  node "$here/syncing.mjs" --port "$real" --source "$syncing" > "$out/syncing.log" 2>&1 &
  load=$!
  # Measured once the run is past its set-up and into its reads.
  for _ in $(seq 1 60); do
    grep -qx syncing "$out/syncing.log" && break
    kill -0 "$load" 2>/dev/null || { cat "$out/syncing.log" >&2; exit 1; }
    sleep 1
  done
  grep -qx syncing "$out/syncing.log" || { echo "open-budget: $syncing never started syncing" >&2; exit 1; }
fi
if [ -n "$forge" ]; then
  lines=forge
  # Every first read waits the forge's delay a request: the whole run takes past open.mjs's 45 minutes (the WSL block,
  # 2026-11-14: its watchdog stopped a 1,000 ms run at the 4x big compares).
  set_args+=(--max-minutes 120)
  node "$here/slow-forge.mjs" --listen 3900 --upstream 127.0.0.1:3443 --delay "$forge" > "$out/slow-forge.log" 2>&1 &
  proxy=$!
  sleep 1
fi
"$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/open.mjs" --app "$url" --out "$out/open.json" \
  --throttle "$throttle" ${only:+--only "$only"} "${set_args[@]}" --check "$here/budget-open.json" --lines "$lines"
status=$?
# The full-screen sizes (the nav pages' by default) at the throttles they are held at, 4x and 6x, of those asked for:
# the same presses, rounds and lines.
[ -n "$sizes" ] || { [ -n "$inner$extremes" ] && sizes=none || sizes="1920x1080@2 2560x1440@1.25"; }
size_throttle=$(echo "$throttle" | tr ',' '\n' | grep -xE '4|6' | paste -sd, -)
[ "$sizes" != none ] && [ -n "$size_throttle" ] && for size in $sizes; do
  echo "open-budget: $size at ${size_throttle//,/x, }x"
  size_args=("${set_args[@]}")
  [ -n "$inner$extremes$forge" ] || size_args=(--max-minutes $((45 * $(echo "$size_throttle" | tr ',' '\n' | grep -c .))))
  "$root/testenv/ui-checks/with-built.sh" "$out/app" "$port" node "$here/open.mjs" --app "$url" --out "$out/open-$size.json" --size "$size" \
    --throttle "$size_throttle" ${only:+--only "$only"} "${size_args[@]}" --check "$here/budget-open.json" --lines "$lines" || status=1
done
echo "open-budget: results in $out"
exit $status
