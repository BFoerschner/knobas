#!/usr/bin/env bash
# Machine-wide merge-tail lock: reviews run in parallel, only rebase + gate +
# merge + cleanup run one at a time, oldest PR first.
#
# Usage: scripts/merge-tail-lock.sh ready <PR> <head SHA>   (review done; may be batched)
#        scripts/merge-tail-lock.sh take <PR>      (blocks until held; exit 3 = merged in a batch)
#        scripts/merge-tail-lock.sh partner <PR>   (the holder asks: oldest other ready, waiting PR)
#        scripts/merge-tail-lock.sh merged <PR> <merge SHA>   (the holder merged it in a batch)
#        scripts/merge-tail-lock.sh release <PR>
#
# Batching (Björn, 2026-10-15): the holder may stack one ready waiter on its own
# head, gate the pair once and merge both. `ready` records the head SHA a
# waiter's review settled on; `merged` tells that waiter its PR is in, and its
# `take` then exits 3 with the merge SHA instead of taking the lock.
#
# A waiter registers in the queue dir and touches its entry every poll. The lock
# goes to the lowest-numbered live waiter, since every merge makes the others
# rebase and an older branch has the most to rebase. An entry not touched for
# 120 s is a dead waiter and is ignored. A holder whose PR is merged or closed
# is taken over.
set -u
LOCK="${TMPDIR:-/tmp}/knobas-merge-tail.lock"
QUEUE="${TMPDIR:-/tmp}/knobas-merge-tail.queue"
READY="${TMPDIR:-/tmp}/knobas-merge-tail.ready"
cmd="${1:-}"; pr="${2:-}"; arg="${3:-}"
[ -n "$cmd" ] && [ -n "$pr" ] || { echo "usage: $0 ready|take|partner|merged|release <PR> [SHA]"; exit 2; }
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
ready)
  [ -n "$arg" ] || { echo "usage: $0 ready <PR> <head SHA>"; exit 2; }
  mkdir -p "$READY"; echo "$arg" > "$READY/$pr"
  echo "merge-tail: PR #$pr ready at $arg"
  ;;
partner)
  now=$(date +%s); best=''
  for f in "$READY"/*; do
    [ -e "$f" ] || continue
    n=${f##*/}
    case $n in *.merged) continue ;; esac
    [ "$n" != "$pr" ] || continue
    [ ! -e "$READY/$n.merged" ] || continue
    [ -e "$QUEUE/$n" ] && [ $((now - $(stat -f %m "$QUEUE/$n"))) -le 120 ] || continue
    if [ -z "$best" ] || [ "$n" -lt "$best" ]; then best=$n; fi
  done
  [ -n "$best" ] && echo "$best $(cat "$READY/$best")"
  ;;
merged)
  [ -n "$arg" ] || { echo "usage: $0 merged <PR> <merge SHA>"; exit 2; }
  mkdir -p "$READY"; echo "$arg" > "$READY/$pr.merged"; rm -f "$QUEUE/$pr"
  echo "merge-tail: PR #$pr merged in a batch as $arg"
  ;;
take)
  mkdir -p "$QUEUE"; said=""
  while :; do
    if [ -e "$READY/$pr.merged" ]; then
      rm -f "$QUEUE/$pr" "$READY/$pr"
      echo "merge-tail: PR #$pr was merged in a batch as $(cat "$READY/$pr.merged"); do the cleanup only"
      exit 3
    fi
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
  rm -f "$QUEUE/$pr" "$READY/$pr" "$READY/$pr.merged"
  holder=$(cat "$LOCK/holder" 2>/dev/null || true)
  if [ "$holder" = "$pr" ]; then rm -rf "$LOCK"; echo "merge-tail: released by PR #$pr"
  else echo "merge-tail: PR #$pr does not hold the lock (holder: ${holder:-none})"; fi
  ;;
*) echo "usage: $0 ready|take|partner|merged|release <PR> [SHA]"; exit 2 ;;
esac
