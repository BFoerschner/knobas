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
    forgejo) echo knobas-forgejo ;;
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
    echo "live: unknown product '$product'; one of: gitea forgejo teamcity jira confluence" >&2
    return 1
  }
  _live_guard_shell || return 1
  live_is_jvm "$product" || return 0
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
#
# THE GITEA LOCK (issue #724) is the same mechanism under another name. Two
# `just gitea-live` runs from two worktrees broke each other: one run's
# cleanup deleted its branch while the other's full sync walked the repo, and
# the suites that sweep the whole fixture assert on counts another run is
# changing. So every run that writes to the shared Gitea or reconfigures it --
# `gitea-live`, `gitea-live-capped`, `start-work-live`, all through
# `live_session gitea` -- holds `$TMPDIR/knobas-live-gitea.lock`
# (`KNOBAS_LIVE_GITEA_LOCK` moves it; the holder is passed down in
# `KNOBAS_LIVE_GITEA_LOCK_HOLDER`). A separate lock, not the JVM one: a Gitea
# run does not need TeamCity's memory, and queueing a two-minute Gitea suite
# behind a 20-minute Atlassian run would serialise what never collided. A
# TeamCity run starts and seeds Gitea without it; the seed writes nothing to a
# seeded Gitea. One shell holds at most one of the two.
LIVE_LOCK_MINE=""
# Set while this shell is inside a reclaim, so an interrupt there removes the
# reclaim lock itself instead of leaving it to the minute-old sweep.
LIVE_LOCK_RECLAIMING=""

# _live_lock_describe <target> -- the holder, in words.
_live_lock_describe() {
  local pid since recipe tree hint="" p
  read -r pid since recipe tree <<<"$1"
  case $recipe in live-up-*-kept)
    p=${recipe#live-up-}
    hint=", kept for a live-up product: 'just live-down ${p%-kept}' from any worktree frees it" ;;
  esac
  echo "'$recipe' in ${tree:-?} (pid $pid, since $(date -r "$since" '+%H:%M:%S' 2>/dev/null || echo "$since")$hint)"
}

_live_lock_pid_alive() {
  local pid=${1%% *}
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

# _live_lock_select <jvm|gitea> -- point LIVE_LOCK and its label and holder
# variable at one of the two locks.
_live_lock_select() {
  case $1 in
    jvm)
      LIVE_LOCK=${KNOBAS_LIVE_LOCK:-${TMPDIR:-/tmp}/knobas-live-jvm.lock}
      LIVE_LOCK_LABEL="JVM lock"
      LIVE_LOCK_HOLDER_VAR=KNOBAS_LIVE_LOCK_HOLDER ;;
    gitea)
      LIVE_LOCK=${KNOBAS_LIVE_GITEA_LOCK:-${TMPDIR:-/tmp}/knobas-live-gitea.lock}
      LIVE_LOCK_LABEL="Gitea lock"
      LIVE_LOCK_HOLDER_VAR=KNOBAS_LIVE_GITEA_LOCK_HOLDER ;;
    *) echo "live: no lock named '$1' (jvm or gitea)" >&2; return 1 ;;
  esac
}
_live_lock_select jvm

# live_lock_hold <recipe> [<jvm|gitea>] -- return once this shell holds the
# lock (the JVM one unless named; or runs under a parent that does); return 1,
# naming the holder, after the wait. Arm `live_lock_release` in the EXIT, INT
# and TERM traps before calling it.
live_lock_hold() {
  local recipe=$1 me target waited announced='' last_report=0 wait_s poll from=$SECONDS
  if [ -n "$LIVE_LOCK_MINE" ]; then
    echo "live: $recipe: this shell already holds the $LIVE_LOCK_LABEL; one lock per shell" >&2
    return 1
  fi
  _live_lock_select "${2:-jvm}" || return 1
  # This (sub)shell's own pid: `$$` is the script's in a subshell, and bash
  # 3.2 has no BASHPID. The exec'd sh's parent is the shell that runs this.
  me=$(exec sh -c 'echo $PPID')
  wait_s=${KNOBAS_LIVE_LOCK_WAIT:-2700}
  poll=${KNOBAS_LIVE_LOCK_POLL:-2}
  case $wait_s in '' | *[!0-9]*)
    echo "live: KNOBAS_LIVE_LOCK_WAIT must be a whole number of seconds, not '$wait_s'" >&2; return 1 ;;
  esac
  case $poll in '' | . | *[!0-9.]* | *.*.*)
    echo "live: KNOBAS_LIVE_LOCK_POLL must be a number of seconds, not '$poll'" >&2; return 1 ;;
  esac
  target=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
  if [ -n "${!LIVE_LOCK_HOLDER_VAR:-}" ] && [ "$target" = "${!LIVE_LOCK_HOLDER_VAR}" ] &&
    _live_lock_pid_alive "$target"; then
    echo "live: $recipe runs under the $LIVE_LOCK_LABEL its caller holds"
    return 0
  fi
  if _live_lock_kept_here "$target"; then
    echo "live: $recipe runs under the $LIVE_LOCK_LABEL this worktree's live-up keeps"
    return 0
  fi
  while :; do
    # Something at the path that is not a link is no holder: `ln -s` would
    # fail on a file forever, with no target to wait on or time out against,
    # and would create its link INSIDE a directory and report success.
    if [ -e "$LIVE_LOCK" ] && [ ! -L "$LIVE_LOCK" ]; then
      LIVE_LOCK_MINE=""
      echo "live: $recipe: $LIVE_LOCK is not the $LIVE_LOCK_LABEL's symlink; remove it" >&2
      return 1
    fi
    # Stamped at each try, so the epoch says when the lock was TAKEN -- a run
    # that waited does not claim to have held it since it started waiting.
    # One word for the recipe field, so the worktree path after it may hold spaces.
    LIVE_LOCK_MINE="$me $(date +%s) ${recipe// /_} ${LIVE_TESTENV%/testenv}"
    if ln -s "$LIVE_LOCK_MINE" "$LIVE_LOCK" 2>/dev/null; then
      export "$LIVE_LOCK_HOLDER_VAR=$LIVE_LOCK_MINE"
      waited=$((SECONDS - from))
      [ -z "$announced" ] || echo "live: $recipe: took the $LIVE_LOCK_LABEL after ${waited}s"
      return 0
    fi
    target=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
    # Released between the `ln -s` and the `readlink`: free, so try again now.
    [ -n "$target" ] || continue
    if ! _live_lock_pid_alive "$target"; then
      if mkdir "$LIVE_LOCK.reclaim" 2>/dev/null; then
        LIVE_LOCK_RECLAIMING=1
        if [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$target" ]; then
          rm -f "$LIVE_LOCK"
          echo "live: $recipe: the $LIVE_LOCK_LABEL was stale -- its holder $(_live_lock_describe "$target") is gone; taking it over"
        fi
        rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
        LIVE_LOCK_RECLAIMING=""
        continue
      elif [ -n "$(find "$LIVE_LOCK.reclaim" -maxdepth 0 -mmin +1 2>/dev/null)" ]; then
        # A reclaimer that died inside its few milliseconds.
        rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
      fi
    fi
    waited=$((SECONDS - from))
    if [ "$target" != "$announced" ]; then
      announced=$target
      echo "live: $recipe: the $LIVE_LOCK_LABEL is held by $(_live_lock_describe "$target"); waiting up to ${wait_s}s"
    elif [ $((waited - last_report)) -ge 300 ]; then
      last_report=$waited
      echo "live: $recipe: still waiting for the $LIVE_LOCK_LABEL, ${waited}s so far"
    fi
    if [ "$waited" -ge "$wait_s" ]; then
      LIVE_LOCK_MINE=""
      echo "live: $recipe: the $LIVE_LOCK_LABEL is still held by $(_live_lock_describe "$target") after ${waited}s; giving up" >&2
      return 1
    fi
    sleep "$poll"
  done
}

# live_lock_release -- give the lock back, if this shell holds it.
live_lock_release() {
  [ -z "$LIVE_LOCK_RECLAIMING" ] || rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
  LIVE_LOCK_RECLAIMING=""
  [ -n "$LIVE_LOCK_MINE" ] || return 0
  [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" != "$LIVE_LOCK_MINE" ] || rm -f "$LIVE_LOCK"
  LIVE_LOCK_MINE=""
}

# live_lock_traps -- arm EXIT, INT and TERM to give the lock back, for a
# recipe that holds it around something other than `live_session` (which arms
# its own, the put-back first). Call it before `live_lock_hold`.
live_lock_traps() {
  trap 'live_lock_release' EXIT
  trap 'trap - EXIT INT; live_lock_release; kill -INT $$' INT
  trap 'trap - EXIT TERM; live_lock_release; kill -TERM $$' TERM
}

# A `live-up` JVM PRODUCT KEEPS THE LOCK (#2121). `live up` used to give the
# lock back once the product was seeded, so a suite run by hand against it
# (`cargo test ... --test live_confluence_seeded`) held nothing, and a recipe
# from another worktree took the lock, found the product up and ran beside it
# on the same seeded pages. So `live_up_held` hands the lock to a keeper: a
# detached shell holding it as `live-up-<product>-kept` in this worktree while
# the product is up. This worktree's runs go on under it (`live_lock_hold`
# lets them through, taking and releasing nothing); every other worktree's
# run waits, with the 45 minutes and the refusal of any holder. `live_down
# <product>`, from any worktree, ends the keeper and frees the lock; a product
# gone some other way frees it at the keeper's next look
# (`KNOBAS_LIVE_KEEPER_POLL`, default 30 s); a killed keeper is a dead holder.
_live_lock_kept_here() {
  local pid since recipe tree
  read -r pid since recipe tree <<<"$1"
  case $recipe in live-up-*-kept) ;; *) return 1 ;; esac
  [ "$tree" = "${LIVE_TESTENV%/testenv}" ] && _live_lock_pid_alive "$1"
}

# _live_lock_keep <product> -- hand the JVM lock this shell holds to a keeper.
_live_lock_keep() {
  local product=$1 old=$LIVE_LOCK_MINE kept="" i
  # Let through under this worktree's keeper: that one keeps.
  [ -n "$old" ] || return 0
  (
    trap '' HUP INT
    trap 'exit 0' TERM
    LIVE_LOCK_MINE=""
    trap 'live_lock_release' EXIT
    me=$(exec sh -c 'echo $PPID')
    mine="$me $(date +%s) live-up-$product-kept ${LIVE_TESTENV%/testenv}"
    # Swapped in with a rename, so the lock is never free in between.
    ln -s "$mine" "$LIVE_LOCK.keep.$me" || exit 1
    if [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$old" ] &&
      mv -f "$LIVE_LOCK.keep.$me" "$LIVE_LOCK"; then
      LIVE_LOCK_MINE=$mine
      while live_is_up "$product" && [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$mine" ]; do
        sleep "${KNOBAS_LIVE_KEEPER_POLL:-30}" &
        wait $! || true
      done
    fi
    rm -f "$LIVE_LOCK.keep.$me"
  ) </dev/null >/dev/null 2>&1 &
  for ((i = 0; i < 100; i++)); do
    kept=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
    [ "$kept" = "$old" ] || break
    sleep 0.1
  done
  if [ -n "$kept" ] && [ "$kept" != "$old" ]; then
    LIVE_LOCK_MINE=""
    echo "live: $product stays up and holds the JVM lock until 'just live-down $product'; this worktree's runs go on under it, other worktrees' wait"
  else
    echo "live: $product: no keeper took the JVM lock; giving it back" >&2
  fi
}

# _live_lock_unkeep <product> -- end the keeper of this product's live-up.
_live_lock_unkeep() {
  local lock=${KNOBAS_LIVE_LOCK:-${TMPDIR:-/tmp}/knobas-live-jvm.lock} target pid since recipe tree
  target=$(readlink "$lock" 2>/dev/null || true)
  read -r pid since recipe tree <<<"$target"
  [ "$recipe" = "live-up-$1-kept" ] || return 0
  kill "$pid" 2>/dev/null || true
  [ "$(readlink "$lock" 2>/dev/null || true)" != "$target" ] || rm -f "$lock"
  echo "live: $1: the JVM lock its live-up kept is free"
}

# live_up_held <product> -- `testenv/live up`: a JVM product starts and seeds
# under the JVM lock (#690), which is then kept for its life (#2121).
live_up_held() {
  if ! live_is_jvm "$1"; then live_up "$1"; return; fi
  live_lock_traps
  live_lock_hold "live-up-$1" || return 1
  live_up "$1" || return 1
  _live_lock_keep "$1"
}

# live_to_put_back <product> -- what a run of this one stops again at its end:
# the product itself, if it was not already up -- and NEVER Gitea (#698).
# The Gitea lock (#724) serialises the Gitea runs, not Gitea's users: by the
# time any run that started it ends -- a TeamCity run that brought it up for
# its VCS roots, which takes no Gitea lock, or a `just gitea-live` -- another
# worktree's run may be using it, and stopping it would pull it out from under
# that run mid-suite. Gitea is light (~130 MB) and stays up; only `just
# live-down gitea` stops it.
live_to_put_back() {
  [ "$1" = gitea ] || live_is_up "$1" || echo "$1"
}

# live_left_up <product> -- the other side of that rule: what a run of this
# one starts and leaves up, because it was not up before and is not put back.
live_left_up() {
  local p out="" put_back
  put_back=$(live_to_put_back "$1")
  for p in $(live_needs "$1"); do
    [ "$p" = "$put_back" ] || live_is_up "$p" || out="${out:+$out }$p"
  done
  echo "$out"
}

_live_compose() {
  (cd "$LIVE_TESTENV" && docker compose --env-file images.env "$@")
}

# The timebomb keys, fetched once per shell. Before either Atlassian `up`:
# Confluence reads its key at first start.
_live_licences() {
  if [ -z "${JIRA_LICENSE_KEY:-}" ] || [ -z "${CONFLUENCE_LICENSE_KEY:-}" ]; then
    eval "$("$LIVE_TESTENV/fetch-timebomb-keys.sh")"
    export JIRA_LICENSE_KEY CONFLUENCE_LICENSE_KEY
  fi
}

# Where this machine's Gitea answers from the host, which is what every live
# suite, the seed and Björn's own app talk to.
LIVE_GITEA_URL=${KNOBAS_GITEA_URL:-http://127.0.0.1:3000}

# _live_gitea_wait_s -- the guard below, refused unless whole seconds; read by
# the caller before it starts anything.
_live_gitea_wait_s() {
  local wait_s=${KNOBAS_LIVE_GITEA_WAIT_S:-60}
  case $wait_s in
    '' | 0* | *[!0-9]*)
      echo "live: gitea: KNOBAS_LIVE_GITEA_WAIT_S must be a whole number of seconds, got '$wait_s'" >&2
      return 1 ;;
  esac
  echo "$wait_s"
}

# _live_gitea_answers <guard seconds> -- whether the host reaches Gitea: a
# container that is healthy inside can still be unreachable from the host.
# Polled until it answers, under a guard in seconds (#1961):
# KNOBAS_LIVE_GITEA_WAIT_S, 60 by default, whole seconds only. Under load the
# first answer through the port forward comes late, and ten counted tries ran
# out on a Gitea with nothing wrong with it. `up --wait` has already seen the
# container healthy, so a miss here is the dead forward the caller heals, and
# the guard is what a heal costs. A miss names the guard and curl's last
# complaint.
_live_gitea_answers() {
  local wait_s=$1 from=$SECONDS last
  until last=$(curl -fsS -m 3 -o /dev/null "$LIVE_GITEA_URL/api/healthz" 2>&1); do
    if [ $((SECONDS - from)) -ge "$wait_s" ]; then
      echo "live: gitea: $LIVE_GITEA_URL did not answer within $wait_s s (KNOBAS_LIVE_GITEA_WAIT_S); last seen: ${last:-no answer}" >&2
      return 1
    fi
    sleep "${KNOBAS_LIVE_GITEA_PROBE_POLL:-1}"
  done
}

# _live_gitea_lock_held -- whether this process tree holds the Gitea lock: the
# holder `live_lock_hold` exported is what the lock's symlink names, and its
# pid is alive.
_live_gitea_lock_held() {
  local lock=${KNOBAS_LIVE_GITEA_LOCK:-${TMPDIR:-/tmp}/knobas-live-gitea.lock}
  [ -n "${KNOBAS_LIVE_GITEA_LOCK_HOLDER:-}" ] &&
    [ "$(readlink "$lock" 2>/dev/null || true)" = "$KNOBAS_LIVE_GITEA_LOCK_HOLDER" ] &&
    _live_lock_pid_alive "$KNOBAS_LIVE_GITEA_LOCK_HOLDER"
}

# _live_gitea_up -- `up` Gitea, and heal the one way it comes up unreachable.
#
# `up -d --wait` recreates the container whenever docker-compose.yml changed
# its service (#722 did, for the seeded mirror), and on OrbStack a recreate
# can leave the host's port forward dead: `--wait` passes on the container's
# own healthcheck while 127.0.0.1:3000 answers nothing. `stop` + `up` heals
# it; `restart` and another recreate do not (measured 2026-09-06). This runs
# only inside the Gitea lock (#724): a `just gitea-live` holds it, so no other
# Gitea run is mid-suite when the container stops. A start without it -- a
# TeamCity run's Gitea, `just live-up gitea` -- fails instead and says how to
# heal: no live run stops Gitea behind a Gitea run's back (#699).
_live_gitea_up() {
  local wait_s
  wait_s=$(_live_gitea_wait_s) || return 1
  _live_compose up -d --wait gitea
  _live_gitea_answers "$wait_s" && return 0
  if ! _live_gitea_lock_held; then
    echo "live: gitea: the container is up but $LIVE_GITEA_URL does not answer (the OrbStack port forward a recreate kills); not holding the Gitea lock, so not stopping it -- 'just gitea-live' heals it under the lock" >&2
    return 1
  fi
  echo "live: gitea: the container is up but $LIVE_GITEA_URL does not answer (the OrbStack port forward a recreate kills); stop + up" >&2
  _live_compose stop gitea
  _live_compose up -d --wait gitea
  _live_gitea_answers "$wait_s" || {
    echo "live: gitea: $LIVE_GITEA_URL still does not answer after stop + up" >&2
    return 1
  }
}

# _live_start <product> -- start one product (not its needs) and seed it.
# Every seed is idempotent, so starting a product that is already up is a
# re-check that writes nothing.
_live_start() {
  local product=$1
  echo "live: $product: up and seeded ..."
  case $product in
    gitea)
      _live_gitea_up || return 1
      (cd "$LIVE_TESTENV" && ./seed-gitea.sh) ;;
    forgejo)
      # Forgejo (#1195): opt-in profile, port 3002, its own seed state. Not a
      # JVM, so never refused; stopped again at the end of a run that started it.
      _live_compose --profile forgejo up -d --wait forgejo
      curl -fsS -m 5 -o /dev/null "${KNOBAS_FORGEJO_URL:-http://127.0.0.1:3002}/api/healthz" || {
        echo "live: forgejo: the container is up but the host does not reach it on port 3002 (stop + up heals the OrbStack port forward)" >&2
        return 1
      }
      (cd "$LIVE_TESTENV" && ./seed-forgejo.sh) || return 1 ;;
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
  for p in $(live_needs "$1"); do _live_start "$p" || return 1; done
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
    forgejo) _live_compose --profile forgejo stop forgejo ;;
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
  live_is_jvm "$1" && _live_lock_unkeep "$1"
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

# live_session <product> [<recipe>] -- for a recipe: hold the JVM lock (as
# <recipe>) for the whole run of a JVM product, the Gitea lock for a Gitea one
# (#724); bring the product up, export what `./seed --env` prints, and arm
# EXIT, INT and TERM to put the product back as this found it. A product that
# was already up -- `just live-up jira` before a run of `just jira-live` -- is
# left up; one this started is stopped. Gitea,
# started by a Gitea run or for a TeamCity one, is the exception: it stays up
# (#698, `live_to_put_back`).
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
  if live_is_jvm "$1"; then
    live_lock_hold "${2:-live_session-$1}" || exit 1
  elif [ "$1" = gitea ] || [ "$1" = forgejo ]; then
    # Forgejo shares the Gitea lock (#1195): the suite and its sweeps are the
    # same, and two Gitea-suite runs at once are what #724 ended.
    live_lock_hold "${2:-live_session-$1}" gitea || exit 1
  fi
  live_guard "$1" || exit 1
  LIVE_PUT_BACK=$(live_to_put_back "$1")
  local need kept
  kept=$(live_left_up "$1")
  live_up "$1" || exit 1
  if [ "$1" = forgejo ]; then
    eval "$(cd "$LIVE_TESTENV" && ./seed --env-forgejo)"
  else
    # Captured first: eval "$(...)" would swallow a refused token (#1377).
    seed_env=$(cd "$LIVE_TESTENV" && ./seed --env) || exit 1
    eval "$seed_env"
  fi
  if [ -n "$LIVE_PUT_BACK" ]; then
    echo "live: $1 is up for this run; $LIVE_PUT_BACK will be stopped at its end"
  elif [[ " $kept " != *" $1 "* ]]; then
    echo "live: $1 was already up and stays up"
  fi
  for need in $kept; do
    echo "live: $need was started for this run and stays up after it (#698;" \
      "other worktrees' runs may share it) -- 'just live-down $need' stops it"
  done
}
