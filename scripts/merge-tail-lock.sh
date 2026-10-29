#!/usr/bin/env bash
# Machine-wide merge-tail lock: reviews and pre-gates run in parallel; only the
# freshness check, a re-gate when main moved, and the merges run one at a
# time, oldest PR first.
#
# Usage: scripts/merge-tail-lock.sh ready <PR> <head SHA> [<main SHA>]   (review done, and with a main: gated green on that head over that main)
#        scripts/merge-tail-lock.sh take <PR>      (exit 0 = held; 4 = still queued, call again)
#        scripts/merge-tail-lock.sh gated <PR>     (the holder asks, in its worktree: exit 0 = merge without gating again; 1 = gate under the lock)
#        scripts/merge-tail-lock.sh hold <PR> <command...>   (the holder runs a long command; its claim stays fresh meanwhile, and a gate in it goes first on the gate slots)
#        scripts/merge-tail-lock.sh release <PR>
#        scripts/merge-tail-lock.sh assert <PR>    (exit 1 unless <PR> holds the lock; run right before gh pr merge)
#
# No batching (Björn, 2026-10-29): a holder merges its own PR alone. It gates
# before the lock, `gated` says whether that pre-gate is still fresh, and if
# not it rebases and gates once more under `hold`, still alone. The `partner`
# and `merged` subcommands that stacked waiters onto a holder's head went with
# batching.
#
# Pre-gate (#1862): a merge-manager gates its rebased head before `take` and
# records it with `ready <PR> <head> <main>`, in `$READY/<PR>.main`. Under the
# lock, `gated` exits 0 only when origin's main (read with `git ls-remote`,
# never the worktree's own possibly unfetched `origin/main`) is still that
# main, the worktree's HEAD is that head, and the head sits on that main: then
# the squash merge produces exactly the tree the pre-gate saw. Anything else,
# including no record and an unreadable origin, exits 1: gate under the lock.
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
#
# The holder's gate first (#1866): `hold` runs its command with
# KNOBAS_MERGE_TAIL_HOLDER=<PR>, and `scripts/gate-slot.sh` gives a gate that
# carries it the next free gate slot ahead of every pre-gate, since the whole
# queue in `take` waits on the holder's re-gate.
set -u
POLL="${KNOBAS_MERGE_TAIL_POLL:-30}"     # seconds between polls, and between `hold`'s refreshes
LEASE="${KNOBAS_MERGE_TAIL_LEASE:-600}"  # seconds a holder may go without refreshing its claim
WAIT="${KNOBAS_MERGE_TAIL_WAIT:-100}"    # seconds a `take` waits before returning 4 (under the Bash tool's default timeout)
LOCK="${TMPDIR:-/tmp}/knobas-merge-tail.lock"
QUEUE="${TMPDIR:-/tmp}/knobas-merge-tail.queue"
READY="${TMPDIR:-/tmp}/knobas-merge-tail.ready"
cmd="${1:-}"; pr="${2:-}"; arg="${3:-}"
usage() { echo "usage: $0 ready|take|gated|hold|assert|release <PR> [SHA [SHA] | command...]"; exit 2; }
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
  [ -n "$arg" ] || { echo "usage: $0 ready <PR> <head SHA> [<main SHA the gate ran over>]"; exit 2; }
  mkdir -p "$READY"; echo "$arg" > "$READY/$pr"
  # A `ready` without a main (the brief before #1862, or a head nobody
  # pre-gated) drops any older record: what it says is no longer true.
  if [ -n "${4:-}" ]; then
    echo "$4" > "$READY/$pr.main"
    echo "merge-tail: PR #$pr ready at $arg, gated over main $4"
  else
    rm -f "$READY/$pr.main"
    echo "merge-tail: PR #$pr ready at $arg"
  fi
  ;;
gated)
  if ! refresh; then
    echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder)); gate under the lock once you hold it"; exit 1
  fi
  gated_head=$(cat "$READY/$pr" 2>/dev/null); gated_main=$(cat "$READY/$pr.main" 2>/dev/null)
  if [ -z "$gated_head" ] || [ -z "$gated_main" ]; then
    echo "merge-tail: PR #$pr has no pre-gate recorded (\`ready <PR> <head> <main>\`); gate under the lock"; exit 1
  fi
  now_head=$(git rev-parse HEAD 2>/dev/null)
  if [ "$now_head" != "$gated_head" ]; then
    echo "merge-tail: PR #$pr's head is ${now_head:-unreadable}, the pre-gate ran on $gated_head; gate under the lock"; exit 1
  fi
  now_main=$(GIT_TERMINAL_PROMPT=0 git ls-remote origin refs/heads/main 2>/dev/null | cut -f1)
  if [ -z "$now_main" ]; then
    echo "merge-tail: cannot read origin's main; gate under the lock"; exit 1
  fi
  if [ "$now_main" != "$gated_main" ]; then
    echo "merge-tail: main moved from $gated_main (the pre-gate's) to $now_main; rebase and gate under the lock"; exit 1
  fi
  if ! git merge-base --is-ancestor "$gated_main" "$gated_head" 2>/dev/null; then
    echo "merge-tail: PR #$pr's head $gated_head does not sit on main $gated_main; rebase and gate under the lock"; exit 1
  fi
  echo "merge-tail: PR #$pr was gated at $gated_head over main $gated_main, and main has not moved: merge without gating again"
  ;;
take)
  mkdir -p "$QUEUE"; said=""; started=$SECONDS
  while :; do
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
  KNOBAS_MERGE_TAIL_HOLDER=$pr "$@"; status=$?
  kill "$keeper" 2>/dev/null; wait "$keeper" 2>/dev/null
  exit "$status"
  ;;
assert)
  if refresh; then echo "merge-tail: PR #$pr holds the lock"
  else echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder)); not merging"; exit 1; fi
  ;;
release)
  rm -f "$QUEUE/$pr" "$READY/$pr" "$READY/$pr.main"
  holder=$(holder)
  if [ "$holder" = "$pr" ]; then rm -rf "$LOCK"; echo "merge-tail: released by PR #$pr"
  else echo "merge-tail: PR #$pr does not hold the lock (holder: ${holder:-none})"; fi
  ;;
*) usage ;;
esac
