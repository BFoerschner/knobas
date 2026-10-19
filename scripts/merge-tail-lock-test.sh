#!/usr/bin/env bash
# The merge-tail lock's own test (#1274), run by `just witness-unit`.
#
# 1. A live holder's PR turns MERGED the moment its own `gh pr merge` lands,
#    seconds before its `release`. A waiter polling in that window must keep
#    waiting: holder takes at t+0, merges at t+3, releases at t+9, while a
#    waiter polls every second from t+1. Before #1274 the waiter took the lock
#    over at about t+3 and the holder's release said "does not hold the lock".
# 2. A holder merged longer ago than the grace period is dead: two waiters
#    find it, exactly one takes it over, and the lowest-numbered gets the lock
#    and keeps it while the other waits. A takeover guard left behind by a
#    killed waiter goes stale and does not block takeovers for good. A waiter
#    whose read of the holder went stale while another took over leaves the
#    new holder's lock alone.
# 3. `take` is re-entrant for the holder; `assert` exits 0 for the holder and
#    1 for anyone else.
#
# Everything runs under a private TMPDIR with a stubbed `gh` on PATH, so the
# real lock under the machine's $TMPDIR is never touched.
#
# MERGE_TAIL_LOCK=<path> runs it against another copy of the script, such as
# the one on `main` before #1274, to watch the race cases fail.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
here=$(cd "$(dirname "$0")" && pwd)

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-merge-tail-check.XXXXXX")
# A private copy, so the cleanup and the watchdog can kill exactly this test's
# `take`s and never a merge-manager's.
lock_script=$scratch/merge-tail-lock.sh
cp "${MERGE_TAIL_LOCK:-$here/merge-tail-lock.sh}" "$lock_script"
trap 'pkill -f "$lock_script" 2>/dev/null; rm -rf "$scratch"' EXIT
export TMPDIR=$scratch/tmp GH_STUB=$scratch/gh
mkdir -p "$TMPDIR" "$GH_STUB" "$scratch/bin"
# A broken lock can make a `take` wait forever: after 90 s, kill this test's
# `take`s so the failures surface and the run ends.
(
    while [ -d "$scratch" ] && [ "$SECONDS" -lt 90 ]; do sleep 1; done
    [ -d "$scratch" ] && { echo "FAIL: timed out after 90 s" >&2; pkill -f "$lock_script"; }
) &
export PATH=$scratch/bin:$PATH KNOBAS_MERGE_TAIL_POLL=1 KNOBAS_MERGE_TAIL_GRACE=600

# The stub answers `gh pr view <PR> --json state[,closedAt] --jq ...` from
# $GH_STUB/<PR>, which holds "<STATE> [<closedAt>]". With $GH_STUB/<PR>.slow,
# the first answer about that PR takes 2 s.
cat > "$scratch/bin/gh" <<'EOF'
#!/usr/bin/env bash
pr="$3"; f="$GH_STUB/$pr"; [ -e "$f" ] || { echo "stub: no such PR $pr" >&2; exit 1; }
if [ -e "$f.slow" ] && mkdir "$f.slowed" 2>/dev/null; then sleep 2; fi
read -r state closed < "$f"
case " $* " in *closedAt*) echo "$state ${closed:-}" ;; *) echo "$state" ;; esac
EOF
chmod +x "$scratch/bin/gh"

iso() { # iso <epoch seconds> -> GitHub's closedAt shape
    date -u -r "$1" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$1" +%Y-%m-%dT%H:%M:%SZ
}
lock() { bash "$lock_script" "$@"; }
assert_status() { lock assert "$1" > /dev/null && echo 0 || echo 1; }
# take_within <seconds> <PR> <out file>: prints take's exit status, or
# "timeout" when it is still waiting after <seconds> (and is then killed).
take_within() {
    local job i
    bash "$lock_script" take "$2" > "$3" & # the script itself, so `kill` reaches it
    job=$!
    for ((i = 0; i < $1 * 2; i++)); do kill -0 "$job" 2>/dev/null || break; sleep 0.5; done
    if kill -0 "$job" 2>/dev/null; then kill "$job"; wait "$job" 2>/dev/null; echo timeout
    else wait "$job"; echo $?; fi
}
fresh() { rm -rf "$TMPDIR"/knobas-merge-tail.*; } # each case starts unlocked, with no queue

failures=0
check() { # check <label> <command...>
    local label=$1; shift
    if "$@"; then echo "ok: $label"; else echo "FAIL: $label" >&2; failures=$((failures + 1)); fi
}
has() { grep -q -- "$2" "$1"; }
lacks() { ! grep -q -- "$2" "$1"; }

# --- 1. A holder that just merged keeps the lock until it releases ---
echo OPEN > "$GH_STUB/1270"; echo OPEN > "$GH_STUB/1268"
lock take 1270 > "$scratch/h1.out"
(
    sleep 3
    echo "MERGED $(iso "$(date +%s)")" > "$GH_STUB/1270"
    sleep 5
    assert_status 1270 > "$scratch/h1.assert"
    sleep 1
    lock release 1270 > "$scratch/h1.release"
) &
holder_job=$!
sleep 1
lock take 1268 > "$scratch/w1.out"
lock release 1268 > /dev/null
wait "$holder_job"
check "race: the waiter never takes over a holder that just merged" lacks "$scratch/w1.out" "taking it over"
check "race: the holder still holds at t+8, after its merge" has "$scratch/h1.assert" "^0$"
check "race: the holder's release finds it still the holder" has "$scratch/h1.release" "released by PR #1270"
check "race: the waiter gets the lock after the release" has "$scratch/w1.out" "held by PR #1268"

# --- 2. A holder dead past the grace period is taken over by one waiter ---
fresh
echo OPEN > "$GH_STUB/1260"; echo OPEN > "$GH_STUB/1261"; echo OPEN > "$GH_STUB/1262"
lock take 1260 > /dev/null
echo "MERGED $(iso $(( $(date +%s) - 700 )))" > "$GH_STUB/1260"
# 1261 is already in the queue, so 1262 can never be the lowest live waiter,
# however the two start; both race to take the dead holder over.
mkdir -p "$TMPDIR/knobas-merge-tail.queue"; touch "$TMPDIR/knobas-merge-tail.queue/1261"
(
    lock take 1261 > "$scratch/w2a.out"
    sleep 3
    assert_status 1261 > "$scratch/w2a.assert"
    lock release 1261 > /dev/null
) &
a_job=$!
(
    lock take 1262 > "$scratch/w2b.out"
    lock release 1262 > /dev/null
) &
b_job=$!
wait "$a_job" "$b_job"
takeovers=$(cat "$scratch/w2a.out" "$scratch/w2b.out" | grep -c "taking it over")
check "dead holder: exactly one of two waiters takes it over (got $takeovers)" [ "$takeovers" -eq 1 ]
check "dead holder: the lower-numbered waiter gets the lock" has "$scratch/w2a.out" "held by PR #1261"
check "dead holder: the lower-numbered waiter keeps it while the other waits" has "$scratch/w2a.assert" "^0$"
check "dead holder: the other waiter gets it after the release" has "$scratch/w2b.out" "held by PR #1262"

# A waiter that read the dead holder before another waiter took over and
# acquired must not remove the new holder's lock. 1267 asks first and its
# answer takes 2 s; 1266 (already queued) takes over and holds meanwhile.
fresh
echo OPEN > "$GH_STUB/1266"; echo OPEN > "$GH_STUB/1267"; echo OPEN > "$GH_STUB/1265"
lock take 1265 > /dev/null
echo "MERGED $(iso $(( $(date +%s) - 700 )))" > "$GH_STUB/1265"; touch "$GH_STUB/1265.slow"
mkdir -p "$TMPDIR/knobas-merge-tail.queue"; touch "$TMPDIR/knobas-merge-tail.queue/1266"
( lock take 1267 > "$scratch/w4b.out"; lock release 1267 > /dev/null ) &
late_job=$!
sleep 0.5
lock take 1266 > "$scratch/w4a.out"
sleep 3 # 1267's answer arrives at about t+2
check "stale read: the late waiter leaves the new holder's lock alone" [ "$(assert_status 1266)" = 0 ]
lock release 1266 > /dev/null
wait "$late_job"
check "stale read: the late waiter took nothing over" lacks "$scratch/w4b.out" "taking it over"

# A takeover guard left by a waiter killed mid-takeover goes stale after 60 s
# and no longer blocks taking over a dead holder.
fresh
echo OPEN > "$GH_STUB/1263"; echo OPEN > "$GH_STUB/1264"
lock take 1263 > /dev/null
echo "CLOSED $(iso $(( $(date +%s) - 700 )))" > "$GH_STUB/1263"
mkdir "$TMPDIR/knobas-merge-tail.lock.takeover"
perl -e 'utime time - 120, time - 120, shift' "$TMPDIR/knobas-merge-tail.lock.takeover"
stale_status=$(take_within 10 1264 "$scratch/w3.out")
check "stale takeover guard: the dead holder is still taken over (status $stale_status)" has "$scratch/w3.out" "held by PR #1264"
lock release 1264 > /dev/null

# --- 3. Re-entrant take and assert ---
fresh
echo OPEN > "$GH_STUB/1280"; echo OPEN > "$GH_STUB/1281"
check "assert: nobody holds the lock, exit 1" [ "$(assert_status 1280)" = 1 ]
lock take 1280 > /dev/null
check "assert: the holder, exit 0" [ "$(assert_status 1280)" = 0 ]
check "assert: anyone else, exit 1" [ "$(assert_status 1281)" = 1 ]
# A `take` that is not re-entrant waits on itself forever; one that is returns
# before its first poll. 10 s is far beyond that, even on a loaded machine.
reentry_status=$(take_within 10 1280 "$scratch/r.out")
check "take: re-entrant for the holder (status $reentry_status)" [ "$reentry_status" = 0 ]
check "take: the re-entry leaves the holder in place" [ "$(assert_status 1280)" = 0 ]
lock release 1280 > /dev/null

if [ "$failures" -ne 0 ]; then echo "merge-tail-lock-test: $failures failed" >&2; exit 1; fi
echo "merge-tail-lock-test: ok"
