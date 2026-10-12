#!/usr/bin/env bash
# The real products the live suites run against, started on this machine on
# demand and stopped again (issue #633). Sourced by `testenv/live` and by the
# `just *-live` recipes; `live-test.sh` holds the decisions below to account.
#
# BJÖRN'S DECISION, 2026-10-12: "Since we focus on one source now at a time we
# don't have to run them 24/7, so we can just spin one up when we have to
# test." So TeamCity, Jira, Confluence and Gitea run in the notebook's Docker,
# one at a time, started when a live suite needs one and stopped after it.
# This supersedes the 2026-09-06 move of the three JVM products to Hetzner for
# live suites; those servers were decommissioned after it (#634). ADR-0013 is
# unchanged -- the witness is still the real product. Recorded in
# docs/decisions/2026-10-unattended-rulings.md.
#
# Four products, and what each brings up:
#
#   gitea       knobas-gitea                          seed-gitea.sh
#   teamcity    gitea, then knobas-teamcity and its   seed-teamcity.sh (which
#               agent -- the VCS roots clone Gitea    runs the builds seed too)
#   jira        knobas-jira-db, knobas-jira           seed-atlassian.sh jira, then
#                                                     its content, twice, verified
#   confluence  knobas-confluence-db, knobas-confluence   ... the same for Confluence
#
# ONE JVM PRODUCT AT A TIME. The Docker VM here has 8 GB; Jira wants ~4 GB,
# Confluence ~2 GB, TeamCity's server and agent ~2.2 GB, and two JVMs claiming
# their heaps at once is what killed #313's windows. `live_guard` refuses a
# second one in one line that names the one that is up. Gitea is not a JVM and
# is never refused.
#
# WHAT `down` MEANS DIFFERS, ON PURPOSE. TeamCity and Gitea are *stopped*:
# their volumes hold the seeded builds and the seeded corpus, and the next `up`
# finds them seeded in seconds. Jira and Confluence are removed with their
# volumes: they run on 3-hour timebomb licences that a restart does not renew,
# so a kept volume is a product whose licence has already run out. Their seed-state.json block goes
# with them, so `./seed --env` stops printing a URL nothing answers.
#
# NEVER A REMOTE ENGINE. Until #634 the docker contexts `knobas-teamcity`,
# `knobas-jira` and `knobas-confluence` reached three Hetzner servers over ssh.
# The servers are gone, and a context of that name left active on a machine
# would send these calls to an address that answers nothing -- or to whatever
# holds it next. `live_guard` refuses such a context before anything else.

LIVE_TESTENV=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# The containers whose presence means "this product is up".
_live_main() {
  case "$1" in
    gitea) echo knobas-gitea ;;
    teamcity) echo knobas-teamcity ;;
    jira) echo knobas-jira ;;
    confluence) echo knobas-confluence ;;
    *) return 1 ;;
  esac
}

# live_needs <product> -- the products a run of this one brings up, in the
# order it brings them up.
live_needs() {
  case "$1" in
    teamcity) echo "gitea teamcity" ;;
    *) echo "$1" ;;
  esac
}

# live_running <container>
live_running() {
  # `grep -x >/dev/null`, not `grep -q`: under pipefail an early-exiting grep
  # can kill `docker ps` with SIGPIPE and turn a match into a failure.
  docker ps --format '{{.Names}}' | grep -x -- "$1" >/dev/null
}

live_is_up() {
  live_running "$(_live_main "$1")"
}

# _live_guard_shell -- refuse a docker context that is not this machine's.
_live_guard_shell() {
  local context
  context=$(docker context show 2>/dev/null || true)
  case $context in
    knobas-*)
      echo "live: the docker context is '$context', left from the decommissioned Hetzner servers (#634); live products run locally -- docker context use orbstack" >&2
      return 1 ;;
  esac
}

# live_guard <product> -- refuse, in one line on stderr and before any docker
# call that changes anything, a product this machine must not start now.
live_guard() {
  local product=$1 other
  _live_main "$product" >/dev/null || {
    echo "live: unknown product '$product'; one of: gitea teamcity jira confluence" >&2
    return 1
  }
  _live_guard_shell || return 1
  [ "$product" != gitea ] || return 0
  for other in teamcity jira confluence; do
    [ "$other" != "$product" ] || continue
    if live_is_up "$other"; then
      echo "live: $(_live_main "$other") is running; one JVM product at a time on the 8 GB Docker VM -- 'just live-down $other' first" >&2
      return 1
    fi
  done
}

# live_is_jvm <product> -- TeamCity, Jira and Confluence; never Gitea.
live_is_jvm() {
  case "$1" in teamcity | jira | confluence) return 0 ;; *) return 1 ;; esac
}

# THE JVM LOCK (issue #690). `live_guard` refuses a second JVM product, which
# made the orchestrator hold a whole ticket -- 30-50 minutes of an agent -- in
# one JVM slot for the few minutes its live run used TeamCity. So every run
# that starts or uses a JVM product takes one machine-wide lock for the whole
# run, seeding and the closing `live_down` included, and a run that finds it
# held WAITS: several JVM tickets are implemented at once and only their live
# runs take turns. `live_guard` stays behind it as the backstop for a product
# somebody started by hand.
#
# The lock is a symlink, `$TMPDIR/knobas-live-jvm.lock` (outside every
# worktree; `KNOBAS_LIVE_LOCK` moves it, for the tests), whose target names
# the holder: `<pid> <epoch taken> <recipe> <worktree>`. `ln -s` is the atomic
# primitive, as in scripts/gate-slot.sh -- macOS ships no `flock`, and the
# link carries its holder from the instant it exists, so nobody can see a lock
# held by nobody. A lock whose pid is dead (`kill -0`) is a run that was
# SIGKILLed; the next taker removes it under `mkdir "$lock.reclaim"` -- two
# takers may read the same dead pid, and without the reclaim lock the slower
# one would unlink the faster one's fresh claim -- and says it took a stale
# lock over. The wait is `KNOBAS_LIVE_LOCK_WAIT` seconds (default 2700, 45
# minutes), polled every `KNOBAS_LIVE_LOCK_POLL` (default 2); past it the run
# refuses, naming the holder. What it cannot see, as for the gate slots: a
# dead holder's pid recycled onto an unrelated process (the run then waits for
# that one; `rm` the link).
#
# A run started by a holder -- `just atlassian-live` running `just jira-live`
# -- finds its parent's lock in `KNOBAS_LIVE_LOCK_HOLDER` and goes on under it
# without taking or releasing anything.
LIVE_LOCK=${KNOBAS_LIVE_LOCK:-${TMPDIR:-/tmp}/knobas-live-jvm.lock}
LIVE_LOCK_MINE=""

# _live_lock_describe <target> -- the holder, in words.
_live_lock_describe() {
  local pid since recipe tree
  read -r pid since recipe tree <<<"$1"
  echo "'$recipe' in ${tree:-?} (pid $pid, since $(date -r "$since" '+%H:%M:%S' 2>/dev/null || echo "$since"))"
}

_live_lock_pid_alive() {
  local pid=${1%% *}
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

# live_lock_hold <recipe> -- return once this shell holds the lock (or runs
# under a parent that does); return 1, naming the holder, after the wait.
# Arm `live_lock_release` in the EXIT, INT and TERM traps before calling it.
live_lock_hold() {
  local recipe=$1 me target waited announced='' last_report=0 wait_s poll from=$SECONDS
  # This (sub)shell's own pid: `$$` is the script's in a subshell, and bash
  # 3.2 has no BASHPID. The exec'd sh's parent is the shell that runs this.
  me=$(exec sh -c 'echo $PPID')
  wait_s=${KNOBAS_LIVE_LOCK_WAIT:-2700}
  poll=${KNOBAS_LIVE_LOCK_POLL:-2}
  case $wait_s in '' | *[!0-9]*)
    echo "live: KNOBAS_LIVE_LOCK_WAIT must be a whole number of seconds, not '$wait_s'" >&2; return 1 ;;
  esac
  target=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
  if [ -n "${KNOBAS_LIVE_LOCK_HOLDER:-}" ] && [ "$target" = "$KNOBAS_LIVE_LOCK_HOLDER" ] &&
    _live_lock_pid_alive "$target"; then
    echo "live: $recipe runs under the JVM lock its caller holds"
    return 0
  fi
  # One word for the recipe field, so the worktree path after it may hold spaces.
  LIVE_LOCK_MINE="$me $(date +%s) ${recipe// /_} ${LIVE_TESTENV%/testenv}"
  while :; do
    if ln -s "$LIVE_LOCK_MINE" "$LIVE_LOCK" 2>/dev/null; then
      export KNOBAS_LIVE_LOCK_HOLDER=$LIVE_LOCK_MINE
      waited=$((SECONDS - from))
      [ -z "$announced" ] || echo "live: $recipe: took the JVM lock after ${waited}s"
      return 0
    fi
    target=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
    if [ -n "$target" ] && ! _live_lock_pid_alive "$target"; then
      if mkdir "$LIVE_LOCK.reclaim" 2>/dev/null; then
        if [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$target" ]; then
          rm -f "$LIVE_LOCK"
          echo "live: $recipe: the JVM lock was stale -- its holder $(_live_lock_describe "$target") is gone; taking it over"
        fi
        rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
        continue
      elif [ -n "$(find "$LIVE_LOCK.reclaim" -maxdepth 0 -mmin +1 2>/dev/null)" ]; then
        # A reclaimer that died inside its few milliseconds.
        rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
      fi
    fi
    waited=$((SECONDS - from))
    if [ -n "$target" ] && [ "$target" != "$announced" ]; then
      announced=$target
      echo "live: $recipe: the JVM lock is held by $(_live_lock_describe "$target"); waiting up to ${wait_s}s"
    elif [ $((waited - last_report)) -ge 300 ]; then
      last_report=$waited
      echo "live: $recipe: still waiting for the JVM lock, ${waited}s so far"
    fi
    if [ "$waited" -ge "$wait_s" ]; then
      LIVE_LOCK_MINE=""
      echo "live: $recipe: the JVM lock is still held by $(_live_lock_describe "${target:-? ? ? ?}") after ${waited}s; giving up" >&2
      return 1
    fi
    sleep "$poll"
  done
}

# live_lock_release -- give the lock back, if this shell holds it.
live_lock_release() {
  [ -n "$LIVE_LOCK_MINE" ] || return 0
  [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" != "$LIVE_LOCK_MINE" ] || rm -f "$LIVE_LOCK"
  LIVE_LOCK_MINE=""
}

# live_to_put_back <product> -- the products a run of this one starts and must
# therefore stop again, last-started first: whatever was not already up.
live_to_put_back() {
  local p out=""
  for p in $(live_needs "$1"); do
    live_is_up "$p" || out="$p${out:+ $out}"
  done
  echo "$out"
}

_live_compose() {
  (cd "$LIVE_TESTENV" && docker compose "$@")
}

# The timebomb keys, fetched once per shell. Before either Atlassian `up`:
# Confluence reads its key at first start.
_live_licences() {
  if [ -z "${JIRA_LICENSE_KEY:-}" ] || [ -z "${CONFLUENCE_LICENSE_KEY:-}" ]; then
    eval "$("$LIVE_TESTENV/fetch-timebomb-keys.sh")"
    export JIRA_LICENSE_KEY CONFLUENCE_LICENSE_KEY
  fi
}

# _live_start <product> -- start one product (not its needs) and seed it.
# Every seed is idempotent, so starting a product that is already up is a
# re-check that writes nothing.
_live_start() {
  local product=$1
  echo "live: $product: up and seeded ..."
  case $product in
    gitea)
      _live_compose up -d --wait gitea
      (cd "$LIVE_TESTENV" && ./seed-gitea.sh) ;;
    teamcity)
      _live_compose --profile real-teamcity up -d teamcity teamcity-agent
      (cd "$LIVE_TESTENV" && ./seed-teamcity.sh) ;;
    jira | confluence)
      _live_licences
      _live_compose --profile real-atlassian up -d --wait "$product-db" "$product"
      (cd "$LIVE_TESTENV" && ./seed-atlassian.sh "$product")
      # Twice, deliberately: the second run reaches the find-then-skip branches
      # a seed from empty never does (#396), and must write nothing.
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product")
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product")
      (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$product" --verify) ;;
  esac
  echo "live: $product: up"
}

# live_up <product> -- the product and what it needs, guarded.
live_up() {
  local p
  live_guard "$1" || return 1
  for p in $(live_needs "$1"); do _live_start "$p"; done
}

# live_down <product> -- this product only; what it needed stays as it is.
live_down() {
  # The context guard first: through a remote context this `rm` and `volume
  # rm` would land somewhere else. Its one-JVM half cannot refuse a product
  # that is itself the one up, and `down` of another is harmless.
  _live_guard_shell || return 1
  _live_main "$1" >/dev/null || { live_guard "$1"; return 1; }
  case $1 in
    gitea) _live_compose stop gitea ;;
    teamcity) _live_compose --profile real-teamcity stop teamcity teamcity-agent ;;
    jira | confluence)
      # `rm` and the two named volumes, NOT `down -v`. A `down` with services
      # named still removes the project's network once no RUNNING container
      # is on it -- and a stopped Gitea, TeamCity or Kuma is still attached
      # to it by id, so the next `up` of any of them fails with "network ...
      # not found" (seen on this branch's first runs, #633). Named volumes,
      # because `rm -v` removes only anonymous ones.
      _live_compose --profile real-atlassian rm --stop --force "$1" "$1-db"
      local v
      for v in "knobas-testenv_$1-data" "knobas-testenv_$1-db-data"; do
        if docker volume inspect "$v" >/dev/null 2>&1; then docker volume rm "$v" >/dev/null; fi
      done
      local state=$LIVE_TESTENV/seed-state.json
      if [ -f "$state" ]; then
        jq --arg k "$1" 'del(.[$k])' "$state" >"$state.tmp" && mv "$state.tmp" "$state"
      fi ;;
  esac
  echo "live: $1: down"
}

# Commands a recipe registers to run before the products are put back, last
# registered first -- `gitea-live-capped`'s uncap is one.
LIVE_ON_EXIT=()
live_on_exit() { LIVE_ON_EXIT=("$1" ${LIVE_ON_EXIT[@]+"${LIVE_ON_EXIT[@]}"}); }

LIVE_PUT_BACK=""
_live_restore() {
  # Taken and cleared before either loop runs: a Ctrl-C during the EXIT trap's
  # restore runs the INT trap, which must find nothing left to do twice.
  local hook p hooks=(${LIVE_ON_EXIT[@]+"${LIVE_ON_EXIT[@]}"}) put_back=$LIVE_PUT_BACK
  LIVE_ON_EXIT=()
  LIVE_PUT_BACK=""
  for hook in ${hooks[@]+"${hooks[@]}"}; do eval "$hook" || true; done
  for p in $put_back; do live_down "$p" || true; done
}

# live_session <product> [<recipe>] -- for a recipe: for a JVM product, hold
# the JVM lock (as <recipe>) for the whole run; bring the product up, export what
# `./seed --env` prints, and arm EXIT, INT and TERM to put the machine back as
# this found it. A product that was already up -- `just live-up jira` before a
# run of `just jira-live` -- is left up; one this started is stopped.
#
# INT and TERM as well as EXIT, the `gitea-live-capped` rule: bash runs no EXIT
# trap for a signal it is not trapping, and a Ctrl-C'd run is the one that
# leaves a JVM product holding the VM. Each clears EXIT, restores once and
# re-raises. Armed before the first `up`, so an interrupt mid-seed stops what
# had been started.
live_session() {
  # The traps first, holding nothing yet: an interrupt while waiting for the
  # lock, or a refusal from the guard behind it, gives the lock back.
  trap '_live_restore; live_lock_release' EXIT
  trap 'trap - EXIT INT; _live_restore; live_lock_release; kill -INT $$' INT
  trap 'trap - EXIT TERM; _live_restore; live_lock_release; kill -TERM $$' TERM
  _live_main "$1" >/dev/null || { live_guard "$1"; exit 1; }
  # The lock BEFORE the guard: a run queued behind another JVM run must wait
  # for it, not meet the refusal that run's product causes.
  if live_is_jvm "$1"; then live_lock_hold "${2:-live_session-$1}" || exit 1; fi
  live_guard "$1" || exit 1
  LIVE_PUT_BACK=$(live_to_put_back "$1")
  live_up "$1"
  eval "$(cd "$LIVE_TESTENV" && ./seed --env)"
  if [ -n "$LIVE_PUT_BACK" ]; then
    echo "live: $1 is up for this run; $LIVE_PUT_BACK will be stopped at its end"
  else
    echo "live: $1 was already up and stays up"
  fi
}
