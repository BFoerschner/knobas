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
  "ps "*) for n in ${STUB_RUNNING:-}; do echo "$n"; done ;;
  "context show") echo orbstack ;;
  *) if [ -L "$KNOBAS_LIVE_LOCK" ]; then l=held; else l=free; fi
     echo "docker $* [lock $l]" >>"$STUB_LOG" ;;
esac
STUB
chmod +x "$scratch/bin/docker"

export PATH="$scratch/bin:$PATH" STUB_LOG="$scratch/docker.log"
export KNOBAS_LIVE_LOCK="$scratch/knobas-live-jvm.lock"
export KNOBAS_LIVE_LOCK_POLL=0.1
# Short, so a mutant that never lets a waiter through fails in seconds instead
# of hanging the gate for the real 45 minutes; cases that test the wait itself
# set their own.
export KNOBAS_LIVE_LOCK_WAIT=5
unset KNOBAS_LIVE_LOCK_HOLDER

# A scratch testenv/ whose seeds do nothing, so `live_session` can run whole.
fake=$scratch/tree/testenv
mkdir -p "$fake"
for s in seed-gitea.sh seed-teamcity.sh seed; do
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

# holder <name> -- a background shell in a tree of its own that takes the lock
# as recipe <name>, says so in $scratch/<name>.out, and lets it go when
# $scratch/<name>.release appears.
holder() {
  local name=$1
  mkdir -p "$scratch/$name/testenv"
  (
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=$scratch/$name/testenv
    trap 'live_lock_release' EXIT
    live_lock_hold "$name"
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
    LIVE_TESTENV=$fake
    trap 'live_lock_release' EXIT
    if live_lock_hold "$name"; then echo "held by $name"; rc=0; else rc=$?; fi
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
[ "$lines" = 1 ] && [ "$lines2" = 1 ] || fail "the holder is printed once" "$(cat "$scratch/second.out")"
touch "$scratch/first.release"
rc=$(wait_rc second)
out=$(cat "$scratch/second.out")
check "the second run goes on once the first lets go" "$rc" "0"
check "the second run held the lock" "$out" "held by second"
check "the second run says how long it waited" "$out" "after"
checks=$((checks + 1))
[ ! -L "$KNOBAS_LIVE_LOCK" ] || fail "the lock is free once both are done" "$(readlink "$KNOBAS_LIVE_LOCK")"

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
checks=$((checks + 1))
[ ! -L "$KNOBAS_LIVE_LOCK" ] || fail "the heir released the lock" "$(readlink "$KNOBAS_LIVE_LOCK")"

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
  ) >"$scratch/$name.out" 2>&1 && echo 0 || echo $?
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
checks=$((checks + 1))
[ ! -L "$KNOBAS_LIVE_LOCK" ] || fail "the session releases the lock at its end" "$(readlink "$KNOBAS_LIVE_LOCK")"

# The backstop stays: with the lock free, a hand-started Jira still refuses a
# TeamCity session -- and the refusal gives the lock back.
rc=$(session byhand teamcity STUB_RUNNING="knobas-jira")
out=$(cat "$scratch/byhand.out")
check_not "the one-JVM guard still refuses behind a free lock" "$rc" "0"
check "the guard's refusal names the product that is up" "$out" "knobas-jira is running"
checks=$((checks + 1))
[ ! -L "$KNOBAS_LIVE_LOCK" ] || fail "a refused session gives the lock back" "$(readlink "$KNOBAS_LIVE_LOCK")"

# Gitea is not a JVM product: its session neither takes nor waits on the lock.
holder gitearun
: >"$STUB_LOG"
rc=$(session giteaonly gitea STUB_RUNNING="" KNOBAS_LIVE_LOCK_WAIT=1)
check "a Gitea session does not wait on the JVM lock" "$rc" "0"
check "the Gitea session's own calls ran" "$(cat "$STUB_LOG")" "up -d --wait gitea"
touch "$scratch/gitearun.release"

if [ "$failures" -gt 0 ]; then
  echo "live-lock-test: $failures of $checks checks failed" >&2
  exit 1
fi
echo "live-lock-test: all $checks checks passed"
