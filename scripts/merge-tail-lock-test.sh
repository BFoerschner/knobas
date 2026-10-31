#!/usr/bin/env bash
# The merge-tail lock's own test (#1274, #1836, #2335), run by `just witness-unit`.
#
# The holder slot lives on origin as a ref (#2335). Here origin is a local bare
# repo and each "machine" is its own clone with its own TMPDIR (its own local
# queue): machine A is the default, `lock_b` runs as machine B.
#
# 0. Two machines (#2335): two `take`s racing from two clones on a free slot,
#    and on a dead holder's slot: exactly one gets the lock. A holder whose
#    lease ran out is taken over from the other machine, and the old holder's
#    `assert` then exits 1 and its `release` leaves the new holder alone. An
#    unreadable origin never holds: `take` stays queued, `assert` exits 1.
# 1. A holder keeps the lock while it refreshes its claim: a long command
#    under `hold` refreshes it for as long as the command runs, and every
#    lock call by the holder (`assert`, a re-entrant `take`) refreshes it too.
#    A waiter polling through all of that never takes over.
# 2. A holder whose claim is older than the lease is dead: a waiter takes it
#    over and gets the lock, however the holder's PR stands (before #1836 only
#    a MERGED or CLOSED PR counted). Two waiters on one machine find it,
#    exactly one takes it over, and the lowest-numbered gets the lock.
# 3. `take` returns within the wait bound with exit 4 when it is still queued,
#    so a waiter can wait in the foreground and call it again; its queue entry
#    stays. `take` is re-entrant for the holder; `assert` exits 0 for the
#    holder and 1 for anyone else; `hold` runs nothing for a non-holder and
#    passes its command's exit status through.
# 4. `gated` (#1862) says "merge without gating again" only to the holder,
#    only while origin's main is the main `ready` recorded, the worktree's head
#    is the head it recorded, and that head sits on that main. Anything else
#    (a commit since, main moved, no record, origin unreadable) is stale.
# 5. `hold` tells its command it runs for the holder (#1866):
#    KNOBAS_MERGE_TAIL_HOLDER=<PR>, which `scripts/gate-slot.sh` reads. End to
#    end with that script, the holder's gate under `hold` takes the freed gate
#    slot ahead of a pre-gate that has waited longer.
#
# Everything runs under a private TMPDIR against a private origin
# (KNOBAS_MERGE_TAIL_REPO points the lock at a scratch clone), so neither the
# machine's real queue nor GitHub's lock ref is ever touched. The lock never
# asks `gh`: a `gh` on PATH that records being called proves it.
#
# MERGE_TAIL_LOCK=<path> runs it against another copy of the script, such as
# the per-machine one on `main` before #2335, to watch the two-machine cases
# fail; GATE_SLOT=<path> does the same for `scripts/gate-slot.sh` in case 5.
set -uo pipefail # no -e: a failed check is counted, and the run goes on
here=$(cd "$(dirname "$0")" && pwd)
# Under a holder's re-gate (`hold <PR> just gate` runs `witness-unit`) the
# environment already carries the mark case 5 looks for, and maybe overrides.
unset KNOBAS_MERGE_TAIL_HOLDER KNOBAS_MERGE_TAIL_REMOTE KNOBAS_MERGE_TAIL_REF
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-merge-tail-check.XXXXXX")
# A private copy, so the cleanup and the watchdog can kill exactly this test's
# `take`s and never a merge-manager's.
lock_script=$scratch/merge-tail-lock.sh
cp "${MERGE_TAIL_LOCK:-$here/merge-tail-lock.sh}" "$lock_script"
trap 'pkill -f "$lock_script" 2>/dev/null; rm -rf "$scratch"' EXIT
export TMPDIR=$scratch/tmp
mkdir -p "$TMPDIR" "$scratch/tmp-b" "$scratch/bin"
# A broken lock can make a `take` wait forever: after 150 s, kill this test's
# `take`s so the failures surface and the run ends.
(
    while [ -d "$scratch" ] && [ "$SECONDS" -lt 150 ]; do sleep 1; done
    [ -d "$scratch" ] && { echo "FAIL: timed out after 150 s" >&2; pkill -f "$lock_script"; }
) &
export PATH=$scratch/bin:$PATH KNOBAS_MERGE_TAIL_POLL=1 KNOBAS_MERGE_TAIL_LEASE=600 KNOBAS_MERGE_TAIL_WAIT=600

printf '#!/usr/bin/env bash\ntouch "%s/gh-called"; echo "stub: the lock asked gh: $*" >&2; exit 1\n' "$scratch" > "$scratch/bin/gh"
chmod +x "$scratch/bin/gh"

g() { git -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false "$@"; }
# origin, and one clone per machine. `-b main` on the bare one: its HEAD is
# what a clone checks out, and git's own default is `master` on ubuntu-latest
# (#2035, Linux only).
origin=$scratch/origin.git
git init -q --bare -b main "$origin"
for m in a b; do git init -q -b main "$scratch/$m"; git -C "$scratch/$m" remote add origin "$origin"; done
export KNOBAS_MERGE_TAIL_REPO=$scratch/a KNOBAS_MERGE_TAIL_MACHINE=machine-a
REF=refs/locks/merge-tail

QUEUE=$TMPDIR/knobas-merge-tail.queue
lock() { bash "$lock_script" "$@"; }
lock_b() { TMPDIR=$scratch/tmp-b KNOBAS_MERGE_TAIL_REPO=$scratch/b KNOBAS_MERGE_TAIL_MACHINE=machine-b bash "$lock_script" "$@"; }
assert_status() { lock assert "$1" > /dev/null && echo 0 || echo 1; }
assert_b_status() { lock_b assert "$1" > /dev/null && echo 0 || echo 1; }
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
# The slot as origin sees it, and the PR its token names.
slot() { git -C "$origin" rev-parse -q --verify "$REF"; }
slot_holder() { local s; s=$(slot) && git -C "$origin" cat-file commit "$s" | sed -n 's/^holder: //p'; }
# expire: the holder stops refreshing and its lease runs out, as origin sees
# it: the token is rewritten with an expiry 100 s in the past, its holder kept.
expire() {
    local s msg tree new
    s=$(slot) || return 1
    msg=$(git -C "$origin" cat-file commit "$s" | sed '1,/^$/d' | sed "s/^expires: .*/expires: $(($(date +%s) - 100))/")
    tree=$(git -C "$origin" hash-object -t tree -w /dev/null)
    new=$(g -C "$origin" commit-tree "$tree" -m "$msg")
    git -C "$origin" update-ref "$REF" "$new" "$s"
}
# Each case starts with a free slot and no queue on either machine.
fresh() { git -C "$origin" update-ref -d "$REF" 2>/dev/null; rm -rf "$TMPDIR"/knobas-merge-tail.* "$scratch"/tmp-b/knobas-merge-tail.*; }

failures=0
check() { # check <label> <command...>
    local label=$1; shift
    if "$@"; then echo "ok: $label"; else echo "FAIL: $label" >&2; failures=$((failures + 1)); fi
}
has() { grep -q -- "$2" "$1"; }
lacks() { ! grep -q -- "$2" "$1"; }

# --- 0. Two machines share one holder slot on origin (#2335) ---
# Two takes race from two clones on a free slot, five rounds: exactly one
# exits 0 each time, and the slot names it; the other is still queued.
# `racing <s>` puts a git on PATH that waits <s> seconds before every push.
# One machine waits 1 s, the other 2 s, so both read the same free slot before
# either pushes, and the second push carries a lease that is already stale:
# the race happens every round, not only when two calls happen to interleave.
# (Equal waits would make both pushes meet origin's own ref lock at once, and
# one would fail on that instead of on the lease.)
export KNOBAS_MERGE_TAIL_WAIT=3
real_git=$(command -v git)
mkdir -p "$scratch/slow-push"
# shellcheck disable=SC2016 # the wrapper's own shell expands it
printf '#!/usr/bin/env bash\nfor a in "$@"; do [ "$a" = push ] && { sleep "$SLOW_PUSH"; break; }; done\nexec "%s" "$@"\n' "$real_git" > "$scratch/slow-push/git"
chmod +x "$scratch/slow-push/git"
racing() { local s=$1; shift; SLOW_PUSH=$s PATH=$scratch/slow-push:$PATH "$@"; }
for round in 1 2 3 4 5; do
    fresh
    da=$((1 + round % 2)); db=$((2 - round % 2)) # who pushes first alternates
    racing "$da" lock take "14${round}1" > "$scratch/race$round.a" 2>&1 & ja=$!
    racing "$db" lock_b take "14${round}2" > "$scratch/race$round.b" 2>&1 & jb=$!
    wait "$ja"; sa=$?; wait "$jb"; sb=$?
    winners=$(( (sa == 0) + (sb == 0) ))
    check "race $round: two machines on a free slot, exactly one gets the lock (A $sa, B $sb)" [ "$winners" -eq 1 ]
    if [ "$sa" = 0 ]; then want="14${round}1"; else want="14${round}2"; fi
    check "race $round: the slot on origin names the winner" [ "$(slot_holder)" = "$want" ]
    check "race $round: the loser is still queued (exit 4)" [ $((sa + sb)) -eq 4 ]
done

# Two machines find a dead holder: exactly one takes it over.
fresh
lock take 1400 > /dev/null
expire
racing 1 lock take 1401 > "$scratch/dead.a" 2>&1 & ja=$!
racing 2 lock_b take 1402 > "$scratch/dead.b" 2>&1 & jb=$!
wait "$ja"; sa=$?; wait "$jb"; sb=$?
check "dead holder, two machines: exactly one gets the lock (A $sa, B $sb)" [ $(( (sa == 0) + (sb == 0) )) -eq 1 ]
takeovers=$(cat "$scratch/dead.a" "$scratch/dead.b" | grep -c "taking it over")
check "dead holder, two machines: exactly one says it takes over (got $takeovers)" [ "$takeovers" -eq 1 ]

# A holder whose lease ran out is taken over from the other machine; its own
# assert then exits 1, and its release leaves the new holder in place.
fresh
KNOBAS_MERGE_TAIL_LEASE=2 lock take 1420 > /dev/null
sleep 3
lock_b take 1421 > "$scratch/over.b" 2>&1; sb=$?
check "expired lease: machine B takes it over (exit $sb)" [ "$sb" = 0 ]
check "expired lease: B says whose lease it took over" has "$scratch/over.b" "PR #1420 .*lease.*taking it over"
check "expired lease: the old holder's assert exits 1" [ "$(assert_status 1420)" = 1 ]
lock release 1420 > /dev/null
check "expired lease: the old holder's release leaves B holding" [ "$(assert_b_status 1421)" = 0 ]
check "expired lease: B's assert refreshed the slot, B still named" [ "$(slot_holder)" = 1421 ]
lock_b release 1421 > /dev/null
check "release: the slot on origin is gone" [ -z "$(slot)" ]

# A live holder on the other machine keeps the lock: A waits.
fresh
lock_b take 1431 > /dev/null
status=$(KNOBAS_MERGE_TAIL_WAIT=2 take_within 10 1430 "$scratch/live.a")
check "live holder on B: A is still queued (status $status)" [ "$status" = 4 ]
check "live holder on B: A names the holder" has "$scratch/live.a" "#1431"
# An origin nobody can reach never holds.
status=$(KNOBAS_MERGE_TAIL_REMOTE=$scratch/nowhere.git KNOBAS_MERGE_TAIL_WAIT=2 take_within 10 1432 "$scratch/gone.out")
check "origin unreachable: take stays queued (status $status)" [ "$status" = 4 ]
check "origin unreachable: assert exits 1 even for the holder" [ "$(KNOBAS_MERGE_TAIL_REMOTE=$scratch/nowhere.git assert_b_status 1431)" = 1 ]
lock_b release 1431 > /dev/null
export KNOBAS_MERGE_TAIL_WAIT=600

# --- 1. A holder that refreshes its claim keeps the lock ---
# Lease 3 s, poll 1 s: `hold` runs a 6 s command, a waiter polls throughout.
fresh
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
# `take` on a lapsed claim keep a polling waiter out.
fresh
lock take 1271 > /dev/null
expire
lock assert 1271 > /dev/null
status=$(take_within 3 1272 "$scratch/w1c.out")
check "assert refreshes the claim: the waiter does not take over (status $status)" lacks "$scratch/w1c.out" "taking it over"
expire
lock take 1271 > "$scratch/r1.out"
check "re-entrant take: already held" has "$scratch/r1.out" "already held by PR #1271"
status=$(take_within 3 1272 "$scratch/w1d.out")
check "re-entrant take refreshes the claim: the waiter does not take over (status $status)" lacks "$scratch/w1d.out" "taking it over"
lock release 1271 > /dev/null

# --- 2. A holder whose claim outlived the lease is taken over ---
fresh
lock take 1260 > /dev/null
expire
status=$(take_within 10 1261 "$scratch/w2.out")
check "dead holder, PR still open: the waiter takes the lock over (status $status)" has "$scratch/w2.out" "held by PR #1261"
check "dead holder: the takeover is said" has "$scratch/w2.out" "taking it over"
lock release 1261 > /dev/null

# Two waiters on one machine find the dead holder; exactly one takes it over,
# the lowest-numbered gets the lock.
fresh
lock take 1260 > /dev/null
expire
# 1261 is already in the queue, so 1262 can never be the lowest live waiter,
# however the two start.
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

# --- 4. The pre-gate record: merge without gating again only when nothing moved ---
# The shared origin and two more clones: `wt` is the merge-manager's worktree,
# `other` moves main the way another merge does.
wt=$scratch/wt
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

# --- 5. The holder's gate goes first on the gate slots (#1866) ---
fresh
lock take 1320 > /dev/null
# shellcheck disable=SC2016 # the command's own shell expands it
lock hold 1320 bash -c 'echo "holder=${KNOBAS_MERGE_TAIL_HOLDER:-none}"' > "$scratch/s1.out" 2>&1
check "hold: its command is told it runs for the holder" has "$scratch/s1.out" "holder=1320"

# One gate slot, held by a live pid; the pre-gate starts waiting first. The
# slot frees while the holder's gate is stopped (alive, but it cannot poll),
# so on first come, first served the pre-gate's next poll would take it.
# Each step waits for what it needs to see, not for a fixed time.
slots_dir=$TMPDIR/knobas-gate-slots
await() { # await <seconds> <command...>: polls every 0.1 s until it succeeds
    local n=$(($1 * 10)); shift
    until "$@"; do n=$((n - 1)); [ "$n" -gt 0 ] || return 1; sleep 0.1; done
}
mkdir -p "$slots_dir"
sleep 60 & busy=$!
ln -s "$busy" "$slots_dir/slot-1"
printf '#!/usr/bin/env bash\n. "%s"; trap release_slot EXIT; hold_gate_slot test && sleep 3\n' \
    "${GATE_SLOT:-$here/gate-slot.sh}" > "$scratch/gate.sh"
export KNOBAS_GATE_SLOTS=1 KNOBAS_GATE_SLOT_WAIT=20
bash "$scratch/gate.sh" > "$scratch/s2.pre" 2>&1 & pre=$!
await 10 has "$scratch/s2.pre" "waiting up to"
lock hold 1320 bash "$scratch/gate.sh" > "$scratch/s2.holder" 2>&1 & holder_gate=$!
await 10 has "$scratch/s2.holder" "waiting up to"
gate_pid=$(pgrep -f "^bash $scratch/gate.sh" | grep -vx "$pre")
kill -STOP "$gate_pid"
kill "$busy"; wait "$busy" 2>/dev/null
sleep 2.5 # the pre-gate's once-a-second poll comes round twice
check "slots: the pre-gate leaves the freed slot to the holder's gate" lacks "$scratch/s2.pre" "gate slot free after"
kill -CONT "$gate_pid"
await 10 has "$scratch/s2.holder" "gate slot free after"
check "slots: the holder's gate under hold takes the freed slot" has "$scratch/s2.holder" "gate slot free after"
check "slots: the pre-gate that waited longer is still waiting" lacks "$scratch/s2.pre" "gate slot free after"
wait "$holder_gate" "$pre"
check "slots: the pre-gate takes the slot after the holder's run" has "$scratch/s2.pre" "gate slot free after"
unset KNOBAS_GATE_SLOTS KNOBAS_GATE_SLOT_WAIT
lock release 1320 > /dev/null

check "the lock never asks GitHub" [ ! -e "$scratch/gh-called" ]

if [ "$failures" -ne 0 ]; then echo "merge-tail-lock-test: $failures failed" >&2; exit 1; fi
echo "merge-tail-lock-test: ok"
