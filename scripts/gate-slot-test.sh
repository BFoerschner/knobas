#!/usr/bin/env bash
# `scripts/gate-slot.sh`'s own test (#1690), run by `just witness-unit`.
#
# 1. The default wait is 150 s: a taker finding every slot held announces
#    "waiting up to 150 s". The default is one slot (#1892): slot-1 held is
#    every slot held. (900 s outlasts the Bash tool's 600 s foreground
#    ceiling, so a third gate was killed by the tool and read as a red gate.)
# 2. The wait expiring exits 75, not 1, naming every holder's pid, so a
#    timeout is told from a red gate; a short KNOBAS_GATE_SLOT_WAIT is honoured.
# 3. A free slot is taken at once (exit 0), and a slot whose holder is dead
#    is reclaimed.
# 4. Race: with one slot and five takers started together, exactly one holds it
#    and the other four expire with 75.
#
# Everything runs under a private TMPDIR, so the machine's real
# `$TMPDIR/knobas-gate-slots` is never touched.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-gate-slot-check.XXXXXX")
export TMPDIR=$scratch/tmp
mkdir -p "$TMPDIR"
holders=
trap 'for p in $holders; do kill "$p" 2>/dev/null; done; rm -rf "$scratch"' EXIT
fails=0
check() { # check <name> <actual> <expected>
    if [ "$2" = "$3" ]; then echo "ok: $1"; else echo "FAIL: $1: got '$2', want '$3'" >&2; fails=$((fails + 1)); fi
}
contains() { # contains <name> <haystack> <needle>
    case $2 in *"$3"*) echo "ok: $1" ;; *) echo "FAIL: $1: '$3' not in '$2'" >&2; fails=$((fails + 1)) ;; esac
}
# A taker: sources the real script, takes a slot, holds it for $1 seconds.
# A taker that never gives up (a wait that ignores its deadline) is killed after
# 30 s and exits 143, so that fails a check instead of hanging the run (macOS
# has no `timeout`).
taker() {
    bash -c ". '$here/gate-slot.sh'; trap release_slot EXIT; hold_gate_slot t && sleep $1" 2>&1 &
    local t=$! k
    ( sleep 30; kill "$t" 2>/dev/null ) >/dev/null 2>&1 &
    k=$!
    wait "$t"; local code=$?
    kill "$k" 2>/dev/null; wait "$k" 2>/dev/null
    return "$code"
}

# 1 + 2. Two live holders; a third taker.
sleep 60 & a=$!
sleep 60 & b=$!
holders="$a $b"
mkdir -p "$TMPDIR/knobas-gate-slots"
ln -s "$a" "$TMPDIR/knobas-gate-slots/slot-1"
ln -s "$b" "$TMPDIR/knobas-gate-slots/slot-2"

unset KNOBAS_GATE_SLOT_WAIT KNOBAS_GATE_SLOTS
bash -c ". '$here/gate-slot.sh'; hold_gate_slot t" >"$scratch/default.out" 2>&1 &
p=$!
sleep 2
kill "$p" 2>/dev/null
wait "$p" 2>/dev/null
out=$(cat "$scratch/default.out")
contains "default wait is 150 s" "$out" "waiting up to 150 s"
# One slot by default (#1892): with slot-1 held, a taker waits even though
# slot-2 is a link to a live pid it never looks at.
contains "default is one slot" "$out" "all 1 gate slots are held (pids $a)"
# The two-slot checks below ask for two, as `KNOBAS_GATE_SLOTS=2` still can.
export KNOBAS_GATE_SLOTS=2

out=$(KNOBAS_GATE_SLOT_WAIT=2 taker 0); code=$?
check "expired wait exits 75" "$code" 75
contains "an override replaces the default" "$out" "waiting up to 2 s"
giveup=$(printf '%s\n' "$out" | grep 'giving up')
contains "expiry line names holder 1" "$giveup" "$a"
contains "expiry line names holder 2" "$giveup" "$b"
contains "expiry says it is not a red gate" "$out" "not a red gate"

# 3. Free and dead slots.
kill "$a" "$b" 2>/dev/null; wait "$a" "$b" 2>/dev/null
holders=
out=$(KNOBAS_GATE_SLOT_WAIT=0 taker 0); code=$?
check "dead holders' slots are reclaimed" "$code" 0
rm -rf "$TMPDIR/knobas-gate-slots"
out=$(KNOBAS_GATE_SLOT_WAIT=0 taker 0); code=$?
check "free slot is taken at once" "$code" 0

# 4. Race: one slot, five takers.
rm -rf "$TMPDIR/knobas-gate-slots"
for i in 1 2 3 4 5; do
    ( KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=1 taker 4 >/dev/null; echo $? > "$scratch/code-$i" ) &
done
wait
won=0 lost=0
for i in 1 2 3 4 5; do
    case $(cat "$scratch/code-$i") in 0) won=$((won + 1)) ;; 75) lost=$((lost + 1)) ;; esac
done
check "race: exactly one taker wins" "$won" 1
check "race: the others expire with 75" "$lost" 4

[ "$fails" -eq 0 ] || { echo "gate-slot-test: $fails failed" >&2; exit 1; }
echo "gate-slot-test: ok"
