#!/usr/bin/env bash
# Machine-wide merge-tail lock: reviews run in parallel, only rebase + gate +
# merge + cleanup run one at a time, oldest PR first.
#
# Usage: scripts/merge-tail-lock.sh ready <PR> <head SHA>   (review done; may be batched)
#        scripts/merge-tail-lock.sh take <PR>      (exit 0 = held; 3 = merged in a batch; 4 = still queued, call again)
#        scripts/merge-tail-lock.sh hold <PR> <command...>   (the holder runs a long command; its claim stays fresh meanwhile)
#        scripts/merge-tail-lock.sh partner <PR>   (the holder asks: oldest other ready, waiting PR)
#        scripts/merge-tail-lock.sh merged <PR> <merge SHA>   (the holder merged it in a batch)
#        scripts/merge-tail-lock.sh release <PR>
#        scripts/merge-tail-lock.sh assert <PR>    (exit 1 unless <PR> holds the lock; run right before gh pr merge)
#
# Batching (Björn, 2026-10-15): the holder may stack one ready waiter on its own
# head, gate the pair once and merge both. `ready` records the head SHA a
# waiter's review settled on; `merged` tells that waiter its PR is in, and its
# `take` then exits 3 with the merge SHA instead of taking the lock.
#
# A waiter registers in the queue dir and touches its entry every poll. The lock
# goes to the lowest-numbered live waiter, since every merge makes the others
# rebase and an older branch has the most to rebase. An entry not touched for
# 120 s is a dead waiter and is ignored. `take` returns after WAIT seconds with
# exit 4 while still queued, so a waiter can wait in the foreground and call it
# again; its entry stays.
#
# The holder's claim (#1836): `$LOCK/alive` is touched by every lock call the
# holder makes and, under `hold`, every POLL seconds while the command runs. A
# claim older than LEASE is a dead holder, whatever its PR's state, and the
# next waiter takes the lock over; a live holder in a long gate keeps it by
# running the gate under `hold`. Before #1836 only a MERGED or CLOSED PR
# counted, so a holder that died with its PR open held the lock until a human
# noticed (2026-10-23, 20 min). A lock taken by that older script has no
# `alive` file: its claim is as old as its `holder` file.
set -u
POLL="${KNOBAS_MERGE_TAIL_POLL:-30}"     # seconds between polls, and between `hold`'s refreshes
LEASE="${KNOBAS_MERGE_TAIL_LEASE:-600}"  # seconds a holder may go without refreshing its claim
WAIT="${KNOBAS_MERGE_TAIL_WAIT:-100}"    # seconds a `take` waits before returning 4 (under the Bash tool's default timeout)
LOCK="${TMPDIR:-/tmp}/knobas-merge-tail.lock"
QUEUE="${TMPDIR:-/tmp}/knobas-merge-tail.queue"
READY="${TMPDIR:-/tmp}/knobas-merge-tail.ready"
cmd="${1:-}"; pr="${2:-}"; arg="${3:-}"
usage() { echo "usage: $0 ready|take|hold|partner|merged|assert|release <PR> [SHA | command...]"; exit 2; }
if [ -z "$cmd" ] || [ -z "$pr" ]; then usage; fi
oldest_live_waiter() {
  local f n best='' now; now=$(date +%s)
  for f in "$QUEUE"/*; do
    [ -e "$f" ] || continue
    n=${f##*/}
    [ $((now - $(mtime "$f"))) -le 120 ] || continue
    if [ -z "$best" ] || [ "$n" -lt "$best" ]; then best=$n; fi
  done
  echo "${best:-}"
}
# GNU stat first: on Linux `stat -f` is file-system status and exits 0. A file
# another waiter removed since the glob reads as epoch 0, i.e. long dead.
mtime() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null || echo 0; }
holder() { cat "$LOCK/holder" 2>/dev/null || true; }
# Seconds since the holder last refreshed its claim; `holder` stands in for a
# lock the pre-#1836 script took.
claim_age() { local f="$LOCK/alive"; [ -e "$f" ] || f="$LOCK/holder"; echo $(( $(date +%s) - $(mtime "$f") )); }
refresh() { [ "$(holder)" = "$pr" ] && touch "$LOCK/alive"; }
case "$cmd" in
ready)
  [ -n "$arg" ] || { echo "usage: $0 ready <PR> <head SHA>"; exit 2; }
  mkdir -p "$READY"; echo "$arg" > "$READY/$pr"
  echo "merge-tail: PR #$pr ready at $arg"
  ;;
partner)
  refresh
  now=$(date +%s); best=''
  for f in "$READY"/*; do
    [ -e "$f" ] || continue
    n=${f##*/}
    case $n in *.merged) continue ;; esac
    [ "$n" != "$pr" ] || continue
    [ ! -e "$READY/$n.merged" ] || continue
    [ -e "$QUEUE/$n" ] || continue
    [ $((now - $(mtime "$QUEUE/$n"))) -le 120 ] || continue
    if [ -z "$best" ] || [ "$n" -lt "$best" ]; then best=$n; fi
  done
  [ -n "$best" ] && echo "$best $(cat "$READY/$best")"
  ;;
merged)
  [ -n "$arg" ] || { echo "usage: $0 merged <PR> <merge SHA>"; exit 2; }
  refresh
  mkdir -p "$READY"; echo "$arg" > "$READY/$pr.merged"; rm -f "$QUEUE/$pr"
  echo "merge-tail: PR #$pr merged in a batch as $arg"
  ;;
take)
  mkdir -p "$QUEUE"; said=""; started=$SECONDS
  while :; do
    if [ -e "$READY/$pr.merged" ]; then
      rm -f "$QUEUE/$pr" "$READY/$pr"
      echo "merge-tail: PR #$pr was merged in a batch as $(cat "$READY/$pr.merged"); do the cleanup only"
      exit 3
    fi
    touch "$QUEUE/$pr"
    first=$(oldest_live_waiter)
    if [ "$first" = "$pr" ] && mkdir "$LOCK" 2>/dev/null; then break; fi
    holder=$(holder)
    if [ "$holder" = "$pr" ]; then
      rm -f "$QUEUE/$pr"; refresh; echo "merge-tail: already held by PR #$pr"; exit 0
    fi
    if [ -n "$holder" ] && [ "$(claim_age)" -gt "$LEASE" ]; then
      # One waiter at a time decides a takeover, on a read made under the
      # guard: another waiter may have taken over and acquired, or the holder
      # may have refreshed, since the read above. A guard older than 60 s was
      # left by a waiter killed mid-takeover and would block every takeover
      # for good.
      if [ -d "$LOCK.takeover" ] && [ $(( $(date +%s) - $(mtime "$LOCK.takeover") )) -gt 60 ]; then
        rmdir "$LOCK.takeover" 2>/dev/null
      fi
      if mkdir "$LOCK.takeover" 2>/dev/null; then
        age=$(claim_age)
        if [ "$(holder)" = "$holder" ] && [ "$age" -gt "$LEASE" ]; then
          echo "merge-tail: PR #$holder has not refreshed its claim for $age s (lease $LEASE s) and is dead; taking it over"
          rm -rf "$LOCK"
        fi
        rmdir "$LOCK.takeover"
        continue
      fi
    fi
    [ -z "$said" ] && { echo "merge-tail: waiting (holder #${holder:-none}, first in queue #${first:-?})"; said=1; }
    if [ $((SECONDS - started)) -ge "$WAIT" ]; then
      echo "merge-tail: PR #$pr still waiting after $((SECONDS - started)) s (holder #${holder:-none}, first in queue #${first:-?}); call take again"
      exit 4
    fi
    sleep "$POLL"
  done
  rm -f "$QUEUE/$pr"
  echo "$pr" > "$LOCK/holder"; touch "$LOCK/alive"
  echo "merge-tail: held by PR #$pr"
  ;;
hold)
  [ $# -ge 3 ] || { echo "usage: $0 hold <PR> <command...>"; exit 2; }
  shift 2
  if ! refresh; then
    echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder)); not running: $*"; exit 1
  fi
  ( while [ "$(holder)" = "$pr" ]; do touch "$LOCK/alive"; sleep "$POLL"; done ) &
  keeper=$!
  "$@"; status=$?
  kill "$keeper" 2>/dev/null; wait "$keeper" 2>/dev/null
  exit "$status"
  ;;
assert)
  if refresh; then echo "merge-tail: PR #$pr holds the lock"
  else echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder)); not merging"; exit 1; fi
  ;;
release)
  rm -f "$QUEUE/$pr" "$READY/$pr" "$READY/$pr.merged"
  holder=$(holder)
  if [ "$holder" = "$pr" ]; then rm -rf "$LOCK"; echo "merge-tail: released by PR #$pr"
  else echo "merge-tail: PR #$pr does not hold the lock (holder: ${holder:-none})"; fi
  ;;
*) usage ;;
esac
