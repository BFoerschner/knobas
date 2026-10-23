#!/usr/bin/env bash
# `scripts/machine-load.sh`'s own test (#1892), run by `just witness-unit`.
#
# Every reading is injected through KNOBAS_MACHINE_SAMPLE, so the verdicts are
# pinned without depending on how busy this machine is:
# 1. A quiet machine on AC power, every core recommended: exit 0, one line.
# 2. A five-minute load above the cores the kernel recommends: exit 1, and
#    the line says so.
# 3. Cores parked (the recommended-core mask below the core count, as on a
#    MacBook on a low battery, 2026-10-23: 6 of 12 at 4 %): the budget is the
#    recommended count, so a load the full machine would carry is over.
# 4. On battery below 20 %: exit 1 whatever the load.
# 5. The real machine: one line in the same shape, exit 0 or 1.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
here=$(cd "$(dirname "$0")" && pwd)
fails=0
check() { # check <name> <actual> <expected>
    if [ "$2" = "$3" ]; then echo "ok: $1"; else echo "FAIL: $1: got '$2', want '$3'" >&2; fails=$((fails + 1)); fi
}
contains() { # contains <name> <haystack> <needle>
    case $2 in *"$3"*) echo "ok: $1" ;; *) echo "FAIL: $1: '$3' not in '$2'" >&2; fails=$((fails + 1)) ;; esac
}
# run <load1> <load5> <mask> <ncpu> <power> <percent>: the script's output, then its exit code on the last line.
run() {
    local out code
    out=$(KNOBAS_MACHINE_SAMPLE="$*" "$here/machine-load.sh" 2>&1); code=$?
    printf '%s\n%s' "$out" "$code"
}
last() { printf '%s' "${1##*$'\n'}"; }
body() { printf '%s' "${1%$'\n'*}"; }

# 1. Quiet, AC, all twelve cores (mask 4095).
r=$(run 3.2 4.1 4095 12 AC 80)
check "quiet machine on AC is settled" "$(last "$r")" 0
check "settled is one line" "$(printf '%s\n' "$(body "$r")" | wc -l | tr -d ' ')" 1
contains "line names the five-minute load" "$(body "$r")" "load5 4.1"
contains "line names recommended of total cores" "$(body "$r")" "cores 12/12"
contains "line names the power source" "$(body "$r")" "power AC 80%"

# 2. Load above the core count.
r=$(run 30.5 25.0 4095 12 AC 80)
check "load5 over the cores backs off" "$(last "$r")" 1
contains "over says why" "$(body "$r")" "over: load5 25.0 > 12 cores"

# Exactly the core count is not over.
r=$(run 12 12 4095 12 AC 80)
check "load5 equal to the cores is settled" "$(last "$r")" 0

# 3. Cores parked: mask 287 is cpus 0-4 and 8, six of twelve.
r=$(run 9 9 287 12 AC 7)
check "parked cores lower the budget" "$(last "$r")" 1
contains "parked count is shown" "$(body "$r")" "cores 6/12"
contains "parked cores are named as the reason" "$(body "$r")" "6 of 12 cores recommended"

# 4. Battery below 20 %, light load.
r=$(run 2 2 4095 12 battery 19)
check "battery under 20% backs off" "$(last "$r")" 1
contains "battery is named as the reason" "$(body "$r")" "on battery at 19%"
r=$(run 2 2 4095 12 battery 20)
check "battery at 20% is settled" "$(last "$r")" 0

# A malformed sample is an error (2), never a silent "settled".
r=$(run 2 2)
check "malformed sample exits 2" "$(last "$r")" 2

# 5. The real machine.
out=$("$here/machine-load.sh" 2>&1); code=$?
case $code in 0|1) echo "ok: real machine exits 0 or 1" ;; *) echo "FAIL: real machine exited $code: $out" >&2; fails=$((fails + 1)) ;; esac
contains "real machine line has the shape" "$out" "machine: load1 "

if [ "$fails" -ne 0 ]; then echo "machine-load-test: $fails failed" >&2; exit 1; fi
echo "machine-load-test: all passed"
