#!/usr/bin/env bash
# The merge-tail lock's own test (#1274, #1836), run by `just witness-unit`.
#
# 1. A holder keeps the lock while it refreshes its claim: a long command
#    under `hold` refreshes it for as long as the command runs, and every
#    lock call by the holder (`assert`, a re-entrant `take`) refreshes it too.
#    A waiter polling through all of that never takes over.
# 2. A holder whose claim is older than the lease is dead: a waiter takes it
#    over and gets the lock, however the holder's PR stands (before #1836 only
#    a MERGED or CLOSED PR counted, so a holder that died with its PR open
#    held the lock until a human noticed). Two waiters find it, exactly one
#    takes it over, and the lowest-numbered gets the lock. A takeover guard
#    left by a killed waiter goes stale and does not block takeovers for good.
#    A lock taken by the pre-#1836 script has no `alive` file: its claim is as
#    old as its `holder` file.
# 3. `take` returns within the wait bound with exit 4 when it is still queued,
#    so a waiter can wait in the foreground and call it again; its queue entry
#    stays. `take` is re-entrant for the holder; `assert` exits 0 for the
#    holder and 1 for anyone else; `hold` runs nothing for a non-holder and
#    passes its command's exit status through.
# 4. `partner <PR> 2` names up to two ready, live waiters, oldest first
#    (#1862); without a count it names one, as the brief before #1862 reads it.
# 5. `gated` (#1862) says "merge without gating again" only to the holder,
#    only while origin's main is the main `ready` recorded, the worktree's head
#    is the head it recorded, and that head sits on that main. Anything else
#    (a commit since, main moved, no record, origin unreadable) is stale.
# 6. `hold` tells its command it runs for the holder (#1866):
#    KNOBAS_MERGE_TAIL_HOLDER=<PR>, which `scripts/gate-slot.sh` reads. End to
#    end with that script, the holder's gate under `hold` takes the freed gate
#    slot ahead of a pre-gate that has waited longer.
#
# Everything runs under a private TMPDIR, so the real lock under the machine's
# $TMPDIR is never touched. The lock no longer asks GitHub anything: a `gh` on
# PATH that records being called proves it.
#
# MERGE_TAIL_LOCK=<path> runs it against another copy of the script, such as
# the one on `main` before #1836, to watch the dead-holder cases fail;
# GATE_SLOT=<path> does the same for `scripts/gate-slot.sh` in case 6.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
here=$(cd "$(dirname "$0")" && pwd)
# Under a holder's re-gate (`hold <PR> just gate` runs `witness-unit`) the
# environment already carries the mark case 6 looks for.
unset KNOBAS_MERGE_TAIL_HOLDER

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-merge-tail-check.XXXXXX")
# A private copy, so the cleanup and the watchdog can kill exactly this test's
# `take`s and never a merge-manager's.
lock_script=$scratch/merge-tail-lock.sh
cp "${MERGE_TAIL_LOCK:-$here/merge-tail-lock.sh}" "$lock_script"
trap 'pkill -f "$lock_script" 2>/dev/null; rm -rf "$scratch"' EXIT
export TMPDIR=$scratch/tmp
mkdir -p "$TMPDIR" "$scratch/bin"
# A broken lock can make a `take` wait forever: after 100 s, kill this test's
# `take`s so the failures surface and the run ends.
(
    while [ -d "$scratch" ] && [ "$SECONDS" -lt 100 ]; do sleep 1; done
    [ -d "$scratch" ] && { echo "FAIL: timed out after 100 s" >&2; pkill -f "$lock_script"; }
) &
export PATH=$scratch/bin:$PATH KNOBAS_MERGE_TAIL_POLL=1 KNOBAS_MERGE_TAIL_LEASE=600 KNOBAS_MERGE_TAIL_WAIT=600

printf '#!/usr/bin/env bash\ntouch "%s/gh-called"; echo "stub: the lock asked gh: $*" >&2; exit 1\n' "$scratch" > "$scratch/bin/gh"
chmod +x "$scratch/bin/gh"

LOCK=$TMPDIR/knobas-merge-tail.lock
QUEUE=$TMPDIR/knobas-merge-tail.queue
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
age() { perl -e 'my $s = shift; utime time - $s, time - $s, @ARGV' "$@"; } # age <seconds> <file>...
fresh() { rm -rf "$TMPDIR"/knobas-merge-tail.*; } # each case starts unlocked, with no queue

failures=0
check() { # check <label> <command...>
    local label=$1; shift
    if "$@"; then echo "ok: $label"; else echo "FAIL: $label" >&2; failures=$((failures + 1)); fi
}
has() { grep -q -- "$2" "$1"; }
lacks() { ! grep -q -- "$2" "$1"; }

# --- 1. A holder that refreshes its claim keeps the lock ---
# Lease 3 s, poll 1 s: `hold` runs a 6 s command, a waiter polls throughout.
export KNOBAS_MERGE_TAIL_LEASE=3
lock take 1270 > /dev/null
( lock hold 1270 sleep 6 > "$scratch/h1.hold" 2>&1; echo "hold exit $?" >> "$scratch/h1.hold" ) &
hold_job=$!
sleep 1
status=$(take_within 4 1268 "$scratch/w1.out")
check "hold: the waiter is still waiting while the holder's command runs (status $status)" [ "$status" = timeout ]
check "hold: the holder still holds 5 s into a 6 s command, lease 3 s" [ "$(assert_status 1270)" = 0 ]
wait "$hold_job"
check "hold: the command's exit status passes through" has "$scratch/h1.hold" "hold exit 0"
# The command is over and nothing refreshes the claim: the waiter gets the lock once the lease runs out.
status=$(take_within 10 1268 "$scratch/w1b.out")
check "lease: a holder that stopped refreshing loses the lock within lease + poll (status $status)" has "$scratch/w1b.out" "held by PR #1268"
check "lease: the takeover names the holder and the lease" has "$scratch/w1b.out" "PR #1270 .*lease"
lock release 1268 > /dev/null
export KNOBAS_MERGE_TAIL_LEASE=600

# Every lock call by the holder is proof of life: `assert` and a re-entrant
# `take` on a claim 700 s old keep a polling waiter out.
fresh
lock take 1271 > /dev/null
age 700 "$LOCK/alive" "$LOCK/holder"
lock assert 1271 > /dev/null
status=$(take_within 3 1272 "$scratch/w1c.out")
check "assert refreshes the claim: the waiter does not take over (status $status)" lacks "$scratch/w1c.out" "taking it over"
age 700 "$LOCK/alive" "$LOCK/holder"
lock take 1271 > "$scratch/r1.out"
check "re-entrant take: already held" has "$scratch/r1.out" "already held by PR #1271"
status=$(take_within 3 1272 "$scratch/w1d.out")
check "re-entrant take refreshes the claim: the waiter does not take over (status $status)" lacks "$scratch/w1d.out" "taking it over"
lock release 1271 > /dev/null

# --- 2. A holder whose claim outlived the lease is taken over ---
fresh
lock take 1260 > /dev/null
age 700 "$LOCK/alive" "$LOCK/holder"
status=$(take_within 10 1261 "$scratch/w2.out")
check "dead holder, PR still open: the waiter takes the lock over (status $status)" has "$scratch/w2.out" "held by PR #1261"
check "dead holder: the takeover is said" has "$scratch/w2.out" "taking it over"
lock release 1261 > /dev/null

# Two waiters find the dead holder; exactly one takes it over, the lowest-numbered gets the lock.
fresh
lock take 1260 > /dev/null
age 700 "$LOCK/alive" "$LOCK/holder"
# 1261 is already in the queue, so 1262 can never be the lowest live waiter,
# however the two start; both race to take the dead holder over.
mkdir -p "$QUEUE"; touch "$QUEUE/1261"
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
check "two waiters: exactly one takes the dead holder over (got $takeovers)" [ "$takeovers" -eq 1 ]
check "two waiters: the lower-numbered gets the lock" has "$scratch/w2a.out" "held by PR #1261"
check "two waiters: the lower-numbered keeps it while the other waits" has "$scratch/w2a.assert" "^0$"
check "two waiters: the other gets it after the release" has "$scratch/w2b.out" "held by PR #1262"

# A takeover guard left by a waiter killed mid-takeover goes stale after 60 s
# and no longer blocks taking over a dead holder.
fresh
lock take 1263 > /dev/null
age 700 "$LOCK/alive" "$LOCK/holder"
mkdir "$LOCK.takeover"; age 120 "$LOCK.takeover"
status=$(take_within 10 1264 "$scratch/w3.out")
check "stale takeover guard: the dead holder is still taken over (status $status)" has "$scratch/w3.out" "held by PR #1264"
lock release 1264 > /dev/null

# A lock the pre-#1836 script took has no `alive` file: its claim is as old as `holder`.
fresh
lock take 1265 > /dev/null
rm "$LOCK/alive"
status=$(take_within 3 1266 "$scratch/w5a.out")
check "legacy holder, fresh: not taken over (status $status)" [ "$status" = timeout ]
age 700 "$LOCK/holder"
status=$(take_within 10 1266 "$scratch/w5b.out")
check "legacy holder, 700 s old: taken over (status $status)" has "$scratch/w5b.out" "held by PR #1266"
lock release 1266 > /dev/null

# --- 3. take's wait bound, re-entry, assert, hold's guards ---
fresh
lock take 1280 > /dev/null
status=$(KNOBAS_MERGE_TAIL_WAIT=2 take_within 10 1281 "$scratch/w6.out")
check "wait bound: a queued take returns with exit 4 (status $status)" [ "$status" = 4 ]
check "wait bound: it says it is still waiting and names the holder" has "$scratch/w6.out" "still waiting.*#1280"
check "wait bound: the queue entry stays for the next call" [ -e "$QUEUE/1281" ]
lock release 1280 > /dev/null
status=$(take_within 10 1281 "$scratch/w6b.out")
check "wait bound: the next call gets the lock (status $status)" has "$scratch/w6b.out" "held by PR #1281"
lock release 1281 > /dev/null

fresh
check "assert: nobody holds the lock, exit 1" [ "$(assert_status 1280)" = 1 ]
lock take 1280 > /dev/null
check "assert: the holder, exit 0" [ "$(assert_status 1280)" = 0 ]
check "assert: anyone else, exit 1" [ "$(assert_status 1281)" = 1 ]
# A `take` that is not re-entrant waits on itself forever; one that is returns
# before its first poll. 10 s is far beyond that, even on a loaded machine.
reentry_status=$(take_within 10 1280 "$scratch/r.out")
check "take: re-entrant for the holder (status $reentry_status)" [ "$reentry_status" = 0 ]
check "take: the re-entry leaves the holder in place" [ "$(assert_status 1280)" = 0 ]
lock hold 1281 touch "$scratch/ran-as-non-holder" > "$scratch/h3.out" 2>&1; hold_status=$?
check "hold: a non-holder's command exits 1 (status $hold_status)" [ "$hold_status" = 1 ]
check "hold: a non-holder's command does not run" [ ! -e "$scratch/ran-as-non-holder" ]
lock hold 1280 bash -c 'exit 7' > /dev/null 2>&1; hold_status=$?
check "hold: the holder's command exit status passes through (status $hold_status)" [ "$hold_status" = 7 ]
check "hold: the holder still holds afterwards" [ "$(assert_status 1280)" = 0 ]
lock release 1280 > /dev/null

# --- 4. partner: up to two ready, live waiters, oldest first ---
# 1302 is ready before 1301, so the order is by number, not by arrival. 1299
# is merged in a batch, 1298's queue entry is dead, 1297 is queued but not
# ready, 1300 is the holder asking.
fresh
lock take 1300 > /dev/null
mkdir -p "$QUEUE"
for n in 1302 1301 1303 1299 1298; do lock ready "$n" "sha$n" > /dev/null; done
lock merged 1299 merge1299 > /dev/null
touch "$QUEUE/1302" "$QUEUE/1301" "$QUEUE/1303" "$QUEUE/1298" "$QUEUE/1297"
age 200 "$QUEUE/1298"
lock partner 1300 2 > "$scratch/p1.out"
check "partner 2: the two oldest ready, live waiters, oldest first" \
    diff <(printf '1301 sha1301\n1302 sha1302\n') "$scratch/p1.out"
lock partner 1300 > "$scratch/p2.out"
# A holder on the brief before #1862 stacks one partner and reads one line.
check "partner without a count: one partner, the oldest" diff <(printf '1301 sha1301\n') "$scratch/p2.out"
rm "$QUEUE/1301" "$QUEUE/1302"
lock partner 1300 2 > "$scratch/p3.out"
check "partner 2 with one candidate left: one line" diff <(printf '1303 sha1303\n') "$scratch/p3.out"
lock partner 1300 3 > "$scratch/p4.out" 2>&1; p4_status=$?
check "partner 3: refused, a batch is at most three (status $p4_status)" [ "$p4_status" = 2 ]
lock release 1300 > /dev/null

# --- 5. The pre-gate record: merge without gating again only when nothing moved ---
# A real origin and two clones: `wt` is the merge-manager's worktree, `other`
# moves main the way another merge does.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
g() { git -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false "$@"; }
wt=$scratch/wt
git init -q --bare "$scratch/origin.git"
git init -q -b main "$wt"
g -C "$wt" commit -q --allow-empty -m base
git -C "$wt" remote add origin "$scratch/origin.git"
git -C "$wt" push -q origin HEAD:refs/heads/main
git -C "$wt" switch -q -c feature
g -C "$wt" commit -q --allow-empty -m feature
main0=$(git -C "$wt" rev-parse HEAD^); head0=$(git -C "$wt" rev-parse HEAD)
git clone -q "$scratch/origin.git" "$scratch/other" 2> /dev/null
# gated_status <PR> <out file>: `gated`'s exit status, run in the worktree as the brief runs it.
gated_status() { (cd "$wt" && lock gated "$1") > "$2" 2>&1; echo $?; }

fresh
lock take 1310 > /dev/null
lock ready 1310 "$head0" "$main0" > /dev/null
status=$(gated_status 1310 "$scratch/g1.out")
check "gated: same head, main unchanged: merge without gating again (status $status)" [ "$status" = 0 ]
check "gated: it says so" has "$scratch/g1.out" "without gating again"
# 1311's record is as good as 1310's: only the lock tells them apart.
lock ready 1311 "$head0" "$main0" > /dev/null
status=$(gated_status 1311 "$scratch/g2.out")
check "gated: a PR that does not hold the lock, exit 1 (status $status)" [ "$status" = 1 ]
check "gated: it says the PR does not hold the lock" has "$scratch/g2.out" "does NOT hold the lock"

g -C "$wt" commit -q --allow-empty -m "fix after the pre-gate"
status=$(gated_status 1310 "$scratch/g3.out")
check "gated: a commit after the pre-gate is stale (status $status)" [ "$status" = 1 ]
check "gated: it names the head" has "$scratch/g3.out" "head"
git -C "$wt" reset -q --hard "$head0"

g -C "$scratch/other" commit -q --allow-empty -m "another merge"
git -C "$scratch/other" push -q origin HEAD:refs/heads/main
main1=$(git -C "$scratch/other" rev-parse HEAD)
# The worktree has not fetched: its own origin/main still reads main0, and
# `gated` must not trust it.
status=$(gated_status 1310 "$scratch/g4.out")
check "gated: main moved since the pre-gate, unfetched: stale (status $status)" [ "$status" = 1 ]
check "gated: it says main moved, from and to" has "$scratch/g4.out" "main moved.*$main0.*$main1"

# A record that claims the new main for a head that does not sit on it.
lock ready 1310 "$head0" "$main1" > /dev/null
git -C "$wt" fetch -q origin
status=$(gated_status 1310 "$scratch/g5.out")
check "gated: the head is not on the recorded main: stale (status $status)" [ "$status" = 1 ]

git -C "$wt" rebase -q origin/main
head1=$(git -C "$wt" rev-parse HEAD)
lock ready 1310 "$head1" "$main1" > /dev/null
status=$(gated_status 1310 "$scratch/g6.out")
check "gated: rebased, gated again, recorded: merge without gating again (status $status)" [ "$status" = 0 ]

# `ready` with no main is a merge-manager on the brief before #1862: no pre-gate.
lock ready 1310 "$head1" > /dev/null
status=$(gated_status 1310 "$scratch/g7.out")
check "gated: ready without a main records no pre-gate (status $status)" [ "$status" = 1 ]
check "gated: it says there is no pre-gate" has "$scratch/g7.out" "no pre-gate"

lock ready 1310 "$head1" "$main1" > /dev/null
git -C "$wt" remote set-url origin "$scratch/nowhere.git"
status=$(gated_status 1310 "$scratch/g8.out")
check "gated: origin unreadable: stale, never fresh (status $status)" [ "$status" = 1 ]
git -C "$wt" remote set-url origin "$scratch/origin.git"

lock release 1310 > /dev/null
lock take 1310 > /dev/null
status=$(gated_status 1310 "$scratch/g9.out")
check "gated: release drops the record (status $status)" [ "$status" = 1 ]
lock release 1310 > /dev/null

# --- 6. The holder's gate goes first on the gate slots (#1866) ---
fresh
lock take 1320 > /dev/null
lock hold 1320 bash -c 'echo "holder=${KNOBAS_MERGE_TAIL_HOLDER:-none}"' > "$scratch/s1.out" 2>&1
check "hold: its command is told it runs for the holder" has "$scratch/s1.out" "holder=1320"

# One gate slot, held by a live pid. The pre-gate polls at about 0, 1, 2, 3 s,
# the holder's gate at about 0.5, 1.5, 2.5 s, and the slot frees at 2.7 s: on
# first come, first served the pre-gate's next poll would take it.
slots_dir=$TMPDIR/knobas-gate-slots
mkdir -p "$slots_dir"
sleep 60 & busy=$!
ln -s "$busy" "$slots_dir/slot-1"
printf '#!/usr/bin/env bash\n. "%s"; trap release_slot EXIT; hold_gate_slot test && sleep 3\n' \
    "${GATE_SLOT:-$here/gate-slot.sh}" > "$scratch/gate.sh"
export KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=20
bash "$scratch/gate.sh" > "$scratch/s2.pre" 2>&1 & pre=$!
sleep 0.5
lock hold 1320 bash "$scratch/gate.sh" > "$scratch/s2.holder" 2>&1 & holder_gate=$!
sleep 2.2
kill "$busy"; wait "$busy" 2>/dev/null
sleep 1.5
check "slots: the holder's gate under hold takes the freed slot" has "$scratch/s2.holder" "gate slot free after"
check "slots: the pre-gate that waited longer is still waiting" lacks "$scratch/s2.pre" "gate slot free after"
wait "$holder_gate" "$pre"
check "slots: the pre-gate takes the slot after the holder's run" has "$scratch/s2.pre" "gate slot free after"
unset KNOBAS_GATE_SLOTS KNOBAS_GATE_SLOT_WAIT
lock release 1320 > /dev/null

check "the lock never asks GitHub" [ ! -e "$scratch/gh-called" ]

if [ "$failures" -ne 0 ]; then echo "merge-tail-lock-test: $failures failed" >&2; exit 1; fi
echo "merge-tail-lock-test: ok"
