#!/usr/bin/env bash
# Merge-tail lock, shared by every machine that merges (#2335): reviews and
# pre-gates run in parallel; only the freshness check, a re-gate when main
# moved, and the merges run one at a time, oldest PR first on each machine.
#
# Usage: scripts/merge-tail-lock.sh ready <PR> <head SHA> [<main SHA>]   (review done, and with a main: gated green on that head over that main)
#        scripts/merge-tail-lock.sh take <PR>      (exit 0 = held; 4 = still queued, call again)
#        scripts/merge-tail-lock.sh gated <PR>     (the holder asks, in its worktree: exit 0 = merge without gating again; 1 = gate under the lock)
#        scripts/merge-tail-lock.sh hold <PR> <command...>   (the holder runs a long command; its claim stays fresh meanwhile, and a gate in it goes first on the gate slots)
#        scripts/merge-tail-lock.sh release <PR>
#        scripts/merge-tail-lock.sh assert <PR>    (exit 1 unless <PR> holds the lock; run right before gh pr merge)
#
# The holder slot is a ref on origin (#2335): `refs/locks/merge-tail`. Before
# #2335 it was a directory in `$TMPDIR`, so the Mac and the Linux host each had
# their own lock and could both merge at once; a merge from one machine could
# land between the other's `gated` and its `gh pr merge`, onto a main no gate
# saw (`--match-head-commit` pins the head, not the base). Now the slot holds a
# token commit (empty tree, no parent) whose message says
#     holder: <PR>
#     machine: <host>
#     expires: <epoch seconds>
# and every change to it is a compare-and-swap: `git push --force-with-lease
# =<ref>:<the token just read>` (empty = must not exist), which origin applies
# only if nobody moved the ref since. Never with a `+` on the refspec: that
# overrides the lease (checked against GitHub, 2026-10-31). So of two machines
# racing for a free slot, or for a dead holder's, exactly one push lands; the
# other re-reads and waits. Since every merge on every machine happens under
# the one slot, nobody can merge between a holder's `gated` and its merge:
# the shared slot is what closes that window, not `--match-head-commit`.
#
# What stays per machine: the waiter queue and the pre-gate records, under
# `$TMPDIR`. Only each machine's lowest-numbered live waiter competes for the
# slot; across machines it goes to whoever's push lands first.
#
# The lock talks to origin through the repo the script sits in (the root
# checkout's copy: the root checkout), or KNOBAS_MERGE_TAIL_REPO. An origin it
# cannot read never holds: `take` stays queued and `assert` exits 1.
#
# No batching (Björn, 2026-10-29): a holder merges its own PR alone. It gates
# before the lock, `gated` says whether that pre-gate is still fresh, and if
# not it rebases and gates once more under `hold`, still alone.
#
# Pre-gate (#1862): a merge-manager gates its rebased head before `take` and
# records it with `ready <PR> <head> <main>`, in `$READY/<PR>.main`. Under the
# lock, `gated` exits 0 only when origin's main (read with `git ls-remote`,
# never the worktree's own possibly unfetched `origin/main`) is still that
# main, the worktree's HEAD is that head, and the head sits on that main: then
# the squash merge produces exactly the tree the pre-gate saw. Anything else,
# including no record and an unreadable origin, exits 1: gate under the lock.
#
# A waiter registers in the queue dir and touches its entry every poll. The
# slot goes to the lowest-numbered live waiter, since every merge makes the
# others rebase and an older branch has the most to rebase. An entry not
# touched for 120 s is a dead waiter and is ignored. `take` returns after WAIT
# seconds with exit 4 while still queued, so a waiter can wait in the
# foreground and call it again; its entry stays.
#
# The holder's claim (#1836) is a lease: every lock call the holder makes
# (`assert`, `gated`, a re-entrant `take`) and, under `hold`, every POLL
# seconds while the command runs, swaps in a token with a new expiry, LEASE
# seconds on. A token past its expiry is a dead holder, whatever its PR's
# state, and the next waiter takes it over with the same compare-and-swap; the
# old holder's next `assert` then exits 1. Expiry is wall-clock time written
# by one machine and read by another: clock skew between them shortens or
# lengthens a lease by the skew, seconds against 600.
#
# The holder's gate first (#1866): `hold` runs its command with
# KNOBAS_MERGE_TAIL_HOLDER=<PR>, and `scripts/gate-slot.sh` gives a gate that
# carries it the next free gate slot ahead of every pre-gate, since the whole
# queue in `take` waits on the holder's re-gate.
set -u
POLL="${KNOBAS_MERGE_TAIL_POLL:-30}"     # seconds between polls, and between `hold`'s refreshes
LEASE="${KNOBAS_MERGE_TAIL_LEASE:-600}"  # seconds a holder may go without refreshing its claim
WAIT="${KNOBAS_MERGE_TAIL_WAIT:-100}"    # seconds a `take` waits before returning 4 (under the Bash tool's default timeout)
QUEUE="${TMPDIR:-/tmp}/knobas-merge-tail.queue"
READY="${TMPDIR:-/tmp}/knobas-merge-tail.ready"
REPO="${KNOBAS_MERGE_TAIL_REPO:-$(cd "$(dirname "$0")" && pwd)}"
REMOTE="${KNOBAS_MERGE_TAIL_REMOTE:-origin}"
REF="${KNOBAS_MERGE_TAIL_REF:-refs/locks/merge-tail}"
MACHINE="${KNOBAS_MERGE_TAIL_MACHINE:-$(hostname -s 2>/dev/null || hostname)}"
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
lg() { GIT_TERMINAL_PROMPT=0 git -C "$REPO" "$@"; }
# read_slot: reads the slot into SLOT (the token's SHA, empty when free) and
# S_HOLDER, S_MACHINE, S_EXPIRES. Returns 1 when origin cannot be read. A
# token without an expiry reads as long expired.
read_slot() {
  local out msg
  SLOT='' S_HOLDER='' S_MACHINE='' S_EXPIRES=0
  for _ in 1 2 3; do
    out=$(lg ls-remote "$REMOTE" "$REF" 2>/dev/null) || return 1
    SLOT=$(printf '%s\n' "$out" | awk -v r="$REF" '$2 == r { print $1 }')
    [ -n "$SLOT" ] || return 0
    lg cat-file -e "$SLOT^{commit}" 2>/dev/null || lg fetch -q --no-write-fetch-head "$REMOTE" "$REF" 2>/dev/null || return 1
    # Moved again between the read and the fetch: read once more.
    msg=$(lg cat-file commit "$SLOT" 2>/dev/null) || continue
    S_HOLDER=$(printf '%s\n' "$msg" | sed -n 's/^holder: //p')
    S_MACHINE=$(printf '%s\n' "$msg" | sed -n 's/^machine: //p')
    S_EXPIRES=$(printf '%s\n' "$msg" | sed -n 's/^expires: \([0-9][0-9]*\)$/\1/p')
    S_EXPIRES=${S_EXPIRES:-0}
    return 0
  done
  return 1
}
# token: a new token commit naming this PR, expiring LEASE seconds from now.
token() {
  local tree
  tree=$(lg hash-object -t tree -w /dev/null) || return 1
  printf 'merge-tail lock\n\nholder: %s\nmachine: %s\nexpires: %s\nnonce: %s\n' \
    "$pr" "$MACHINE" "$(( $(date +%s) + LEASE ))" "$$-$RANDOM" |
    lg -c user.name=merge-tail -c user.email=merge-tail@knobas.invalid commit-tree --no-gpg-sign "$tree"
}
# cas <expected SHA, empty = free> <new SHA, empty = free it>: one leased push.
cas() {
  if [ -n "$2" ]; then lg push -q --no-verify "$REMOTE" "$2:$REF" "--force-with-lease=$REF:$1" 2>/dev/null
  else lg push -q --no-verify "$REMOTE" ":$REF" "--force-with-lease=$REF:$1" 2>/dev/null; fi
}
# refresh: 0 = this PR holds the slot and its lease starts over; 1 = it does
# not hold it; 2 = undecided (origin unreadable, or pushes keep failing).
refresh() {
  local new
  for _ in 1 2 3; do
    read_slot || return 2
    { [ -n "$SLOT" ] && [ "$S_HOLDER" = "$pr" ]; } || return 1
    new=$(token) || return 2
    cas "$SLOT" "$new" && return 0
  done
  return 2
}
holder_said() { if [ -n "${SLOT:-}" ]; then echo "#${S_HOLDER:-?} on ${S_MACHINE:-?}"; else echo none; fi; }
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
  refresh; held=$?
  if [ "$held" -ne 0 ]; then
    if [ "$held" -eq 2 ]; then echo "merge-tail: cannot read the lock on $REMOTE; PR #$pr does NOT hold the lock as far as anyone can tell"
    else echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder_said)); gate under the lock once you hold it"; fi
    exit 1
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
    if read_slot; then
      holder=$(holder_said)
      if [ -n "$SLOT" ] && [ "$S_HOLDER" = "$pr" ]; then
        if refresh; then rm -f "$QUEUE/$pr"; echo "merge-tail: already held by PR #$pr"; exit 0; fi
      elif [ "$first" = "$pr" ]; then
        # Only this machine's first waiter competes; the leased push decides
        # between machines, and a lost push just means another poll.
        if [ -z "$SLOT" ]; then
          new=$(token) && cas "" "$new" && break
        elif [ "$(date +%s)" -gt "$S_EXPIRES" ]; then
          dead=$holder; late=$(( $(date +%s) - S_EXPIRES ))
          if new=$(token) && cas "$SLOT" "$new"; then
            echo "merge-tail: PR $dead let its lease run out $late s ago (lease $LEASE s) and is dead; taking it over"
            break
          fi
        fi
      fi
    else
      holder="unknown: cannot read $REF on $REMOTE"
    fi
    [ -z "$said" ] && { echo "merge-tail: waiting (holder $holder, first in this machine's queue #${first:-?})"; said=1; }
    if [ $((SECONDS - started)) -ge "$WAIT" ]; then
      echo "merge-tail: PR #$pr still waiting after $((SECONDS - started)) s (holder $holder, first in this machine's queue #${first:-?}); call take again"
      exit 4
    fi
    sleep "$POLL"
  done
  rm -f "$QUEUE/$pr"
  echo "merge-tail: held by PR #$pr (on $MACHINE)"
  ;;
hold)
  [ $# -ge 3 ] || { echo "usage: $0 hold <PR> <command...>"; exit 2; }
  shift 2
  if ! refresh; then
    echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder_said)); not running: $*"; exit 1
  fi
  # Refresh until the slot is definitely someone else's; an origin that cannot
  # be read for a poll or two is not a lost lock.
  ( while :; do sleep "$POLL"; refresh; [ $? -eq 1 ] && break; done ) &
  keeper=$!
  KNOBAS_MERGE_TAIL_HOLDER=$pr "$@"; status=$?
  kill "$keeper" 2>/dev/null; wait "$keeper" 2>/dev/null
  exit "$status"
  ;;
assert)
  refresh; held=$?
  if [ "$held" -eq 0 ]; then echo "merge-tail: PR #$pr holds the lock"
  elif [ "$held" -eq 2 ]; then echo "merge-tail: cannot read the lock on $REMOTE; not merging"; exit 1
  else echo "merge-tail: PR #$pr does NOT hold the lock (holder: $(holder_said)); not merging"; exit 1; fi
  ;;
release)
  rm -f "$QUEUE/$pr" "$READY/$pr" "$READY/$pr.main"
  for _ in 1 2 3; do
    if ! read_slot; then
      echo "merge-tail: cannot read the lock on $REMOTE; a slot PR #$pr still holds frees itself when its lease runs out"; exit 0
    fi
    if [ -z "$SLOT" ] || [ "$S_HOLDER" != "$pr" ]; then
      echo "merge-tail: PR #$pr does not hold the lock (holder: $(holder_said))"; exit 0
    fi
    if cas "$SLOT" ""; then echo "merge-tail: released by PR #$pr"; exit 0; fi
  done
  echo "merge-tail: PR #$pr could not free the slot; it frees itself when its lease runs out"
  ;;
*) usage ;;
esac
