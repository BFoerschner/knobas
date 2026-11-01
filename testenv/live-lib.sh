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
# docs/archive/2026-10-unattended-rulings.md.
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
# NEVER A STRAY REMOTE ENGINE. Until #634 the docker contexts `knobas-teamcity`,
# `knobas-jira` and `knobas-confluence` reached three Hetzner servers over ssh.
# The servers are gone, and a context of that name left active on a machine
# would send these calls to an address that answers nothing -- or to whatever
# holds it next. `live_guard` refuses such a context before anything else.
#
# A REMOTE TARGET, DECLARED (Björn, 2026-11-01). The products may instead run
# 24/7 on another machine -- a WSL box with the memory for all of them at once
# -- named in `testenv/live.env` (git-ignored; `live.env.example` is the
# template). `KNOBAS_LIVE_TARGET=remote` with `KNOBAS_LIVE_SSH=<ssh host>`:
#
#   - every `docker` call in this file, the seeds and the suites goes to that
#     machine's engine (`DOCKER_HOST=ssh://<host>`, exported here);
#   - one ssh master forwards the product ports (3000 3002 8080 8090 8111) from
#     this machine's 127.0.0.1 to that machine's 127.0.0.1, so every URL the
#     seeds, `./seed --env` and the suites use is the one they use locally;
#   - no run stops a product when it ends (`live_to_put_back`), and the
#     one-JVM refusal is off (`KNOBAS_LIVE_ONE_JVM`, default 0 there): the
#     products stay up until THE MEMORY BUDGET (below) needs their room;
#   - every container is capped (docker-compose.remote.yml, via COMPOSE_FILE).
#
# The locks stay on this machine: the worktrees that take turns are all here,
# and on a remote target each JVM product has its own (ONE LOCK PER PRODUCT,
# below).
# Uptime Kuma follows the target too (`just kuma-live` sources this file),
# and so do the canary and the stall (canary.sh, stall.sh run themselves on
# that machine). With a remote target this machine runs no container. Unset, or
# `local`, everything below behaves as it did before the remote target
# existed. testenv/README.md, "Live products on a remote machine (WSL)".

LIVE_TESTENV=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# _live_target_file -- the live.env this checkout reads: its own testenv/'s,
# else the main checkout's, so every worktree follows the one file. Prints
# nothing when there is none.
_live_target_file() {
  local common
  if [ -r "$LIVE_TESTENV/live.env" ]; then echo "$LIVE_TESTENV/live.env"; return; fi
  common=$(git -C "$LIVE_TESTENV" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || return 0
  [ ! -r "${common%/.git}/testenv/live.env" ] || echo "${common%/.git}/testenv/live.env"
}

# _live_target_load -- KNOBAS_LIVE_* from live.env, each only when the
# environment does not already set it (the environment wins, per variable:
# the unit tests pin KNOBAS_LIVE_TARGET=local whatever the file says). Read
# as KEY=VALUE lines, never sourced; anything else in it is refused.
_live_target_load() {
  local file line key value
  file=$(_live_target_file)
  [ -n "$file" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line%$'\r'}
    case $line in '' | '#'*) continue ;; esac
    line=${line#export }
    key=${line%%=*}
    value=${line#*=}
    case $key in
      KNOBAS_LIVE_[A-Z0-9_]*) ;;
      *) echo "live: $file: '$line' is not a KNOBAS_LIVE_*=value line" >&2; return 1 ;;
    esac
    [ "$key" != "$line" ] || { echo "live: $file: '$line' has no '='" >&2; return 1; }
    case $value in \"*\") value=${value#\"}; value=${value%\"} ;; \'*\') value=${value#\'}; value=${value%\'} ;; esac
    [ -n "${!key+set}" ] || export "$key=$value"
  done <"$file"
}

_live_target_load || return 1
LIVE_TARGET=${KNOBAS_LIVE_TARGET:-local}
case $LIVE_TARGET in
  local) LIVE_REMOTE="" ;;
  remote)
    LIVE_REMOTE=1
    if [ -z "${KNOBAS_LIVE_SSH:-}" ]; then
      echo "live: KNOBAS_LIVE_TARGET=remote names no machine; set KNOBAS_LIVE_SSH to its ssh host in testenv/live.env" >&2
      return 1
    fi
    export KNOBAS_LIVE_TARGET=remote KNOBAS_LIVE_SSH
    export DOCKER_HOST="ssh://$KNOBAS_LIVE_SSH"
    # The memory caps (docker-compose.remote.yml) on every compose call a
    # remote run makes, the recipes' and the seeds' too; an explicit `-f`
    # (gitea-live-capped's overlay) replaces it for that one call.
    export COMPOSE_FILE="$LIVE_TESTENV/docker-compose.yml:$LIVE_TESTENV/docker-compose.remote.yml"
    # The caps in MB, read by docker-compose.remote.yml.
    export KNOBAS_LIVE_GITEA_MB=${KNOBAS_LIVE_GITEA_MB:-512} \
      KNOBAS_LIVE_TEAMCITY_MB=${KNOBAS_LIVE_TEAMCITY_MB:-2048} \
      KNOBAS_LIVE_TEAMCITY_AGENT_MB=${KNOBAS_LIVE_TEAMCITY_AGENT_MB:-1024} \
      KNOBAS_LIVE_JIRA_MB=${KNOBAS_LIVE_JIRA_MB:-4608} \
      KNOBAS_LIVE_CONFLUENCE_MB=${KNOBAS_LIVE_CONFLUENCE_MB:-3072} \
      KNOBAS_LIVE_DB_MB=${KNOBAS_LIVE_DB_MB:-512} ;;
  *)
    echo "live: KNOBAS_LIVE_TARGET must be local or remote, not '$LIVE_TARGET'" >&2
    return 1 ;;
esac
# The one-JVM refusal: on for this machine's 8 GB VM, off for a remote target
# that keeps all three up. Either may be overridden.
LIVE_ONE_JVM=${KNOBAS_LIVE_ONE_JVM:-$([ -n "$LIVE_REMOTE" ] && echo 0 || echo 1)}
LIVE_TUNNEL_PORTS=${KNOBAS_LIVE_TUNNEL_PORTS:-3000 3001 3002 8080 8090 8111}

# _live_tunnel_sock -- the ssh master's control socket. Short on purpose:
# macOS caps a socket path at 104 bytes and $TMPDIR is already half of that.
_live_tunnel_sock() {
  echo "${KNOBAS_LIVE_TUNNEL_SOCK:-${TMPDIR:-/tmp}/knobas-live-tunnel.sock}"
}

# With -S the host argument names nothing ssh dials, so a local shell, which
# has no KNOBAS_LIVE_SSH, can still ask about (and close) a tunnel left open.
live_tunnel_is_up() {
  [ -S "$(_live_tunnel_sock)" ] || return 1
  ssh -S "$(_live_tunnel_sock)" -O check "${KNOBAS_LIVE_SSH:-knobas-live-tunnel}" >/dev/null 2>&1
}

# live_tunnel_up -- one ssh master, left running, forwarding every product
# port from this machine's loopback to the remote's. A second worktree racing
# to open it fails to bind and then finds the first one's master.
live_tunnel_up() {
  local p args=() out
  live_tunnel_is_up && return 0
  for p in $LIVE_TUNNEL_PORTS; do args+=(-L "127.0.0.1:$p:127.0.0.1:$p"); done
  if out=$(ssh -f -N -M -S "$(_live_tunnel_sock)" \
    -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -o ServerAliveCountMax=3 \
    -o ControlPersist=no "${args[@]}" "$KNOBAS_LIVE_SSH" 2>&1 </dev/null); then
    echo "live: tunnel to $KNOBAS_LIVE_SSH up (127.0.0.1: $LIVE_TUNNEL_PORTS)"
    return 0
  fi
  live_tunnel_is_up && return 0
  echo "live: could not open the tunnel to $KNOBAS_LIVE_SSH: ${out:-no output}" >&2
  for p in $LIVE_TUNNEL_PORTS; do
    if lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; then
      echo "live: 127.0.0.1:$p is already bound here -- a local product still up? 'docker --context orbstack ps' and stop it" >&2
    fi
  done
  return 1
}

live_tunnel_down() {
  if live_tunnel_is_up; then
    ssh -S "$(_live_tunnel_sock)" -O exit "${KNOBAS_LIVE_SSH:-knobas-live-tunnel}" >/dev/null 2>&1 || true
    echo "live: tunnel to ${KNOBAS_LIVE_SSH:-the remote target} down"
  else
    echo "live: no tunnel to ${KNOBAS_LIVE_SSH:-a remote target}"
  fi
}

# live_remote_ready -- for a remote target: the engine answers and the tunnel
# is up. Nothing at all for a local one. Once per shell.
LIVE_REMOTE_READY=""
live_remote_ready() {
  if [ -z "$LIVE_REMOTE" ]; then
    # A LOCAL run next to a tunnel left open by a remote one: the tunnel holds
    # 127.0.0.1:3000/8080/8090/8111/3002 here, so a local product cannot
    # publish its port, and its seeds and suites would talk to the remote
    # product through the tunnel instead (#2366's first day: a worktree whose
    # code predates the remote target ran `just jira-live` beside it).
    if live_tunnel_is_up; then
      echo "live: an ssh tunnel to a remote target holds this machine's product ports ($LIVE_TUNNEL_PORTS); 'testenv/live tunnel down' frees them, or run with KNOBAS_LIVE_TARGET=remote" >&2
      return 1
    fi
    return 0
  fi
  [ -z "$LIVE_REMOTE_READY" ] || { live_tunnel_up; return; }
  local v
  if ! v=$(docker version --format '{{.Server.Version}}' 2>&1); then
    echo "live: the docker engine on $KNOBAS_LIVE_SSH ($DOCKER_HOST) does not answer: $v" >&2
    echo "live: is the machine up, 'ssh $KNOBAS_LIVE_SSH docker ps' allowed, and dockerd running? testenv/README.md, 'Live products on a remote machine (WSL)'" >&2
    return 1
  fi
  live_tunnel_up || return 1
  LIVE_REMOTE_READY=1
}

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
  # A declared remote target is the one remote engine allowed: DOCKER_HOST
  # names it, and docker reads that before any context.
  [ -z "$LIVE_REMOTE" ] || return 0
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
  [ "$LIVE_ONE_JVM" != 0 ] || return 0
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
LIVE_LOCK_RECLAIMED=""

# _live_lock_describe <target> -- the holder, in words.
_live_lock_describe() {
  local pid since recipe tree hint="" product
  read -r pid since recipe tree <<<"$1"
  case $recipe in live-up-*-kept)
    product=${recipe#live-up-}
    hint=", kept for a live-up product: 'just live-down ${product%-kept}' from any worktree frees it" ;;
  esac
  echo "'$recipe' in ${tree:-?} (pid $pid, since $(date -r "$since" '+%H:%M:%S' 2>/dev/null || echo "$since")$hint)"
}

# _live_lock_reclaim <target> -- remove a dead holder's lock, under
# `mkdir "$lock.reclaim"`, if it still names <target>. Fails when another
# reclaim is under way; LIVE_LOCK_RECLAIMED says whether this one removed it.
_live_lock_reclaim() {
  LIVE_LOCK_RECLAIMED=""
  mkdir "$LIVE_LOCK.reclaim" 2>/dev/null || return 1
  LIVE_LOCK_RECLAIMING=1
  if [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$1" ]; then
    rm -f "$LIVE_LOCK"
    LIVE_LOCK_RECLAIMED=1
  fi
  rmdir "$LIVE_LOCK.reclaim" 2>/dev/null || true
  LIVE_LOCK_RECLAIMING=""
}

_live_lock_pid_alive() {
  local pid=${1%% *}
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

# ONE LOCK PER PRODUCT ON A REMOTE TARGET (Björn, 2026-11-01). The JVM lock
# had two jobs: the 8 GB VM's memory, and two runs in one product's data. On
# a remote target the products stay up together and the memory budget below
# keeps them from freezing the box, so only the second job is left -- and it
# is per product. There the "JVM lock" of a product is its own,
# `$TMPDIR/knobas-live-<product>.lock` (`KNOBAS_LIVE_LOCK_DIR` moves them;
# the holder is passed down in `KNOBAS_LIVE_LOCK_HOLDER_<PRODUCT>`): a Jira
# run and a TeamCity run from two worktrees go at once, two Jira runs take
# turns. A JVM lock asked for with no product -- `atlassian-live` around its
# two halves -- is no lock there, since each half takes its own. The local
# target keeps the one machine-wide lock.
#
# _live_lock_select <jvm|gitea> [<product>] -- point LIVE_LOCK and its label
# and holder variable at a lock.
_live_lock_select() {
  case $1 in
    jvm)
      if [ -n "$LIVE_REMOTE" ] && [ -n "${2:-}" ]; then
        LIVE_LOCK=${KNOBAS_LIVE_LOCK_DIR:-${TMPDIR:-/tmp}}/knobas-live-$2.lock
        LIVE_LOCK_LABEL="$2 lock"
        LIVE_LOCK_HOLDER_VAR=KNOBAS_LIVE_LOCK_HOLDER_$(printf '%s' "$2" | tr '[:lower:]' '[:upper:]')
        return 0
      fi
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

# live_lock_hold <recipe> [<jvm|gitea> [<product>]] -- return once this
# shell holds the lock (the JVM one unless named -- on a remote target, the
# product's; or runs under a parent that does); return 1, naming the holder,
# after the wait. Arm `live_lock_release` in the EXIT, INT and TERM traps
# before calling it.
live_lock_hold() {
  local recipe=$1 me target waited announced='' last_report=0 wait_s poll from=$SECONDS
  if [ -n "$LIVE_LOCK_MINE" ]; then
    echo "live: $recipe: this shell already holds the $LIVE_LOCK_LABEL; one lock per shell" >&2
    return 1
  fi
  if [ -n "$LIVE_REMOTE" ] && [ "${2:-jvm}" = jvm ] && [ -z "${3:-}" ]; then
    echo "live: $recipe: a remote target locks per product; each run under this one takes its own"
    return 0
  fi
  _live_lock_select "${2:-jvm}" "${3:-}" || return 1
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
      if _live_lock_reclaim "$target"; then
        [ "$LIVE_LOCK_RECLAIMED" != 1 ] ||
          echo "live: $recipe: the $LIVE_LOCK_LABEL was stale -- its holder $(_live_lock_describe "$target") is gone; taking it over"
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
  _live_mem_unlock
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
# (`KNOBAS_LIVE_KEEPER_POLL`, default 30 s), as does its worktree's removal or
# `KNOBAS_LIVE_KEEPER_MAX` (3 h); a killed keeper is a dead holder, and
# `live_down` signals its pid only while that is still the keeper (#2145).
_live_lock_kept_here() {
  local pid since recipe tree
  read -r pid since recipe tree <<<"$1"
  case $recipe in live-up-*-kept) ;; *) return 1 ;; esac
  [ "$tree" = "${LIVE_TESTENV%/testenv}" ] && _live_lock_pid_alive "$1"
}

# _live_lock_keep <product> -- hand the JVM lock this shell holds to a keeper.
_live_lock_keep() {
  local product=$1 old=$LIVE_LOCK_MINE kept="" keeper
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
      # Two misses in a row, so one `docker ps` hiccup does not free the lock.
      misses=0 started=$SECONDS max=${KNOBAS_LIVE_KEEPER_MAX:-10800}
      while [ "$misses" -lt 2 ] && [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$mine" ]; do
        if live_is_up "$product"; then misses=0; else misses=$((misses + 1)); fi
        [ "$misses" -lt 2 ] || break
        # A forgotten `live-down` must not queue every other worktree's JVM
        # runs for good: the keeper lets go once its worktree is removed (a
        # merge's cleanup) or after KNOBAS_LIVE_KEEPER_MAX (3 h, the Atlassian
        # licence's life), the product left up for whoever runs next.
        [ -d "$LIVE_TESTENV" ] || break
        [ $((SECONDS - started)) -lt "$max" ] || break
        sleep "${KNOBAS_LIVE_KEEPER_POLL:-30}" &
        wait $! || true
      done
    fi
    rm -f "$LIVE_LOCK.keep.$me"
  ) </dev/null >/dev/null 2>&1 &
  keeper=$!
  # Until the keeper has swapped its link in or given up: no timeout, since
  # this shell's release while the keeper is between its check and its `mv`
  # would let a third run in and have the `mv` overwrite it.
  while kept=$(readlink "$LIVE_LOCK" 2>/dev/null || true); [ "$kept" = "$old" ] &&
    kill -0 "$keeper" 2>/dev/null; do
    sleep 0.1
  done
  if [ -n "$kept" ] && [ "$kept" != "$old" ]; then
    LIVE_LOCK_MINE=""
    echo "live: $product stays up and holds the JVM lock until 'just live-down $product'; this worktree's runs go on under it, other worktrees' wait"
  else
    echo "live: $product: no keeper took the JVM lock; giving it back" >&2
  fi
}

# _live_lock_is_keeper <pid> <epoch> <product> -- the pid is still the keeper
# the lock names (#2145): a fork of `testenv/live up <product>`, so its
# command line says so, started no later than the epoch it stamped. A keeper
# killed without giving the lock back leaves its pid behind, and macOS may
# hand that pid to any process since: one started after the epoch, or one
# running something else, is not the keeper.
_live_lock_is_keeper() {
  local cmd start
  cmd=$(ps -o command= -p "$1" 2>/dev/null) || return 1
  case $cmd in *"live up $3"*) ;; *) return 1 ;; esac
  start=$(LC_ALL=C ps -o lstart= -p "$1" 2>/dev/null) || return 1
  read -r start <<<"$start" # ps pads the field
  # BSD date (macOS), else GNU's.
  start=$(LC_ALL=C date -j -f '%a %b %d %T %Y' "$start" +%s 2>/dev/null ||
    LC_ALL=C date -d "$start" +%s 2>/dev/null) || return 1
  [ "$start" -le "$2" ]
}

# _live_lock_unkeep <product> -- end the keeper of this product's live-up.
_live_lock_unkeep() {
  local target pid since recipe tree i
  _live_lock_select jvm "$1"
  target=$(readlink "$LIVE_LOCK" 2>/dev/null || true)
  read -r pid since recipe tree <<<"$target"
  [ "$recipe" = "live-up-$1-kept" ] || return 0
  # A pid that is not the keeper any more is a dead holder: its lock goes,
  # as a taker's would, and nothing is signalled.
  if ! _live_lock_is_keeper "$pid" "$since" "$1"; then
    if _live_lock_reclaim "$target"; then
      echo "live: $1: pid $pid is not its keeper any more (gone, or reused); the JVM lock its live-up kept is free, nothing signalled"
    else
      echo "live: $1: pid $pid is not its keeper any more (gone, or reused); nothing signalled, and the next run takes the lock over"
    fi
    return 0
  fi
  # The keeper's own trap gives the link back; removing it here could remove
  # a lock another run took a moment after. A keeper that is gone without
  # doing so left a dead holder, which the next taker takes over.
  kill "$pid" 2>/dev/null || true
  for ((i = 0; i < 50; i++)); do
    [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" = "$target" ] || break
    sleep 0.1
  done
  if [ "$(readlink "$LIVE_LOCK" 2>/dev/null || true)" != "$target" ]; then
    echo "live: $1: the JVM lock its live-up kept is free"
  else
    echo "live: $1: the keeper of its live-up's JVM lock did not let go; the next run takes it over once pid $pid is gone" >&2
  fi
}

# live_up_held <product> -- `testenv/live up`: a JVM product starts and seeds
# under the JVM lock (#690), which is then kept for its life (#2121).
live_up_held() {
  if ! live_is_jvm "$1"; then live_up "$1"; return; fi
  live_lock_traps
  live_lock_hold "live-up-$1" jvm "$1" || return 1
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
  # A remote target keeps its products up around the clock.
  [ -z "$LIVE_REMOTE" ] || return 0
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
      (cd "$LIVE_TESTENV" && ./seed-gitea.sh) || return 1 ;;
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
      _live_compose --profile real-teamcity up -d teamcity teamcity-agent || return 1
      (cd "$LIVE_TESTENV" && ./seed-teamcity.sh) || return 1 ;;
    jira | confluence)
      _live_licences || return 1
      _live_atlassian_start "$product" || return 1 ;;
  esac
  echo "live: $product: up"
}

# THE 3-HOUR LICENCE, RENEWED WHEN IT IS DUE. A timebomb key runs out three
# hours after the wizard applies it, and nothing renews it but a new instance.
# A product only ever up for one run never met that; one kept up -- `live-up`
# for a session, or a remote target that keeps it around the clock, or a
# stopped container a reboot left behind -- does. So before Jira or
# Confluence is started or re-seeded, the age of its data volume is read
# (created before the wizard ran, so the estimate errs early): with less than
# KNOBAS_LIVE_LICENCE_MARGIN_S left (3600 by default), it is removed with its
# volumes and set up from empty. The caller holds the JVM lock, so no other
# run is in it. An age that cannot be read -- no container, or no answer --
# removes nothing.
LIVE_LICENCE_S=10800

# _live_epoch <RFC 3339 time> -- seconds since the epoch, for docker's
# spellings: fractional seconds, and `Z` or a `+01:00` offset.
_live_epoch() {
  jq -rn --arg t "$1" '
    $t | capture("^(?<d>[0-9-]+T[0-9:]+)(\\.[0-9]+)?(?<z>Z|(?<s>[+-])(?<h>[0-9]{2}):(?<m>[0-9]{2}))$")
    | ((.d + "Z") | fromdateiso8601)
      - (if .z == "Z" then 0 else (if .s == "+" then 1 else -1 end) * ((.h | tonumber) * 3600 + (.m | tonumber) * 60) end)' 2>/dev/null
}

# _live_engine_now -- the engine's clock, which is the one `Created` was read
# by: a WSL clock drifts after the host sleeps, and the Mac's would then
# misjudge a container's age. This machine's clock when the engine says
# nothing.
_live_engine_now() {
  local t now
  t=$(docker info --format '{{.SystemTime}}' 2>/dev/null) && now=$(_live_epoch "$t") &&
    case $now in '' | *[!0-9]*) false ;; esac && { echo "$now"; return; }
  date +%s
}

# _live_licence_left <product> -- seconds of licence left, by the age of the
# product's data VOLUME: the licence lives in it, and only a reset removes
# it. Not the container's age: compose recreates a container whenever its
# configuration changes (a moved image pin, a raised memory cap), which
# would make an old licence look new. The container's age is the fallback
# for a volume docker cannot date. Fails when neither is there.
_live_licence_left() {
  local created epoch
  created=$(docker volume inspect -f '{{.CreatedAt}}' "knobas-testenv_$1-data" 2>/dev/null) || created=""
  epoch=$(_live_epoch "$created") || epoch=""
  if [ -z "$epoch" ]; then
    created=$(docker inspect -f '{{.Created}}' "$(_live_main "$1")" 2>/dev/null) || return 1
    epoch=$(_live_epoch "$created") || return 1
  fi
  case $epoch in '' | *[!0-9]*) return 1 ;; esac
  echo $((LIVE_LICENCE_S - ($(_live_engine_now) - epoch)))
}

# _live_atlassian_remove <product> -- the product and its database removed
# with their volumes, and its seed-state.json block with them. `live_down`'s
# removal, without ending a live-up keeper: a renewal under this worktree's
# keeper puts the product straight back.
_live_atlassian_remove() {
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
  fi
}

# _live_atlassian_api <product> -- wait until its REST API answers 200 for the
# admin. `/status` says RUNNING while the plugins behind the API still warm
# up: a Jira restarted from a stopped container answered `/rest/api/2/myself`
# with a 503 for a while after it (2026-11-01), the content seed failed on
# it, and the retry below rebuilt a healthy instance from empty. The right
# password only: a wrong one locks the account (README, "Never send this Jira
# a wrong password"). KNOBAS_LIVE_API_WAIT_S, 300 by default.
_live_atlassian_api() {
  local url path code from=$SECONDS wait_s=${KNOBAS_LIVE_API_WAIT_S:-300}
  case $1 in
    jira) url=http://127.0.0.1:8080 path=/rest/api/2/myself ;;
    confluence) url=http://127.0.0.1:8090 path='/rest/api/space?limit=1' ;;
  esac
  while :; do
    code=$(curl -s -m 10 -o /dev/null -w '%{http_code}' -u knobas:knobas-dev "$url$path" 2>/dev/null) || code=000
    [ "$code" != 200 ] || return 0
    if [ $((SECONDS - from)) -ge "$wait_s" ]; then
      echo "live: $1: $url$path still answers $code after ${wait_s}s (KNOBAS_LIVE_API_WAIT_S)" >&2
      return 1
    fi
    sleep 5
  done
}

# _live_atlassian_seed <product> -- up, wizard, content, verified.
_live_atlassian_seed() {
  _live_compose --profile real-atlassian up -d --wait "$1-db" "$1" || return 1
  (cd "$LIVE_TESTENV" && ./seed-atlassian.sh "$1") || return 1
  _live_atlassian_api "$1" || return 1
  # Twice, deliberately: the second run reaches the find-then-skip branches
  # a seed from empty never does (#396), and must write nothing.
  (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$1") || return 1
  (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$1") || return 1
  (cd "$LIVE_TESTENV" && ./seed-atlassian-content.sh "$1" --verify) || return 1
}

# _live_atlassian_start <product> -- renew a licence that is due, then seed.
# A seed that fails against an instance that was already there -- data a
# killed run left half-changed, an account Jira locked after bad passwords,
# a licence that ran out early -- is retried ONCE from empty; a seed that
# fails from empty is a real failure and is reported as one.
_live_atlassian_start() {
  local product=$1 left margin existed=""
  margin=${KNOBAS_LIVE_LICENCE_MARGIN_S:-3600}
  case $margin in '' | *[!0-9]*)
    echo "live: KNOBAS_LIVE_LICENCE_MARGIN_S must be whole seconds, not '$margin'" >&2; return 1 ;;
  esac
  if [[ " $LIVE_RENEWED " == *" $product "* ]]; then
    : # renewed in the memory section a moment ago: from empty
  elif left=$(_live_licence_left "$product"); then
    if [ "$left" -lt "$margin" ]; then
      echo "live: $product: its 3-hour licence has about $((left > 0 ? left / 60 : 0)) min left (under KNOBAS_LIVE_LICENCE_MARGIN_S=${margin}s); removing it and setting it up from empty"
      _live_atlassian_remove "$product"
    else
      existed=1
      echo "live: $product: about $((left / 60)) min of its 3-hour licence left; re-seeding it"
    fi
  fi
  _live_atlassian_seed "$product" && return 0
  [ -n "$existed" ] || return 1
  echo "live: $product: the seed failed against the instance that was already there; removing it and setting it up from empty, once" >&2
  _live_atlassian_remove "$product"
  _live_atlassian_seed "$product"
}

# THE MEMORY BUDGET, ON A REMOTE TARGET. The WSL box has 16 GB of physical
# memory and WSL 10 to 12 of it (setup-windows.ps1), and a VM pushed into
# swap stops answering ssh -- a frozen box, not a failed run. So before a
# product starts there, what each product up USES is added to what this one
# will use, and while the sum is over the budget the other JVM products are
# STOPPED, the one started longest ago first, until it fits. The figures are
# measured, not the caps (docker-compose.remote.yml, which are ceilings a
# third higher): all three up with a Jira suite running, 2026-11-01 on the
# real box, Jira and its PostgreSQL peaked at 3.7 GB, Confluence 2.3,
# TeamCity and its agent 1.6, the VM at 8.5 of 12 GB -- so here 4096, 2560,
# 2048 and Gitea 512, with headroom (KNOBAS_LIVE_USE_*_MB overrides each). Stopped, not removed: TeamCity keeps its builds, and
# Jira and Confluence their seed, until their licence is due (above). The
# caller holds the JVM lock, so no other run is inside a product it stops --
# except a product another live-up keeps, which is never stopped. A product
# that does not fit even alone is refused. The budget is the engine's memory
# less KNOBAS_LIVE_MEM_RESERVE_MB (1536: kernel, dockerd, page cache), or
# KNOBAS_LIVE_MEM_BUDGET_MB outright. With the defaults all three fit a 12 GB
# WSL (budget ~10.4 GB, needed 9.2); on a 10 GB one a third stops one.
_live_footprint_mb() {
  case $1 in
    gitea | forgejo) echo "${KNOBAS_LIVE_USE_GITEA_MB:-512}" ;;
    teamcity) echo "${KNOBAS_LIVE_USE_TEAMCITY_MB:-2048}" ;;
    jira) echo "${KNOBAS_LIVE_USE_JIRA_MB:-4096}" ;;
    confluence) echo "${KNOBAS_LIVE_USE_CONFLUENCE_MB:-2560}" ;;
  esac
}

_live_mem_budget_mb() {
  local total
  if [ -n "${KNOBAS_LIVE_MEM_BUDGET_MB:-}" ]; then echo "$KNOBAS_LIVE_MEM_BUDGET_MB"; return; fi
  total=$(docker info --format '{{.MemTotal}}' 2>/dev/null) || return 1
  case $total in '' | *[!0-9]*) return 1 ;; esac
  echo $((total / 1048576 - ${KNOBAS_LIVE_MEM_RESERVE_MB:-1536}))
}

# _live_started_at <product> -- when its main container last started, epoch.
_live_started_at() {
  local at
  at=$(docker inspect -f '{{.State.StartedAt}}' "$(_live_main "$1")" 2>/dev/null) || { echo 0; return; }
  at=$(_live_epoch "$at") || at=""
  echo "${at:-0}"
}

# _live_stop <product> -- stop it, keeping its volumes.
_live_stop() {
  case $1 in
    teamcity) _live_compose --profile real-teamcity stop teamcity teamcity-agent ;;
    jira | confluence) _live_compose --profile real-atlassian stop "$1" "$1-db" ;;
    *) return 1 ;;
  esac
}

# _live_lock_grab <product> -- take a product's lock without waiting, for the
# few seconds a stop takes; fail when anyone holds it -- a run inside that
# product, or the keeper of its live-up. A dead holder's lock is taken over.
# Prints the link it made, for _live_lock_drop.
_live_lock_grab() {
  local lock=${KNOBAS_LIVE_LOCK_DIR:-${TMPDIR:-/tmp}}/knobas-live-$1.lock target
  # $$, the run's own shell: this runs in a command substitution, whose pid
  # is gone the moment it returns, and a dead holder is anyone's to take.
  target="$$ $(date +%s) make-room-stops-$1 ${LIVE_TESTENV%/testenv}"
  if ! ln -s "$target" "$lock" 2>/dev/null; then
    _live_lock_pid_alive "$(readlink "$lock" 2>/dev/null || true)" && return 1
    rm -f "$lock"
    ln -s "$target" "$lock" 2>/dev/null || return 1
  fi
  echo "$target"
}

_live_lock_drop() {
  local lock=${KNOBAS_LIVE_LOCK_DIR:-${TMPDIR:-/tmp}}/knobas-live-$1.lock
  [ "$(readlink "$lock" 2>/dev/null || true)" != "$2" ] || rm -f "$lock"
}

# THE MEMORY SECTION. Runs on two products start them at once now, and each
# would count the other as down and find room. So deciding what fits and
# starting the containers -- not seeding them, which is long -- happens under
# one short lock, `$TMPDIR/knobas-live-mem.lock` (`KNOBAS_LIVE_MEM_LOCK` moves
# it): a directory, the holder's pid inside, a dead holder's taken over.
LIVE_MEM_LOCK_MINE=""
_live_mem_lock() {
  local d=${KNOBAS_LIVE_MEM_LOCK:-${TMPDIR:-/tmp}/knobas-live-mem.lock} pid from=$SECONDS me
  me=$(exec sh -c 'echo $PPID')
  until mkdir "$d" 2>/dev/null; do
    pid=$(cat "$d/pid" 2>/dev/null || true)
    if { [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; } ||
      { [ -z "$pid" ] && [ -n "$(find "$d" -maxdepth 0 -mmin +1 2>/dev/null)" ]; }; then
      if mkdir "$d.reclaim" 2>/dev/null; then
        [ "$(cat "$d/pid" 2>/dev/null || true)" != "$pid" ] || rm -rf "$d"
        rmdir "$d.reclaim"
      fi
      continue
    fi
    if [ $((SECONDS - from)) -ge "${KNOBAS_LIVE_MEM_LOCK_WAIT:-900}" ]; then
      echo "live: the memory section ($d) is still held by pid ${pid:-?}; giving up" >&2
      return 1
    fi
    sleep 1
  done
  echo "$me" >"$d/pid"
  LIVE_MEM_LOCK_MINE=$d
}

_live_mem_unlock() {
  [ -n "$LIVE_MEM_LOCK_MINE" ] || return 0
  rm -rf "$LIVE_MEM_LOCK_MINE"
  LIVE_MEM_LOCK_MINE=""
}

# _live_make_room <product> -- on a remote target, stop other JVM products
# until this one fits the budget; refuse when it cannot. Inside the memory
# section. A product is stopped only under its own lock, taken for the stop:
# one another run holds is in use, and is not a candidate.
_live_make_room() {
  local product=$1 budget used=0 need p victim grabbed candidates
  [ -n "$LIVE_REMOTE" ] || return 0
  live_is_up "$product" && return 0
  budget=$(_live_mem_budget_mb) || { echo "live: could not read the memory of the engine on $KNOBAS_LIVE_SSH; set KNOBAS_LIVE_MEM_BUDGET_MB" >&2; return 1; }
  need=$(_live_footprint_mb "$product")
  for p in gitea forgejo teamcity jira confluence; do
    [ "$p" != "$product" ] || continue
    live_is_up "$p" && used=$((used + $(_live_footprint_mb "$p")))
  done
  # Uptime Kuma and its Docker API run there too (`just kuma-live`), and are
  # never stopped to make room.
  live_running knobas-uptime-kuma && used=$((used + ${KNOBAS_LIVE_USE_KUMA_MB:-512}))
  [ $((used + need)) -gt "$budget" ] || return 0
  candidates=$(for p in teamcity jira confluence; do
    [ "$p" != "$product" ] && live_is_up "$p" && echo "$(_live_started_at "$p") $p"
  done | sort -n | awk '{ print $2 }')
  for victim in $candidates; do
    [ $((used + need)) -gt "$budget" ] || break
    grabbed=$(_live_lock_grab "$victim") || {
      echo "live: $product: $victim would make room but another run holds it; not stopping it"
      continue
    }
    echo "live: $product needs ${need} MB; $((used + need)) MB is over the ${budget} MB budget on $KNOBAS_LIVE_SSH -- stopping $victim (started longest ago; its data stays)"
    _live_stop "$victim" || { _live_lock_drop "$victim" "$grabbed"; return 1; }
    _live_lock_drop "$victim" "$grabbed"
    used=$((used - $(_live_footprint_mb "$victim")))
  done
  if [ $((used + need)) -gt "$budget" ]; then
    echo "live: $product needs ${need} MB and ${used} MB of the ${budget} MB budget on $KNOBAS_LIVE_SSH is held by what may not be stopped; not starting it (KNOBAS_LIVE_MEM_BUDGET_MB, KNOBAS_LIVE_USE_*_MB)" >&2
    return 1
  fi
}

# _live_up_args <product> -- the compose arguments that start it, for the
# memory section: its profile flag before `up -d`, its services after. A
# service before the subcommand is read AS the subcommand (#2368).
_live_up_args() {
  case $1 in
    gitea) echo "up -d gitea" ;;
    forgejo) echo "--profile forgejo up -d forgejo" ;;
    teamcity) echo "--profile real-teamcity up -d teamcity teamcity-agent" ;;
    jira | confluence) echo "--profile real-atlassian up -d $1-db $1" ;;
  esac
}

# _live_claim <product> -- on a remote target, the memory section: renew a
# licence that is due, make room, and start the containers, so the next run
# to count finds this product up. Seeding comes after, outside it.
LIVE_RENEWED=""
_live_claim() {
  local p=$1 left margin
  [ -n "$LIVE_REMOTE" ] || return 0
  case $p in
    jira | confluence)
      margin=${KNOBAS_LIVE_LICENCE_MARGIN_S:-3600}
      if left=$(_live_licence_left "$p") && [ "$left" -lt "$margin" ] 2>/dev/null; then
        echo "live: $p: its 3-hour licence has about $((left > 0 ? left / 60 : 0)) min left (under KNOBAS_LIVE_LICENCE_MARGIN_S=${margin}s); removing it and setting it up from empty"
        _live_atlassian_remove "$p"
        LIVE_RENEWED="$LIVE_RENEWED $p"
      fi
      _live_licences || return 1 ;;
  esac
  live_is_up "$p" && return 0
  _live_make_room "$p" || return 1
  # shellcheck disable=SC2046  # the profile flags and services, split on purpose
  _live_compose $(_live_up_args "$p") >/dev/null || return 1
}

# live_up <product> -- the product and what it needs, guarded.
live_up() {
  local p
  live_remote_ready || return 1
  if [ -n "$LIVE_REMOTE" ]; then
    _live_mem_lock || return 1
    for p in $(live_needs "$1"); do _live_claim "$p" || { _live_mem_unlock; return 1; }; done
    _live_mem_unlock
  fi
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
    jira | confluence) _live_atlassian_remove "$1" ;;
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
    live_lock_hold "${2:-live_session-$1}" jvm "$1" || exit 1
  elif [ "$1" = gitea ] || [ "$1" = forgejo ]; then
    # Forgejo shares the Gitea lock (#1195): the suite and its sweeps are the
    # same, and two Gitea-suite runs at once are what #724 ended.
    live_lock_hold "${2:-live_session-$1}" gitea || exit 1
  fi
  live_remote_ready || exit 1
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
  if [ -n "$LIVE_REMOTE" ] && [ -n "$kept" ]; then
    echo "live: $kept started on $KNOBAS_LIVE_SSH for this run; a remote target keeps it up"
    return 0
  fi
  for need in $kept; do
    echo "live: $need was started for this run and stays up after it (#698;" \
      "other worktrees' runs may share it) -- 'just live-down $need' stops it"
  done
}
