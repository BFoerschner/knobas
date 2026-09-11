#!/usr/bin/env bash
# GATE SLOTS -- the mutual exclusion every gate-sized recipe takes before it
# starts. Sourced, never run: it defines three functions and does nothing else,
# so the slot is held by the *recipe's* shell and released by that shell's own
# traps. `just test` and `just search-perf` both source it, so there is one
# implementation of the protocol rather than two that can drift apart -- a
# second, slightly different taker on the same directory is how a mutual
# exclusion stops being one.
#
# At most two gate-sized recipes run on one machine at a time (Björn,
# 2026-09-06, #425; the measurement is #422's). The ceiling is SysV shared
# memory, not CPU: macOS allows 32 segments machine-wide (`kern.sysv.shmmni`),
# every postmaster holds one, and one `just test` peaks at 13 -- the gate
# server plus up to 12 transient servers the `knobas-db` lifecycle suite
# starts. Two gates fit; three fit only when their peaks miss each other, and
# cost 2.4x the single wall each; five ran the kernel out and every worktree
# went red with `shmget: No space left on device`. So a recipe takes one of
# `KNOBAS_GATE_SLOTS` (default 2) slots before it does anything else, and with
# none free prints one line naming the holders' pids and polls, up to
# `KNOBAS_GATE_SLOT_WAIT` seconds (default 900), then exits 1 naming the same
# holders. The slot is held from the recipe's start to after its server has
# stopped, and released by the same EXIT, INT and TERM traps that clean up the
# rest. For a bare `just test` on a cold tree that start is before the build,
# so a waiting gate's compile does not share the cores the two running gates
# are already saturating; under `check` the build has happened in `inventory`
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
slot_root=${TMPDIR:-/tmp}/knobas-gate-slots
slot='' held='' reclaim_lock=''
# Set by `hold_gate_slot` from the environment before `take_slot` reads them;
# declared here so a `set -u` shell cannot meet them unset.
slots=2 slot_wait=900

# One pass over the slots: takes a free one (or one whose holder is dead) and
# returns 0 with `slot` set; else returns 1 with `held` naming the pids that
# hold them. A dead holder's link is unlinked under the reclaim lock and the
# slot taken afresh -- see above.
take_slot() {
    local i name holder lock=$slot_root/reclaim
    held=
    mkdir -p "$slot_root"
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

release_slot() {
    # A reclaim lock this shell still holds is an interrupt that landed
    # inside the reclaim's few milliseconds; the sweep must not be what
    # removes it.
    [ -z "$reclaim_lock" ] || rmdir "$reclaim_lock" 2>/dev/null || true
    reclaim_lock=
    [ -n "$slot" ] || return 0
    # Only a link that still names this shell: a reclaimer removes a
    # slot only when its holder is dead, so this is belt and braces.
    [ "$(readlink "$slot" 2>/dev/null || true)" != "$$" ] || rm -f "$slot"
    slot=
}

# Block until this shell holds a slot, or exit 1 having waited the full
# `KNOBAS_GATE_SLOT_WAIT`. `$1` is the recipe's name, which is what the
# waiting and giving-up lines are prefixed with -- a reader of a stalled
# terminal has to be able to tell which recipe is queued. Call it with
# `release_slot` already armed in the EXIT, INT and TERM traps.
hold_gate_slot() {
    local name=$1 wait_from waited announced=
    slots=${KNOBAS_GATE_SLOTS:-2}
    case $slots in
        ''|*[!0-9]*) echo "error: KNOBAS_GATE_SLOTS must be a whole number of at least 1, not '$slots'" >&2; exit 1 ;;
    esac
    slots=$((10#$slots))
    if [ "$slots" -lt 1 ]; then
        echo "error: KNOBAS_GATE_SLOTS must be at least 1 (0 would let no gate ever start)" >&2; exit 1
    fi
    # Seconds; 0 is one try and no wait.
    slot_wait=${KNOBAS_GATE_SLOT_WAIT:-900}
    case $slot_wait in
        ''|*[!0-9]*) echo "error: KNOBAS_GATE_SLOT_WAIT must be a whole number of seconds, not '$slot_wait'" >&2; exit 1 ;;
    esac
    slot_wait=$((10#$slot_wait))
    wait_from=$SECONDS
    until take_slot; do
        waited=$((SECONDS - wait_from))
        if [ -z "$announced" ]; then
            announced=1
            echo "$name: all $slots gate slots are held (pids $held); waiting up to $slot_wait s for one to free"
        fi
        if [ "$waited" -ge "$slot_wait" ]; then
            echo "error: all $slots gate slots are held (pids $held); waited $waited s for one to free, giving up" >&2
            exit 1
        fi
        sleep 1
    done
    waited=$((SECONDS - wait_from))
    [ "$waited" -eq 0 ] || echo "$name: gate slot free after $waited s"
}
