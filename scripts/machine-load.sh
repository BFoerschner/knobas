#!/usr/bin/env bash
# MACHINE LOAD -- one line saying whether the machine can take more work, for
# the loop's back-off (`.claude/commands/ticket-loop.md`, *Ceiling*; #1892).
#
#     machine: load1 9.7 load5 21.5 cores 12/12 power AC 13% -- over: load5 21.5 > 12 cores
#
# Exit 0 when settled, 1 when over, 2 when a reading cannot be had. Over is any of:
#
# - The five-minute load average above the cores the kernel recommends. The
#   load average counts runnable threads; above the core count, threads wait
#   for a core, and the tests with a wall-clock budget (vitest's 15 s, the
#   Rust suites' scheduler and git deadlines) start to time out on code that
#   is fine. On 2026-10-23 a gate's test run alone held the load at 29 on
#   average (78 samples, against 9.7 with no test run in progress).
# - Fewer cores recommended than the machine has. macOS parks performance
#   cores when it caps power -- on 2026-10-23 a MacBook on battery at 4-7 %
#   recommended 6 of its 12 cores (`kern.sched_recommended_cores` 287, cpus
#   0-4 and 8) and a single-threaded loop got 45 % of a core while `top`
#   reported 45 % idle. The load budget is the recommended count, not
#   `hw.ncpu`, and a parked core is a reason to back off in itself: the
#   gates' timings assume twelve.
# - On battery below 20 %, where the parking starts. The first battery
#   period that day ran from 80 % to 4 % in under two hours of loop.
#
# The five-minute average rather than the one-minute: the loop decides seats
# every iteration, and a one-minute figure swings 20 points with one test
# run's start or end.
#
# KNOBAS_MACHINE_SAMPLE="<load1> <load5> <recommended mask> <ncpu> <AC|battery> <percent>"
# replaces the readings, for `scripts/machine-load-test.sh`.
#
# Portability: bash 3.2, sysctl and pmset (macOS). Off macOS (no
# `kern.sched_recommended_cores`), every core counts as recommended and the
# power is taken as AC.
set -uo pipefail

if [ -n "${KNOBAS_MACHINE_SAMPLE:-}" ]; then
    read -r load1 load5 mask ncpu power pct <<<"$KNOBAS_MACHINE_SAMPLE"
else
    read -r load1 load5 _ <<<"$(sysctl -n vm.loadavg 2>/dev/null | tr -d '{}')"
    ncpu=$(sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN)
    mask=$(sysctl -n kern.sched_recommended_cores 2>/dev/null || echo $(( (1 << ncpu) - 1 )))
    batt=$(pmset -g batt 2>/dev/null || true)
    case $batt in *"Battery Power"*) power=battery ;; *) power=AC ;; esac
    pct=$(printf '%s\n' "$batt" | sed -n 's/.*[[:space:]]\([0-9][0-9]*\)%;.*/\1/p' | head -1)
    pct=${pct:-100}
fi
num='^[0-9]+([.][0-9]+)?$'
for v in "${load1:-}" "${load5:-}"; do
    [[ $v =~ $num ]] || { echo "machine-load: cannot read the load average (got '$v')" >&2; exit 2; }
done
for v in "${mask:-}" "${ncpu:-}" "${pct:-}"; do
    [[ $v =~ ^[0-9]+$ ]] || { echo "machine-load: cannot read the cores or the battery (got '$v')" >&2; exit 2; }
done

cores=0 m=$mask
while [ "$m" -gt 0 ]; do cores=$((cores + (m & 1))); m=$((m >> 1)); done
[ "$cores" -le "$ncpu" ] || cores=$ncpu

why=
if awk -v l="$load5" -v c="$cores" 'BEGIN { exit !(l > c) }'; then
    why="load5 $load5 > $cores cores"
fi
if [ "$cores" -lt "$ncpu" ]; then
    why="${why:+$why; }$cores of $ncpu cores recommended (power capped)"
fi
if [ "$power" = battery ] && [ "$pct" -lt 20 ]; then
    why="${why:+$why; }on battery at $pct%"
fi

line="machine: load1 $load1 load5 $load5 cores $cores/$ncpu power $power $pct%"
if [ -n "$why" ]; then
    echo "$line -- over: $why"
    exit 1
fi
echo "$line -- settled"
