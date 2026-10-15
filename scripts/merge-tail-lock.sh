#!/usr/bin/env bash
# Machine-wide merge-tail lock: reviews run in parallel, only rebase + gate +
# merge + cleanup run one at a time, oldest PR first.
#
# Usage: scripts/merge-tail-lock.sh take <PR>   (blocks until held)
#        scripts/merge-tail-lock.sh release <PR>
#
# A waiter registers in the queue dir and touches its entry every poll. The lock
# goes to the lowest-numbered live waiter, since every merge makes the others
# rebase and an older branch has the most to rebase. An entry not touched for
# 120 s is a dead waiter and is ignored. A holder whose PR is merged or closed
# is taken over.
set -u
LOCK="${TMPDIR:-/tmp}/knobas-merge-tail.lock"
QUEUE="${TMPDIR:-/tmp}/knobas-merge-tail.queue"
cmd="${1:-}"; pr="${2:-}"
[ -n "$cmd" ] && [ -n "$pr" ] || { echo "usage: $0 take|release <PR>"; exit 2; }
oldest_live_waiter() {
  local f n best='' now; now=$(date +%s)
  for f in "$QUEUE"/*; do
    [ -e "$f" ] || continue
    n=${f##*/}
    [ $((now - $(stat -f %m "$f"))) -le 120 ] || continue
    if [ -z "$best" ] || [ "$n" -lt "$best" ]; then best=$n; fi
  done
  echo "${best:-}"
}
case "$cmd" in
take)
  mkdir -p "$QUEUE"; said=""
  while :; do
    touch "$QUEUE/$pr"
    first=$(oldest_live_waiter)
    if [ "$first" = "$pr" ] && mkdir "$LOCK" 2>/dev/null; then break; fi
    holder=$(cat "$LOCK/holder" 2>/dev/null || true)
    if [ -n "$holder" ]; then
      state=$(gh pr view "$holder" --json state --jq .state 2>/dev/null || echo UNKNOWN)
      if [ "$state" = MERGED ] || [ "$state" = CLOSED ]; then
        echo "merge-tail: PR #$holder is $state but still holds the lock; taking it over"
        rm -rf "$LOCK"; continue
      fi
    fi
    [ -z "$said" ] && { echo "merge-tail: waiting (holder #${holder:-none}, first in queue #${first:-?})"; said=1; }
    sleep 30
  done
  rm -f "$QUEUE/$pr"
  echo "$pr" > "$LOCK/holder"
  echo "merge-tail: held by PR #$pr"
  ;;
release)
  rm -f "$QUEUE/$pr"
  holder=$(cat "$LOCK/holder" 2>/dev/null || true)
  if [ "$holder" = "$pr" ]; then rm -rf "$LOCK"; echo "merge-tail: released by PR #$pr"
  else echo "merge-tail: PR #$pr does not hold the lock (holder: ${holder:-none})"; fi
  ;;
*) echo "usage: $0 take|release <PR>"; exit 2 ;;
esac
