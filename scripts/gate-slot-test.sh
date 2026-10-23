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
# 5. The merge-tail holder goes first (#1866): a gate run under
#    `merge-tail-lock.sh hold` (KNOBAS_MERGE_TAIL_HOLDER set) takes the next
#    free slot ahead of a pre-gate that has waited longer, and the pre-gate
#    says why it waits and takes the slot once the holder's run is done. A
#    holder that holds its slot, gave up (exit 75) or was SIGKILLed while
#    waiting holds nobody back.
#
# Everything runs under a private TMPDIR, so the machine's real
# `$TMPDIR/knobas-gate-slots` is never touched.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
# Under a holder's re-gate (`hold <PR> just gate` runs `witness-unit`) the
# environment carries the holder's mark; every taker here sets its own.
unset KNOBAS_MERGE_TAIL_HOLDER
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

# 5. The merge-tail holder's gate goes first. One slot, held by a live pid;
# the pre-gate starts waiting first, the holder's gate second. The slot frees
# while the holder's gate is stopped (SIGSTOP: alive, but it cannot poll), so
# on first come, first served the pre-gate's next poll would take it. Every
# taker gives up after 20 s and holds what it took for 3 s. Each step waits
# for what it needs to see (`await`), not for a fixed time, so a loaded
# machine (`witness-unit` runs inside a gate) slows it down without breaking it.
slots_dir=$TMPDIR/knobas-gate-slots
await() { # await <seconds> <command...>: polls every 0.1 s until it succeeds
    local n=$(($1 * 10)); shift
    until "$@"; do n=$((n - 1)); [ "$n" -gt 0 ] || return 1; sleep 0.1; done
}
says() { grep -q -- "$2" "$1" 2>/dev/null; } # says <file> <text>
slot_names() { [ "$(readlink "$slots_dir/slot-1" 2>/dev/null)" = "$1" ]; }
start_taker() { # start_taker <out file> <hold seconds> [VAR=value...]: sets `started` to its pid
    local out=$1 hold=$2; shift 2
    env KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=20 "$@" \
        bash -c ". '$here/gate-slot.sh'; trap release_slot EXIT; hold_gate_slot t && sleep $hold" > "$out" 2>&1 &
    started=$!
}
rm -rf "$slots_dir"; mkdir -p "$slots_dir"
sleep 60 & a=$!; holders=$a
ln -s "$a" "$slots_dir/slot-1"
start_taker "$scratch/pre.out" 3; pre=$started
await 10 says "$scratch/pre.out" "waiting up to"
start_taker "$scratch/holder.out" 3 KNOBAS_MERGE_TAIL_HOLDER=1900; hold=$started
holders="$a $pre $hold"
await 10 says "$scratch/holder.out" "waiting up to"
kill -STOP "$hold"
kill "$a"; wait "$a" 2>/dev/null
# Long enough for the pre-gate's once-a-second poll to come round twice;
# the scripts before #1866 take the slot in it.
sleep 2.5
# The freed link still names the dead pid: a taker that yields reclaims nothing.
check "a pre-gate leaves the freed slot to a waiting holder's gate" "$(readlink "$slots_dir/slot-1")" "$a"
contains "the pre-gate says a merge-tail holder's gate goes first" "$(cat "$scratch/pre.out")" "merge-tail holder"
kill -CONT "$hold"
await 10 slot_names "$hold"
check "the merge-tail holder's gate takes the freed slot ahead of a pre-gate that waited longer" \
    "$(readlink "$slots_dir/slot-1")" "$hold"
wait "$hold"; code=$?
check "the holder's gate ran" "$code" 0
wait "$pre"; code=$?
check "the pre-gate takes the slot once the holder's run is done" "$code" 0
holders=

# A holder's gate that holds a slot holds nobody back: with two slots, a
# pre-gate takes the second at once while the holder's run goes on.
rm -rf "$slots_dir"
start_taker "$scratch/holding.out" 3 KNOBAS_MERGE_TAIL_HOLDER=1900 KNOBAS_GATE_SLOTS=2; hold=$started
holders=$hold
sleep 0.5
out=$(KNOBAS_GATE_SLOTS=2 KNOBAS_GATE_SLOT_WAIT=0 taker 0); code=$?
check "a holder's gate that holds a slot leaves the free one to a pre-gate" "$code" 0
wait "$hold"; holders=

# A holder whose wait expired (75) holds nobody back afterwards.
sleep 60 & a=$!; holders=$a
rm -rf "$slots_dir"; mkdir -p "$slots_dir"; ln -s "$a" "$slots_dir/slot-1"
out=$(KNOBAS_MERGE_TAIL_HOLDER=1900 KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=1 taker 0); code=$?
check "a holder's gate still gives up with 75 when no slot frees" "$code" 75
kill "$a"; wait "$a" 2>/dev/null; holders=
out=$(KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=0 taker 0); code=$?
check "a holder that gave up holds no pre-gate back" "$code" 0

# A holder SIGKILLed while it waits (no trap ran) holds nobody back either.
sleep 60 & a=$!; holders=$a
rm -rf "$slots_dir"; mkdir -p "$slots_dir"; ln -s "$a" "$slots_dir/slot-1"
start_taker "$scratch/killed.out" 0 KNOBAS_MERGE_TAIL_HOLDER=1900; hold=$started
holders="$a $hold"
sleep 1
kill -KILL "$hold"; wait "$hold" 2>/dev/null
kill "$a"; wait "$a" 2>/dev/null; holders=
out=$(KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=0 taker 0); code=$?
check "a holder that was SIGKILLed while waiting holds no pre-gate back" "$code" 0

[ "$fails" -eq 0 ] || { echo "gate-slot-test: $fails failed" >&2; exit 1; }
echo "gate-slot-test: ok"
