#!/usr/bin/env bash
# GATE SLOTS -- the mutual exclusion every gate-sized recipe takes before it
# starts. Sourced, never run: it defines three functions and does nothing else,
# so the slot is held by the *recipe's* shell and released by that shell's own
# traps.
#
# WHAT SOURCES IT, which is the roster and lives only here: `just test` (and
# so `just check`) and `just search-perf`. One implementation rather than two
# that can drift apart -- a second, slightly different taker on the same
# directory is how a mutual exclusion stops being one.
#
# At most two gate-sized recipes run on one machine at a time (Björn,
# 2026-09-06, #425; the measurement is #422's), and one by default (below).
# The ceiling on two is SysV shared memory, not CPU: macOS allows 32
# segments machine-wide (`kern.sysv.shmmni`), every postmaster holds one, and one `just test` peaks at 13 -- the gate
# server plus up to 12 transient servers the `knobas-db` lifecycle suite
# starts. Two gates fit; three fit only when their peaks miss each other, and
# cost 2.4x the single wall each; five ran the kernel out and every worktree
# went red with `shmget: No space left on device`.
#
# ONE SLOT BY DEFAULT since #1892 (2026-10-23): the memory ceiling allows
# two, the cores do not. A gate's test run was most of the machine's load
# (Postgres backends in `create database` for every test, the test threads,
# and the file churn of creating and unlinking those databases), so two at
# once is what stalled the vitest half of whichever gate was beside them.
# Measured with two `just check` and one `just pre-gate` started 20 s apart in
# one warm worktree, the machine-wide sampler every 5 s, `KNOBAS_TEST_THREADS`
# per run (the `test` recipe says what that is):
#   2 slots, 12 threads (as before): load1 median 43.5, max 65.3; 2 vitest
#     timeouts (launcher-chain, fake-features), test runs 189-229 s
#   2 slots, 2 threads: median 24.0, max 50.7; launcher-chain timed out again;
#     test runs 276-292 s side by side
#   1 slot, 4 threads: median 18.5, max 28.7; no timeouts; test runs 134 and
#     154 s, the second after a 130 s wait for the slot; checks 170 and 293 s
#   1 slot, 2 threads: median 16.1, max 36.7; runs 157-178 s, the waiter's
#     check 340 s
# So one slot, four threads. The load stays above the twelve cores with two
# gates beside each other even so. #1897 then cut a gate's databases from
# ~930 to ~180 (the `justfile`'s `test` has the numbers); the same three
# side by side, with other agents' gates overlapping, still read a median of
# 16-23 either way. `KNOBAS_GATE_SLOTS=2` brings the old ceiling back.
#
# THE MERGE-TAIL HOLDER GOES FIRST (#1866). Since #1862 pre-gates run beside
# the merge-tail lock holder's re-gate, and every merge-manager queued in
# `take` waits on that re-gate. So a gate run under `merge-tail-lock.sh hold`,
# which exports KNOBAS_MERGE_TAIL_HOLDER=<PR> to its command, gets the next
# free slot ahead of every other taker: while it waits it keeps a link
# `holder-waiting-<pid>` -> pid beside the slots, and a taker without the
# variable takes no slot, not even a free one, while a live such link exists.
# It cannot cut into a run that already holds the slot, so the holder waits at
# most for the run in progress, not for the queue behind it. On 2026-10-23
# PR #1900's re-gate waited the full 150 s behind a bare `just test` and exited
# 75 after 239 s under the lock; its rerun found the slot free, and a pre-gate
# that started meanwhile waited about 95 s behind the holder. The link names
# its pid, so a holder SIGKILLed mid-wait is seen dead (`kill -0`) and its
# link removed by the next taker; one interrupted removes its own from the
# traps. A pre-gate kept back still waits at most `KNOBAS_GATE_SLOT_WAIT` and
# then exits 75 like any other. A taker running this file from before #1866
# (a worktree not yet rebased) does not look for the link; it can take the
# slot ahead of the holder, as every taker could before.
#
# So a recipe takes one of `KNOBAS_GATE_SLOTS` (default 1) slots before it
# does anything else, and with none free prints one line naming the holders' pids and polls, up to
# `KNOBAS_GATE_SLOT_WAIT` seconds (default 150), then exits 75 naming the same
# holders. 150 is what a full gate leaves under the Bash tool's 600 s foreground
# ceiling (383 s median, 504 s p90 over Oct 20-23, #1793): a wait that outlasts
# the tool's timeout is killed there, mid-queue or mid-run, and read as a red
# gate (D4, #1690). It is a fixed cap because the budget left of 600 s is not
# knowable here (the prefix before this point is the recipe's, not ours); the
# way past it is `KNOBAS_GATE_SLOT_WAIT=900` on a gate the caller runs in the
# background and waits for in the foreground, where no 600 s ceiling applies
# to the gate itself (#1836). 75 (EX_TEMPFAIL) is this exit's
# own code, so a wait that expired is told from a gate that failed (1 and up
# from the recipe's own steps) by the code alone; the line says "not a red
# gate" too. The slot is held from the recipe's start to after its server has
# stopped, and released by the same EXIT, INT and TERM traps that clean up the
# rest. For a bare `just test` on a cold tree that start is before the build,
# so a waiting gate's compile does not share the cores the running gate is
# already saturating; under `check` the build has happened in `test-build`
# by then, and what waits is the run.
#
# A slot is a symlink, `$TMPDIR/knobas-gate-slots/slot-<n>`, whose target is
# the holder's pid. `ln -s` is the atomic primitive: it fails on an existing
# name like `mkdir` does, and macOS ships no `flock` binary; unlike a
# directory with a pid file inside it, the link carries its pid from the
# instant it exists, so no taker can ever see a slot that is held by nobody.
# A slot whose pid is dead (`kill -0` fails) is a gate that was SIGKILLed or
# lost its machine, and the next taker reclaims it: unlinks the dead link and
# takes the slot with a fresh `ln -s`, never adopting it where it lies. The
# unlink happens under a lock, `mkdir "$slot_root/reclaim"`, because two
# takers can read the same dead pid, and without the lock the slower one
# would unlink whatever the faster one had just put there -- a live claim --
# and a third taker could then join the two already running. Under the lock
# the re-read cannot go stale: the holder is dead so it cannot release, no
# other reclaim runs, and a take needs the link to be absent, so what is
# unlinked is exactly the dead link that was read. A taker that finds the
# lock held counts the slot as held for this pass and looks again a second
# later; a lock older than a minute (`find -mmin +1`, BSD and GNU) is a
# reclaimer that died inside its few milliseconds, and is removed. An
# interrupted reclaimer removes its own lock from the traps, so what the
# sweep catches is a SIGKILLed one -- or one paused past the minute (a
# stopped process, a laptop asleep), whose lock is then removed while it is
# live, and a second sweeper between the first's `find` and `rmdir` removes a
# lock a fresh reclaimer has just taken. Those two windows are the residue:
# each needs a reclaim already in flight, and the worst case is one gate too
# many, which #422 saw three of run green. The root directory is recreated on
# every pass, so one removed by hand while a gate waits comes back. What the
# scheme cannot see: a dead holder's pid recycled onto some unrelated process
# (the slot then waits for that process; `rm` the link), and another user's
# gate, whose pid `kill -0` cannot signal and so reads as dead -- this is a
# one-user machine. The directory is not named `knobas-test-*`, whose prefix
# the connector's reaper sweeps, nor `knobas-gate.*`, so a count of those
# still counts gate directories. `hold_gate_slot`'s waiting line is
# `<recipe>: ...` with ONE space after the colon, never the two the `test`
# recipe's per-binary lines carry, so `check`'s sum cannot count it as a test
# binary. `check.yml` runs one gate on its runner and takes the first slot
# without waiting; a `cargo test` by hand is not a gate and takes none.
#
# Portability: plain bash 3.2, `ln -s`, `readlink`, `find -mmin` (BSD and
# GNU), `kill -0`. `check.yml` on `ubuntu-latest` sources the same file.

# Where the slots live, and this shell's state in the protocol. `slot` is the
# link this shell holds (empty when it holds none), `held` the pids the last
# failed pass found, `reclaim_lock` a lock this shell is inside right now.
# `tail_pr` is the merge-tail lock holder's PR when this gate runs under
# `hold` (not to be confused with a slot's holder, the pid in its link),
# `tail_mark` this shell's `holder-waiting-<pid>` link while it waits, and
# `tail_waiting` the pids of such waiting gates the last pass yielded to.
slot_root=${TMPDIR:-/tmp}/knobas-gate-slots
slot='' held='' reclaim_lock='' tail_pr='' tail_mark='' tail_waiting=''
# Set by `hold_gate_slot` from the environment before `take_slot` reads them;
# declared here so a `set -u` shell cannot meet them unset.
slots=1 slot_wait=150

# One pass over the slots: takes a free one (or one whose holder is dead) and
# returns 0 with `slot` set; else returns 1 with `held` naming the pids that
# hold them. A dead holder's link is unlinked under the reclaim lock and the
# slot taken afresh -- see above.
take_slot() {
    local i name holder lock=$slot_root/reclaim
    held='' tail_waiting=''
    mkdir -p "$slot_root"
    if [ -n "$tail_pr" ]; then
        # Every pass, so a mark removed by hand with the directory comes back.
        [ -L "$tail_mark" ] || ln -s "$$" "$tail_mark" 2>/dev/null || true
    else
        tail_waiters
        if [ -n "$tail_waiting" ]; then
            for ((i = 1; i <= slots; i++)); do
                holder=$(readlink "$slot_root/slot-$i" 2>/dev/null || true)
                if [ -n "$holder" ] && kill -0 "$holder" 2>/dev/null; then held="$held $holder"; fi
            done
            held=${held# }
            return 1
        fi
    fi
    for ((i = 1; i <= slots; i++)); do
        name=$slot_root/slot-$i
        if ln -s "$$" "$name" 2>/dev/null; then slot=$name; return 0; fi
        holder=$(readlink "$name" 2>/dev/null || true)
        if [ -n "$holder" ] && kill -0 "$holder" 2>/dev/null; then
            held="$held $holder"; continue
        fi
        if mkdir "$lock" 2>/dev/null; then
            reclaim_lock=$lock
            holder=$(readlink "$name" 2>/dev/null || true)
            if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
                rm -f "$name"
            fi
            rmdir "$lock" 2>/dev/null || true
            reclaim_lock=
        elif [ -n "$(find "$slot_root" -maxdepth 1 -name reclaim -mmin +1 2>/dev/null)" ]; then
            rmdir "$lock" 2>/dev/null || true
        fi
        if ln -s "$$" "$name" 2>/dev/null; then slot=$name; return 0; fi
        holder=$(readlink "$name" 2>/dev/null || true)
        held="$held ${holder:-?}"
    done
    held=${held# }
    return 1
}

# Sets `tail_waiting` to the pids of merge-tail holders' gates waiting for a
# slot right now (one, unless a holder that lost the lock still waits). A mark whose pid is dead is a holder SIGKILLed mid-wait
# (its traps did not run) and is removed: only that pid ever makes that name.
tail_waiters() {
    local m p
    tail_waiting=
    for m in "$slot_root"/holder-waiting-*; do
        [ -L "$m" ] || continue
        p=${m##*-}
        if kill -0 "$p" 2>/dev/null; then tail_waiting="$tail_waiting $p"; else rm -f "$m"; fi
    done
    tail_waiting=${tail_waiting# }
}

release_slot() {
    # A reclaim lock this shell still holds is an interrupt that landed
    # inside the reclaim's few milliseconds; the sweep must not be what
    # removes it.
    [ -z "$reclaim_lock" ] || rmdir "$reclaim_lock" 2>/dev/null || true
    reclaim_lock=
    # A holder interrupted while it waits takes its mark with it.
    [ -z "$tail_mark" ] || rm -f "$tail_mark"
    tail_mark=
    [ -n "$slot" ] || return 0
    # Only a link that still names this shell: a reclaimer removes a
    # slot only when its holder is dead, so this is belt and braces.
    [ "$(readlink "$slot" 2>/dev/null || true)" != "$$" ] || rm -f "$slot"
    slot=
}

# Block until this shell holds a slot, or exit 75 having waited the full
# `KNOBAS_GATE_SLOT_WAIT` (default 150 s). `$1` is the recipe's name, which is what the
# waiting and giving-up lines are prefixed with -- a reader of a stalled
# terminal has to be able to tell which recipe is queued. Call it with
# `release_slot` already armed in the EXIT, INT and TERM traps.
hold_gate_slot() {
    local name=$1 wait_from waited cause message told=
    slots=${KNOBAS_GATE_SLOTS:-1}
    case $slots in
        ''|*[!0-9]*) echo "error: KNOBAS_GATE_SLOTS must be a whole number of at least 1, not '$slots'" >&2; exit 1 ;;
    esac
    slots=$((10#$slots))
    if [ "$slots" -lt 1 ]; then
        echo "error: KNOBAS_GATE_SLOTS must be at least 1 (0 would let no gate ever start)" >&2; exit 1
    fi
    # Seconds; 0 is one try and no wait.
    slot_wait=${KNOBAS_GATE_SLOT_WAIT:-150}
    case $slot_wait in
        ''|*[!0-9]*) echo "error: KNOBAS_GATE_SLOT_WAIT must be a whole number of seconds, not '$slot_wait'" >&2; exit 1 ;;
    esac
    slot_wait=$((10#$slot_wait))
    # Set by `merge-tail-lock.sh hold` for the command it runs: this gate is
    # the merge-tail holder's, and the queue in `take` waits on it.
    tail_pr=${KNOBAS_MERGE_TAIL_HOLDER:-}
    [ -z "$tail_pr" ] || tail_mark=$slot_root/holder-waiting-$$
    wait_from=$SECONDS
    until take_slot; do
        waited=$((SECONDS - wait_from))
        if [ -n "$tail_waiting" ]; then
            cause=yield message="the merge-tail holder's gate (pid $tail_waiting) is waiting for a gate slot and goes first (slots held by pids ${held:-none})"
        else
            cause=held message="all $slots gate slots are held (pids $held)"
        fi
        # Said again when the cause changes: a pre-gate waiting on a run
        # that a holder then queues behind is told the holder goes first.
        if [ "$told" != "$cause" ]; then
            told=$cause
            if [ -n "$tail_pr" ]; then
                echo "$name: $message; waiting up to $slot_wait s for one to free, ahead of any pre-gate (merge-tail holder #$tail_pr)"
            else
                echo "$name: $message; waiting up to $slot_wait s for one to free"
            fi
        fi
        if [ "$waited" -ge "$slot_wait" ]; then
            echo "error: $message; waited $waited s for one to free, giving up (exit 75: the queue, not a red gate; run it again)" >&2
            exit 75
        fi
        sleep 1
    done
    [ -z "$tail_mark" ] || rm -f "$tail_mark"
    tail_mark=
    # Only after a pass that failed: `SECONDS` can tick over inside a first
    # pass that took the slot, and `check`'s summary prints this line.
    waited=$((SECONDS - wait_from))
    [ -z "$told" ] || echo "$name: gate slot free after $waited s"
}
