#!/usr/bin/env bash
# The machine-wide JVM lock's own unit tests (issue #690), run by `just
# witness-unit` and so by `just check`. No container is started and no product
# is touched: `docker` is the same kind of stub `live-test.sh` uses, the seeds
# are stubs in a scratch testenv/, and the lock is a scratch path, never the
# real `$TMPDIR/knobas-live-jvm.lock` a live run on this machine may be holding.
#
# The rule under test: a live recipe that finds another JVM run holding the
# lock WAITS -- printing who holds it, once -- and goes on when it is
# released; a lock whose holder is dead is taken over, saying so; past the
# wait it refuses, naming the holder. And `live_session` takes the lock before
# its one-JVM guard and gives it up only after it has put the products back,
# so a queued run never meets the backstop refusal its predecessor caused.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-live-lock-test.XXXXXX")
pids=()
cleanup() {
  local p
  for p in ${pids[@]+"${pids[@]}"}; do kill "$p" 2>/dev/null || true; done
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'cleanup; trap - INT; kill -INT $$' INT
trap 'cleanup; trap - TERM; kill -TERM $$' TERM

# The docker stub: `ps` prints $STUB_RUNNING, `context show` prints orbstack,
# and every other call is logged with whether the lock existed at that moment.
mkdir -p "$scratch/bin"
cat >"$scratch/bin/docker" <<'STUB'
#!/usr/bin/env bash
case "${1:-} ${2:-}" in
  "ps "*) for n in ${STUB_RUNNING:-} $(cat "$STUB_RUNNING_FILE" 2>/dev/null); do echo "$n"; done ;;
  "context show") echo orbstack ;;
  *) if [ -L "$KNOBAS_LIVE_LOCK" ]; then l=held; else l=free; fi
     if [ -L "$KNOBAS_LIVE_GITEA_LOCK" ]; then g=held; else g=free; fi
     echo "docker $* [lock $l] [gitea lock $g]" >>"$STUB_LOG" ;;
esac
STUB
chmod +x "$scratch/bin/docker"
# A curl that answers: `live_up gitea` probes http://127.0.0.1:3000 for health,
# and without a stub the cases passed only on a machine with a real Gitea up
# there (a bare Linux runner has none, #1713).
printf '#!/bin/sh
exit 0
' >"$scratch/bin/curl"
chmod +x "$scratch/bin/curl"

export PATH="$scratch/bin:$PATH" STUB_LOG="$scratch/docker.log" STUB_RUNNING_FILE="$scratch/running"
export KNOBAS_LIVE_LOCK="$scratch/knobas-live-jvm.lock"
export KNOBAS_LIVE_GITEA_LOCK="$scratch/knobas-live-gitea.lock"
export KNOBAS_LIVE_LOCK_POLL=0.1
# Short, so a mutant that never lets a waiter through fails in seconds instead
# of hanging the gate for the real 45 minutes; cases that test the wait itself
# set their own.
export KNOBAS_LIVE_LOCK_WAIT=5
unset KNOBAS_LIVE_LOCK_HOLDER KNOBAS_LIVE_GITEA_LOCK_HOLDER

# A scratch testenv/ whose seeds do nothing, so `live_session` can run whole.
fake=$scratch/tree/testenv
mkdir -p "$fake"
for s in seed-gitea.sh seed-teamcity.sh seed-atlassian.sh seed-atlassian-content.sh seed; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$fake/$s"
  chmod +x "$fake/$s"
done

failures=0
checks=0
fail() {
  printf 'live-lock-test: FAIL %s\n' "$1" >&2
  shift
  printf '  %s\n' "$@" >&2
  failures=$((failures + 1))
}
check() { # check <what> <haystack> <needle>
  checks=$((checks + 1))
  case $2 in *"$3"*) ;; *) fail "$1" "wanted: $3" "got:    $2" ;; esac
}
check_not() {
  checks=$((checks + 1))
  case $2 in *"$3"*) fail "$1" "did not want: $3" "got:          $2" ;; esac
}

# wait_for <file> <needle> -- until the file contains the needle, at most 10 s.
wait_for() {
  local i
  for ((i = 0; i < 100; i++)); do
    if grep -qF -- "$2" "$1" 2>/dev/null; then return 0; fi
    sleep 0.1
  done
  return 1
}

# lock_gone <what> [<lock>] -- the check that a run's lock is free once it is
# done (#2120). A taker writes its .rc before its EXIT trap releases, so right
# after `wait_rc` the link can still be there for a moment under load: poll up
# to 5 s for it to go. A lock that is never given back still fails, naming
# what it points at.
lock_gone() {
  local lock=${2:-$KNOBAS_LIVE_LOCK} i
  checks=$((checks + 1))
  for ((i = 0; i < 50; i++)); do [ -L "$lock" ] || return 0; sleep 0.1; done
  fail "$1" "still held: $(readlink "$lock" 2>/dev/null || echo '(gone at the last look)')"
}

# holder <name> [<jvm|gitea>] -- a background shell in a tree of its own that
# takes the lock (the JVM one unless named) as recipe <name>, says so in
# $scratch/<name>.out, and lets it go when $scratch/<name>.release appears.
holder() {
  local name=$1 lock=${2:-jvm}
  mkdir -p "$scratch/$name/testenv"
  (
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=$scratch/$name/testenv
    trap 'live_lock_release' EXIT
    live_lock_hold "$name" "$lock"
    echo "holding" >>"$scratch/$name.out"
    while [ ! -e "$scratch/$name.release" ]; do sleep 0.1; done
  ) >>"$scratch/$name.out" 2>&1 &
  pids+=($!)
  wait_for "$scratch/$name.out" holding || fail "$name never took the lock" "$(cat "$scratch/$name.out")"
}

# taker <name> [<env>...] -- run `live_lock_hold <name>` in the scratch tree,
# in the background, output to $scratch/<name>.out, exit status to
# $scratch/<name>.rc.
taker() {
  local name=$1
  shift
  (
    for kv in "$@"; do export "${kv?}"; done
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=${TAKER_TREE:-$fake}
    trap 'live_lock_release' EXIT
    if live_lock_hold "$name"; then
      echo "held by $name"
      echo "target: $(readlink "$KNOBAS_LIVE_LOCK")"
      rc=0
    else rc=$?; fi
    echo "$rc" >"$scratch/$name.rc"
  ) >>"$scratch/$name.out" 2>&1 &
  pids+=($!)
}
wait_rc() { # wait_rc <name> -- the taker's exit status, waiting up to 10 s.
  local i
  for ((i = 0; i < 100; i++)); do
    if [ -s "$scratch/$1.rc" ]; then cat "$scratch/$1.rc"; return; fi
    sleep 0.1
  done
  echo timeout
}

# --- a second run waits, and goes on when the first lets go -----------------

holder first
taker second
wait_for "$scratch/second.out" "waiting" || true
out=$(cat "$scratch/second.out")
check "the second run says it is waiting" "$out" "waiting"
check "the waiting line names the holder's recipe" "$out" "first"
check "the waiting line names the holder's worktree" "$out" "$scratch/first"
check_not "the second run does not go on while the first holds" "$out" "held by second"
checks=$((checks + 1))
[ ! -s "$scratch/second.rc" ] || fail "the second run did not finish while the first held" "$out"
lines=$(grep -c 'is held by' "$scratch/second.out" || true)
sleep 0.5
lines2=$(grep -c 'is held by' "$scratch/second.out" || true)
checks=$((checks + 1))
if [ "$lines" != 1 ] || [ "$lines2" != 1 ]; then fail "the holder is printed once" "$(cat "$scratch/second.out")"; fi
# A second past the waiter's start, so a lock stamped when it began waiting
# rather than when it took the lock reads earlier than the release.
sleep 1
released_at=$(date +%s)
touch "$scratch/first.release"
rc=$(wait_rc second)
out=$(cat "$scratch/second.out")
taken_at=$(sed -n 's/^target: [0-9]* \([0-9]*\) .*/\1/p' "$scratch/second.out")
checks=$((checks + 1))
if [ -z "$taken_at" ] || [ "$taken_at" -lt "$released_at" ]; then
  fail "the lock says when it was taken, not when the wait began" \
    "released at $released_at, lock stamped ${taken_at:-?}"
fi
check "the second run goes on once the first lets go" "$rc" "0"
check "the second run held the lock" "$out" "held by second"
check "the second run says how long it waited" "$out" "after"
lock_gone "the lock is free once both are done"

# --- a stale lock is taken over -----------------------------------------------

sh -c 'exit 0' &
dead=$!
wait "$dead" || true
ln -s "$dead 1700000000 corpse-recipe /some/gone/worktree" "$KNOBAS_LIVE_LOCK"
taker heir
rc=$(wait_rc heir)
out=$(cat "$scratch/heir.out")
check "a lock whose holder is dead is taken over" "$rc" "0"
check "the takeover says so" "$out" "stale"
check "the takeover names the dead holder" "$out" "corpse-recipe"
check_not "a stale lock is not waited on" "$out" "waiting"
lock_gone "the heir released the lock"

# The same, the way it happens: a run SIGKILLed while it holds the lock, so no
# trap gives it back. The lock must name the run's own process -- here a
# subshell -- for its death to be seen.
holder killed
killed_pid=${pids[${#pids[@]}-1]}
kill -9 "$killed_pid"
wait "$killed_pid" 2>/dev/null || true
taker afterkill
rc=$(wait_rc afterkill)
out=$(cat "$scratch/afterkill.out")
check "a SIGKILLed run's lock is taken over" "$rc" "0"
check "the takeover names the killed run" "$out" "'killed' in $scratch/killed"

# --- the wait has an end ------------------------------------------------------

holder blocker
taker late KNOBAS_LIVE_LOCK_WAIT=1
rc=$(wait_rc late)
out=$(cat "$scratch/late.out")
check_not "past the wait the run refuses" "$rc" "0"
check "the refusal names the holder's recipe" "$out" "blocker"
check "the refusal names the holder's worktree" "$out" "$scratch/blocker"
check "the refusal says it gave up" "$out" "giving up"
checks=$((checks + 1))
[ -L "$KNOBAS_LIVE_LOCK" ] || fail "a refused run leaves the holder's lock alone" "lock is gone"

# A run started under a holder -- `just atlassian-live` running `just
# jira-live` -- is let through on the lock its parent holds, and does not
# release it.
holder_target=$(readlink "$KNOBAS_LIVE_LOCK")
taker child "KNOBAS_LIVE_LOCK_HOLDER=$holder_target"
rc=$(wait_rc child)
check "a child of the holder goes on at once" "$rc" "0"
checks=$((checks + 1))
[ "$(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || true)" = "$holder_target" ] ||
  fail "the child leaves its parent's lock in place" "now: $(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || echo none)"

# --- something at the lock's path that is not a link -----------------------------

# Neither waited on (there is no holder, and `ln -s` would fail on a file
# forever, past any timeout) nor taken (`ln -s` onto a directory makes its link
# inside it and succeeds): refused, naming the path.
# The blocker still holds the lock; its link is put back after.
saved=$(readlink "$KNOBAS_LIVE_LOCK")
for kind in file directory; do
  rm -f "$KNOBAS_LIVE_LOCK"
  if [ "$kind" = file ]; then : >"$KNOBAS_LIVE_LOCK"; else mkdir "$KNOBAS_LIVE_LOCK"; fi
  taker "not-a-link-$kind" KNOBAS_LIVE_LOCK_WAIT=30
  rc=$(wait_rc "not-a-link-$kind")
  out=$(cat "$scratch/not-a-link-$kind.out")
  check_not "a $kind at the lock's path is refused" "$rc" "0"
  check_not "a $kind at the lock's path is refused at once, not timed out" "$rc" "timeout"
  check "the refusal names the path" "$out" "$KNOBAS_LIVE_LOCK is not the JVM lock's symlink"
  rm -rf "$KNOBAS_LIVE_LOCK"
done
ln -s "$saved" "$KNOBAS_LIVE_LOCK"

# --- live_session takes the lock before its guard, and gives it up last -------

# The blocker's TeamCity is up. A queued `live_session teamcity` must meet the
# lock (and say who holds it), never the one-JVM refusal of the guard.
session() { # session <name> <product> [<env>...]
  local name=$1 product=$2
  shift 2
  (
    for kv in "$@"; do export "${kv?}"; done
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=$fake
    live_session "$product" "$name"
  ) >"$scratch/$name.out" 2>&1 &
  # Bounded, as the takers are: a mutant that waits forever is a failure here,
  # not a hung gate.
  local pid=$! i
  for ((i = 0; i < 200; i++)); do
    if ! kill -0 "$pid" 2>/dev/null; then
      if wait "$pid"; then echo 0; else echo $?; fi
      return
    fi
    sleep 0.1
  done
  kill "$pid" 2>/dev/null || true
  echo timeout
}
rc=$(session queued teamcity STUB_RUNNING="knobas-teamcity knobas-teamcity-agent knobas-jira" KNOBAS_LIVE_LOCK_WAIT=1)
out=$(cat "$scratch/queued.out")
check_not "a session queued behind a holder is refused only after the wait" "$rc" "0"
check "the queued session names the lock's holder" "$out" "blocker"
check_not "the queued session never reaches the one-JVM guard" "$out" "one JVM product at a time"
touch "$scratch/blocker.release"
for ((i = 0; i < 100; i++)); do [ -L "$KNOBAS_LIVE_LOCK" ] || break; sleep 0.1; done

: >"$STUB_LOG"
rc=$(session own teamcity STUB_RUNNING="")
out=$(cat "$scratch/own.out")
log=$(cat "$STUB_LOG")
check "a session with the lock free runs" "$rc" "0"
check "the session stops what it started while still holding the lock" "$log" \
  "stop teamcity teamcity-agent [lock held]"
check "and it starts it holding the lock" "$log" "up -d teamcity teamcity-agent [lock held]"
lock_gone "the session releases the lock at its end"

# The backstop stays: with the lock free, a hand-started Jira still refuses a
# TeamCity session -- and the refusal gives the lock back.
rc=$(session byhand teamcity STUB_RUNNING="knobas-jira")
out=$(cat "$scratch/byhand.out")
check_not "the one-JVM guard still refuses behind a free lock" "$rc" "0"
check "the guard's refusal names the product that is up" "$out" "knobas-jira is running"
lock_gone "a refused session gives the lock back"

# Gitea is not a JVM product: its session neither takes nor waits on the JVM lock.
holder gitearun
: >"$STUB_LOG"
rc=$(session giteaonly gitea STUB_RUNNING="" KNOBAS_LIVE_LOCK_WAIT=1)
check "a Gitea session does not wait on the JVM lock" "$rc" "0"
check "the Gitea session's own calls ran" "$(cat "$STUB_LOG")" "up -d --wait gitea"
touch "$scratch/gitearun.release"
for ((i = 0; i < 100; i++)); do [ -L "$KNOBAS_LIVE_LOCK" ] || break; sleep 0.1; done

# --- the Gitea lock (issue #724): Gitea runs queue behind each other ----------

# Two `just gitea-live` from two worktrees broke each other: one run's cleanup
# deleted its branch under the other's full sync. So a Gitea session takes the
# Gitea lock -- the same mechanism under another path -- and a second one waits.
holder giteaholder gitea
: >"$STUB_LOG"
rc=$(session giteaqueued gitea STUB_RUNNING="" KNOBAS_LIVE_LOCK_WAIT=1)
out=$(cat "$scratch/giteaqueued.out")
check_not "a Gitea session behind a Gitea run is refused only after the wait" "$rc" "0"
check "the queued Gitea session says it waits for the Gitea lock" "$out" "the Gitea lock is held by"
check "and names the Gitea run that holds it" "$out" "'giteaholder' in $scratch/giteaholder"
checks=$((checks + 1))
[ ! -s "$STUB_LOG" ] || fail "a queued Gitea session touches nothing while it waits" "$(cat "$STUB_LOG")"

# The other side: once the holder lets go, the waiter goes on and runs.
(session giteawaiter gitea STUB_RUNNING="" >"$scratch/giteawaiter.rc") &
pids+=($!)
wait_for "$scratch/giteawaiter.out" "waiting" || fail "the second Gitea session never waited" "$(cat "$scratch/giteawaiter.out")"
touch "$scratch/giteaholder.release"
rc=$(wait_rc giteawaiter)
out=$(cat "$scratch/giteawaiter.out")
check "the waiting Gitea session goes on once the first lets go" "$rc" "0"
check "and says how long it waited" "$out" "took the Gitea lock after"
check "it starts Gitea holding the Gitea lock" "$(cat "$STUB_LOG")" "up -d --wait gitea [lock free] [gitea lock held]"
check_not "and, as every run, leaves Gitea up (#698)" "$(cat "$STUB_LOG")" "stop gitea"
lock_gone "the Gitea session releases its lock at its end" "$KNOBAS_LIVE_GITEA_LOCK"

# The two locks are two: a JVM run does not wait on a Gitea run.
holder giteaonlyholder gitea
: >"$STUB_LOG"
rc=$(session jvmbesidegitea teamcity STUB_RUNNING="" KNOBAS_LIVE_LOCK_WAIT=1)
check "a TeamCity session does not wait on the Gitea lock" "$rc" "0"
touch "$scratch/giteaonlyholder.release"
for ((i = 0; i < 100; i++)); do [ -L "$KNOBAS_LIVE_GITEA_LOCK" ] || break; sleep 0.1; done

# A Gitea run killed outright (SIGKILL: no trap runs) leaves its link behind;
# the next Gitea session takes it over instead of waiting out the 45 minutes.
holder giteakilled gitea
kill -9 "${pids[${#pids[@]} - 1]}"
wait "${pids[${#pids[@]} - 1]}" 2>/dev/null || true
checks=$((checks + 1))
[ -L "$KNOBAS_LIVE_GITEA_LOCK" ] || fail "a killed Gitea holder leaves its lock behind" "no link"
rc=$(session giteaheir gitea STUB_RUNNING="" KNOBAS_LIVE_LOCK_WAIT=1)
out=$(cat "$scratch/giteaheir.out")
check "a Gitea lock whose holder was killed is taken over" "$rc" "0"
check "the takeover says the Gitea lock was stale and names the holder" "$out" "the Gitea lock was stale -- its holder 'giteakilled'"
lock_gone "the heir releases the Gitea lock" "$KNOBAS_LIVE_GITEA_LOCK"

# --- a `live-up` product keeps the JVM lock until it is down (#2121) ----------

# `live up confluence` used to let the lock go once the product was seeded, so
# a suite run by hand against it held nothing and another worktree's recipe
# ran beside it. Now a keeper holds it for the product's life: that worktree's
# own runs go on under it, every other one waits.
export KNOBAS_LIVE_KEEPER_POLL=0.2 JIRA_LICENSE_KEY=x CONFLUENCE_LICENSE_KEY=x
liveup() { # liveup <name> <product> -- `testenv/live up <product>` in the scratch tree
  (
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=${3:-$fake}
    live_up_held "$2"
  ) >"$scratch/$1.out" 2>&1 && echo 0 || echo $?
}
echo knobas-confluence >"$STUB_RUNNING_FILE"
rc=$(liveup up1 confluence)
check "live-up of a JVM product succeeds" "$rc" "0"
keeper=$(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || true)
check "after live-up the JVM lock is still held, by live-up" "$keeper" " live-up-confluence-kept $scratch/tree"
checks=$((checks + 1))
_lt_alive() { [ -n "$1" ] && kill -0 "${1%% *}" 2>/dev/null; }
_lt_alive "$keeper" || fail "the lock's holder outlives the live-up that started it" "$keeper"
check "live-up says the product keeps the lock" "$(cat "$scratch/up1.out")" "holds the JVM lock until"
taker otherworktree TAKER_TREE="$scratch/elsewhere/testenv" KNOBAS_LIVE_LOCK_WAIT=1
rc=$(wait_rc otherworktree)
check_not "another worktree's run waits on a live-up product, then refuses" "$rc" "0"
check "and names the live-up holding it" "$(cat "$scratch/otherworktree.out")" "'live-up-confluence-kept' in $scratch/tree"
check "and says how to free it" "$(cat "$scratch/otherworktree.out")" "'just live-down confluence' from any worktree frees it)"
taker sameworktree KNOBAS_LIVE_LOCK_WAIT=1
rc=$(wait_rc sameworktree)
check "the live-up's own worktree runs under its lock" "$rc" "0"
sleep 0.3
check "and leaves the lock with the keeper" "$(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || true)" "$keeper"
mkdir -p "$scratch/elsewhere/testenv"
rc=$( (
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$scratch/elsewhere/testenv
  live_down confluence
) >"$scratch/down1.out" 2>&1 && echo 0 || echo $?)
check "live-down from another worktree succeeds" "$rc" "0"
lock_gone "live-down, from any worktree, gives the live-up's lock back"
checks=$((checks + 1))
for ((i = 0; i < 50; i++)); do _lt_alive "$keeper" || break; sleep 0.1; done
! _lt_alive "$keeper" || fail "live-down ends the keeper" "$keeper"
# A product that goes away without live-down (removed by hand, a VM restart)
# frees the lock too, at the keeper's next look.
rc=$(liveup up2 confluence)
check "a second live-up succeeds" "$rc" "0"
: >"$STUB_RUNNING_FILE"
lock_gone "a live-up product that is gone frees the lock"
# A forgotten live-down does not hold the lock for good: the keeper lets go
# once its worktree is removed, or after KNOBAS_LIVE_KEEPER_MAX.
echo knobas-confluence >"$STUB_RUNNING_FILE"
mkdir -p "$scratch/gone"
cp -R "$scratch/tree" "$scratch/gone/"
rc=$(liveup up3 confluence "$scratch/gone/tree/testenv")
check "a live-up in a worktree to be removed succeeds" "$rc" "0"
check "and its keeper holds the lock" "$(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || true)" "live-up-confluence-kept $scratch/gone/tree"
rm -r "$scratch/gone/tree"
lock_gone "a live-up whose worktree is removed frees the lock"
rc=$(KNOBAS_LIVE_KEEPER_MAX=1 liveup up4 confluence)
check "a live-up with a short keeper life succeeds" "$rc" "0"
check "and its keeper holds the lock" "$(readlink "$KNOBAS_LIVE_LOCK" 2>/dev/null || true)" "live-up-confluence-kept $scratch/tree"
sleep 1
lock_gone "a keeper lets go after KNOBAS_LIVE_KEEPER_MAX"
: >"$STUB_RUNNING_FILE"

if [ "$failures" -gt 0 ]; then
  echo "live-lock-test: $failures of $checks checks failed" >&2
  exit 1
fi
echo "live-lock-test: all $checks checks passed"
