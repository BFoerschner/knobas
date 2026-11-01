#!/usr/bin/env bash
# The on-demand live products' own unit tests (issue #633), run by `just
# witness-unit`. No container is started: `docker` is a stub on PATH that
# answers `ps` from a list this file sets and records every other call, so the
# decisions `live-lib.sh` makes before it touches the engine -- may this
# product start now, and which containers does a run have to put back -- are
# checked without the 8 GB VM they protect.
#
# The rule under test is Björn's of 2026-10-12: one JVM product at a time. A
# second one is refused with a single line naming the one that is up, and the
# refusal comes before any `docker compose` call, so a refused start leaves
# nothing half-created behind.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-live-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'rm -rf "$scratch"; trap - INT; kill -INT $$' INT
trap 'rm -rf "$scratch"; trap - TERM; kill -TERM $$' TERM

# The stub: `docker ps ...` prints $STUB_RUNNING one name per line, `docker
# context show` prints $STUB_CONTEXT, and everything else is appended to
# $STUB_LOG and succeeds.
mkdir -p "$scratch/bin"
cat >"$scratch/bin/docker" <<'STUB'
#!/usr/bin/env bash
case "${1:-} ${2:-}" in
  "ps "*) for n in ${STUB_RUNNING:-}; do echo "$n"; done ;;
  "context show") echo "${STUB_CONTEXT:-orbstack}" ;;
  *) echo "docker $*" >>"$STUB_LOG" ;;
esac
STUB
chmod +x "$scratch/bin/docker"
export PATH="$scratch/bin:$PATH" STUB_LOG="$scratch/docker.log"
# Never the real locks a live run on this machine may be holding (#690, #724):
# a `live_session gitea` below would otherwise queue behind a real `just
# gitea-live`, or take the lock out from under one.
export KNOBAS_LIVE_LOCK="$scratch/jvm.lock" KNOBAS_LIVE_GITEA_LOCK="$scratch/gitea.lock"
unset KNOBAS_LIVE_LOCK_HOLDER KNOBAS_LIVE_GITEA_LOCK_HOLDER
# This machine's products, whatever testenv/live.env says: a developer whose
# file names a remote target must not have these tests ssh anywhere.
export KNOBAS_LIVE_TARGET=local
unset KNOBAS_LIVE_SSH KNOBAS_LIVE_ONE_JVM DOCKER_HOST

# shellcheck source=testenv/live-lib.sh disable=SC1091
. "$here/live-lib.sh"
# Anything that would write -- seed-state.json, a compose call's working
# directory -- lands in the scratch directory, never in this tree's testenv/,
# even on a mutant that gets past a refusal.
LIVE_TESTENV=$scratch/testenv
mkdir -p "$LIVE_TESTENV"

failures=0
checks=0

fail() {
  printf 'live-test: FAIL %s\n' "$1" >&2
  shift
  printf '  %s\n' "$@" >&2
  failures=$((failures + 1))
}

# check_guard <what> <expected: ok|refused> <product> [<message needle>]
# Runs `live_guard <product>` against the containers in $STUB_RUNNING. A
# refusal must be ONE line on stderr, contain the needle, and leave no call
# in the stub's log.
check_guard() {
  local what=$1 expected=$2 product=$3 needle=${4:-} outcome=ok err lines
  checks=$((checks + 1))
  : >"$STUB_LOG"
  live_guard "$product" 2>"$scratch/err" || outcome=refused
  err=$(cat "$scratch/err")
  if [ "$outcome" != "$expected" ]; then
    fail "$what" "expected: $expected" "actual:   $outcome" "stderr:   $err"
    return
  fi
  [ "$expected" = refused ] || return 0
  lines=$(printf '%s\n' "$err" | grep -c . || true)
  [ "$lines" -eq 1 ] || fail "$what: the refusal is one line" "got $lines: $err"
  case $err in *"$needle"*) ;; *) fail "$what: the refusal names $needle" "got: $err" ;; esac
  [ ! -s "$STUB_LOG" ] || fail "$what: a refusal touches nothing" "$(cat "$STUB_LOG")"
}

# --- one JVM product at a time ----------------------------------------------

STUB_RUNNING="" check_guard "an empty engine lets Jira start" ok jira
STUB_RUNNING="knobas-gitea knobas-uptime-kuma" \
  check_guard "Gitea and Kuma are not JVM products" ok teamcity

STUB_RUNNING="knobas-gitea knobas-teamcity knobas-teamcity-agent" \
  check_guard "Jira is refused while TeamCity is up" refused jira knobas-teamcity
STUB_RUNNING="knobas-jira-db knobas-jira" \
  check_guard "Confluence is refused while Jira is up" refused confluence knobas-jira
STUB_RUNNING="knobas-confluence-db knobas-confluence" \
  check_guard "TeamCity is refused while Confluence is up" refused teamcity knobas-confluence
STUB_RUNNING="knobas-confluence-db knobas-confluence" \
  check_guard "Jira is refused while Confluence is up" refused jira knobas-confluence

STUB_RUNNING="knobas-teamcity knobas-teamcity-agent" \
  check_guard "a product already up may be brought up again" ok teamcity
STUB_RUNNING="knobas-jira-db" \
  check_guard "a database on its own is not a JVM product" ok confluence
STUB_RUNNING="knobas-jira knobas-jira-db" \
  check_guard "Gitea is never refused" ok gitea

check_guard "an unknown product is refused" refused kuma "unknown product"

# --- never a remote engine ---------------------------------------------------

STUB_CONTEXT=knobas-jira \
  check_guard "a context left from the Hetzner servers is refused" refused jira knobas-jira

# check_down_refused <what> -- `live_down jira` must refuse before any call.
check_down_refused() {
  checks=$((checks + 1))
  : >"$STUB_LOG"
  if live_down jira 2>"$scratch/err"; then
    fail "$1" "expected: refused" "actual:   ok"
  elif [ -s "$STUB_LOG" ]; then
    fail "$1: a refusal touches nothing" "$(cat "$STUB_LOG")"
  fi
}
STUB_CONTEXT=knobas-jira check_down_refused "a server's docker context cannot take a product down either"

# --- what a run puts back ---------------------------------------------------

# check_eq <what> <expected> <actual>
check_eq() {
  checks=$((checks + 1))
  [ "$2" = "$3" ] || fail "$1" "expected: $2" "actual:   $3"
}
# check_contains <what> <haystack> <needle>
check_contains() {
  checks=$((checks + 1))
  case $2 in *"$3"*) ;; *) fail "$1" "wanted: $3" "got:    $2" ;; esac
}

check_eq "TeamCity's run needs Gitea, and Gitea first" \
  "gitea teamcity" "$(live_needs teamcity)"
check_eq "Jira needs nothing else" "jira" "$(live_needs jira)"

check_eq "from an empty engine a TeamCity run puts back TeamCity only" \
  "teamcity" "$(STUB_RUNNING="" live_to_put_back teamcity)"
check_eq "a Gitea that was up stays up after a TeamCity run" \
  "teamcity" "$(STUB_RUNNING="knobas-gitea" live_to_put_back teamcity)"
check_eq "a product that was up stays up after its run" \
  "" "$(STUB_RUNNING="knobas-gitea knobas-jira knobas-jira-db" live_to_put_back jira)"
check_eq "a Gitea run from an empty engine does not stop Gitea" \
  "" "$(STUB_RUNNING="" live_to_put_back gitea)"
check_eq "a Gitea run from an empty engine leaves the Gitea it started up" \
  "gitea" "$(STUB_RUNNING="" live_left_up gitea)"
check_eq "a Gitea run leaves nothing up it did not start" \
  "" "$(STUB_RUNNING="knobas-gitea" live_left_up gitea)"
check_eq "a Jira run from an empty engine leaves nothing up" \
  "" "$(STUB_RUNNING="" live_left_up jira)"
check_eq "a TeamCity run from an empty engine leaves the Gitea it started up" \
  "gitea" "$(STUB_RUNNING="" live_left_up teamcity)"
check_eq "a TeamCity run leaves nothing up it did not start" \
  "" "$(STUB_RUNNING="knobas-gitea" live_left_up teamcity)"

# --- a shared Gitea outlives the JVM run that started it (#698) -------------

# A TeamCity run takes no Gitea lock (that one serialises Gitea runs, #724), so
# a `just gitea-live` from another worktree may start using the Gitea a
# TeamCity run brought up. If the TeamCity run then stopped "what it
# started", that run would lose Gitea mid-suite. The
# rule: a run stops only its own product, never one it merely needed.
#
# Witnessed whole, through `live_session`: a stateful docker stub whose `ps`
# answers from what `compose up` started and `compose stop` stopped (the stub
# above answers `ps` from a fixed list, which cannot show Gitea outliving a run), a fake
# `seed-teamcity.sh` that starts a Gitea user (`live_session gitea`, in the
# background, holding Gitea until told to let go) while the TeamCity run is
# inside its seed, and a check after the TeamCity run's EXIT trap has run.
shared=$scratch/shared
mkdir -p "$shared/bin" "$shared/testenv"
cat >"$shared/bin/docker" <<'STUB'
#!/usr/bin/env bash
# `ps` reads $STUB_STATE; `compose ... up ... <svc>` adds knobas-<svc> to it,
# `compose ... stop <svc>` removes it; every call but `ps` and `context` is logged.
case "${1:-} ${2:-}" in
  "ps "*) cat "$STUB_STATE" 2>/dev/null; exit 0 ;;
  "context show") echo orbstack; exit 0 ;;
esac
echo "docker $*" >>"$STUB_LOG"
verb="" svc=""
for a in "$@"; do
  case $a in
    up | stop) verb=$a ;;
    gitea | teamcity | teamcity-agent)
      [ -n "$verb" ] || continue
      if [ "$verb" = up ]; then
        grep -x -- "knobas-$a" "$STUB_STATE" >/dev/null 2>&1 || echo "knobas-$a" >>"$STUB_STATE"
      else
        grep -vx -- "knobas-$a" "$STUB_STATE" >"$STUB_STATE.tmp" 2>/dev/null || true
        mv "$STUB_STATE.tmp" "$STUB_STATE"
      fi ;;
  esac
done
STUB
chmod +x "$shared/bin/docker"
# The host's view of Gitea (#722), by state rather than by a count of probes
# (#1961), since the session polls under a guard in seconds: $STUB_CURL_DEAD
# holds `dead` (never answers), `until-stop` (answers once Gitea was stopped,
# the forward a stop + up heals) or `late <seconds>` (answers that long after
# the first probe, a Gitea that comes up late). Absent, it answers. A miss says
# what curl says.
cat >"$shared/bin/curl" <<'STUB'
#!/usr/bin/env bash
read -r mode at <"${STUB_CURL_DEAD:-/nonexistent}" 2>/dev/null || mode=
miss() { echo "curl: (7) Failed to connect to 127.0.0.1 port 3000 after 0 ms: Couldn't connect to server" >&2; exit 7; }
case $mode in
  dead) miss ;;
  until-stop) grep -q ' stop .*gitea' "$STUB_LOG" 2>/dev/null || miss ;;
  late)
    [ -s "$STUB_CURL_DEAD.first" ] || date +%s >"$STUB_CURL_DEAD.first"
    [ "$(date +%s)" -ge $(($(cat "$STUB_CURL_DEAD.first") + at)) ] || miss ;;
esac
exit 0
STUB
chmod +x "$shared/bin/curl"
printf '#!/usr/bin/env bash\nexit 0\n' >"$shared/testenv/seed-gitea.sh"
printf '#!/usr/bin/env bash\nexit 0\n' >"$shared/testenv/seed"
chmod +x "$shared/testenv/seed-gitea.sh" "$shared/testenv/seed"
# The TeamCity seed is where the other worktree's Gitea run turns up.
cat >"$shared/testenv/seed-teamcity.sh" <<SEED
#!/usr/bin/env bash
(
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-user
  echo "gitea user: using it" >>"$shared/user.out"
  # Bounded, so an interrupted test leaves no orphan polling a deleted directory.
  for ((i = 0; i < 300; i++)); do [ ! -e "$shared/user.release" ] || break; sleep 0.1; done
  echo "gitea user: done"
) >>"$shared/user.out" 2>&1 </dev/null &
for ((i = 0; i < 100; i++)); do
  grep -q 'using it' "$shared/user.out" 2>/dev/null && exit 0
  sleep 0.1
done
echo "the Gitea user never started" >&2
exit 1
SEED
chmod +x "$shared/testenv/seed-teamcity.sh"

: >"$shared/state"
: >"$shared/docker.log"
rc=0
# shellcheck disable=SC2030  # its own subshell on purpose: the exports and traps end with it
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export KNOBAS_LIVE_LOCK="$shared/jvm.lock" KNOBAS_LIVE_LOCK_WAIT=5 KNOBAS_LIVE_LOCK_POLL=0.1
  unset KNOBAS_LIVE_LOCK_HOLDER
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session teamcity teamcity-run
) >"$shared/teamcity.out" 2>&1 || rc=$?
check_eq "the TeamCity session with a Gitea user beside it runs" "0" "$rc"
grep -q 'using it' "$shared/user.out" 2>/dev/null ||
  fail "the Gitea user turned up during the TeamCity run" "$(cat "$shared/user.out" 2>/dev/null)"
check_eq "the TeamCity run stopped its own server and agent" "knobas-gitea" "$(cat "$shared/state")"
check_contains "the TeamCity run says it leaves the Gitea it started up" \
  "$(cat "$shared/teamcity.out")" "gitea was started for this run and stays up"
checks=$((checks + 1))
# Any way of taking it down, not only `compose stop`: a `down`, an `rm`, a kill.
if grep -E '(^| )(stop|down|rm|kill)( .*)? (knobas-)?gitea( |$)' "$shared/docker.log" >/dev/null; then
  fail "a TeamCity run that started Gitea does not stop it under a Gitea user" "$(cat "$shared/docker.log")"
fi
touch "$shared/user.release"
# Let the user go before the scratch directory goes with this script's exit.
for ((i = 0; i < 100; i++)); do
  grep -q 'gitea user: done' "$shared/user.out" 2>/dev/null && break
  sleep 0.1
done
grep -q 'gitea user: done' "$shared/user.out" 2>/dev/null ||
  fail "the Gitea user let go when told to" "$(cat "$shared/user.out" 2>/dev/null)"

# The same rule for a Gitea run (#698): a `just gitea-live` that started Gitea
# leaves it up too, since a TeamCity run or a second Gitea run from another
# worktree may be cloning from it by then. Same stateful stub, from empty.
: >"$shared/state"
: >"$shared/docker.log"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-run
) >"$shared/gitea.out" 2>&1 || rc=$?
check_eq "a Gitea session from an empty engine runs" "0" "$rc"
check_eq "a Gitea run that started Gitea leaves it up" "knobas-gitea" "$(cat "$shared/state")"
check_contains "the Gitea run says it leaves the Gitea it started up" \
  "$(cat "$shared/gitea.out")" "gitea was started for this run and stays up"
checks=$((checks + 1))
case $(cat "$shared/gitea.out") in
  *"was already up"*) fail "the Gitea run does not claim a Gitea it started was already up" "$(cat "$shared/gitea.out")" ;;
esac
checks=$((checks + 1))
if grep -E '(^| )(stop|down|rm|kill)( .*)? (knobas-)?gitea( |$)' "$shared/docker.log" >/dev/null; then
  fail "a Gitea run that started Gitea does not stop it" "$(cat "$shared/docker.log")"
fi

# A Gitea that comes up healthy inside but unreachable from the host (#722):
# the OrbStack port forward a recreate kills. The session heals it with stop +
# up -- never restart, never a second recreate -- and goes on; the probes are
# all that is stubbed, and they answer only once Gitea was stopped and upped.
: >"$shared/state"
: >"$shared/docker.log"
echo until-stop >"$shared/curl.dead"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export STUB_CURL_DEAD="$shared/curl.dead" KNOBAS_LIVE_GITEA_WAIT_S=1 KNOBAS_LIVE_GITEA_PROBE_POLL=0.1
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-heal
) >"$shared/heal.out" 2>&1 || rc=$?
check_eq "a Gitea session whose port forward died heals and runs" "0" "$rc"
check_contains "the session says why it stops Gitea" \
  "$(cat "$shared/heal.out")" "does not answer (the OrbStack port forward a recreate kills); stop + up"
check_eq "the heal is up, stop, up -- no restart, no recreate" \
  "up stop up" "$(grep -oE ' (up|stop|restart|--force-recreate)( |$)' "$shared/docker.log" | tr -d ' ' | tr '\n' ' ' | sed 's/ $//')"
check_eq "Gitea is up after the heal" "knobas-gitea" "$(cat "$shared/state")"

# And one that stays dead is a failed session, not a suite run against nothing.
: >"$shared/state"
: >"$shared/docker.log"
echo dead >"$shared/curl.dead"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export STUB_CURL_DEAD="$shared/curl.dead" KNOBAS_LIVE_GITEA_WAIT_S=1 KNOBAS_LIVE_GITEA_PROBE_POLL=0.1
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-dead
) >"$shared/dead.out" 2>&1 || rc=$?
checks=$((checks + 1))
[ "$rc" != 0 ] || fail "a Gitea the host cannot reach after stop + up fails the session" "$(cat "$shared/dead.out")"
check_contains "and says so" "$(cat "$shared/dead.out")" "still does not answer after stop + up"
check_contains "and names the guard and what it last saw" "$(cat "$shared/dead.out")" \
  "did not answer within 1 s (KNOBAS_LIVE_GITEA_WAIT_S); last seen: curl: (7) Failed to connect"

# A Gitea that answers late is waited for, not healed (#1961): the probe polls
# under a guard in seconds rather than counting tries. Three seconds after the
# first probe under a ten-second guard, polled every 0.1 s, which ten counted
# tries ran out on.
: >"$shared/state"
: >"$shared/docker.log"
echo "late 3" >"$shared/curl.dead"
rm -f "$shared/curl.dead.first"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export STUB_CURL_DEAD="$shared/curl.dead" KNOBAS_LIVE_GITEA_WAIT_S=10 KNOBAS_LIVE_GITEA_PROBE_POLL=0.1
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-late
) >"$shared/late.out" 2>&1 || rc=$?
check_eq "a Gitea that answers late runs" "0" "$rc"
check_eq "and is not stopped to heal it" "up" \
  "$(grep -oE ' (up|stop|restart|--force-recreate)( |$)' "$shared/docker.log" | tr -d ' ' | tr '\n' ' ' | sed 's/ $//')"

# A guard that is not whole seconds is refused, naming the variable.
: >"$shared/state"
: >"$shared/docker.log"
rm -f "$shared/curl.dead"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export STUB_CURL_DEAD="$shared/curl.dead" KNOBAS_LIVE_GITEA_WAIT_S=1.5
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_session gitea gitea-badguard
) >"$shared/badguard.out" 2>&1 || rc=$?
checks=$((checks + 1))
[ "$rc" != 0 ] || fail "a guard that is not whole seconds fails the session" "$(cat "$shared/badguard.out")"
check_contains "and names the variable" "$(cat "$shared/badguard.out")" \
  "KNOBAS_LIVE_GITEA_WAIT_S must be a whole number of seconds, got '1.5'"
check_eq "and starts nothing" "" "$(grep -oE ' (up|stop)( |$)' "$shared/docker.log" || true)"

# Without the Gitea lock -- `just live-up gitea`, a TeamCity run's Gitea -- a
# dead forward is a failure that names the healing recipe, and Gitea is not
# stopped: another worktree's Gitea run may hold the lock (#699, #724).
: >"$shared/state"
: >"$shared/docker.log"
echo dead >"$shared/curl.dead"
rc=0
# shellcheck disable=SC2030,SC2031  # likewise its own subshell
(
  export PATH="$shared/bin:$PATH" STUB_STATE="$shared/state" STUB_LOG="$shared/docker.log"
  export STUB_CURL_DEAD="$shared/curl.dead" KNOBAS_LIVE_GITEA_WAIT_S=1 KNOBAS_LIVE_GITEA_PROBE_POLL=0.1
  unset KNOBAS_LIVE_GITEA_LOCK_HOLDER
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  LIVE_TESTENV=$shared/testenv
  live_up gitea
) >"$shared/unlocked.out" 2>&1 || rc=$?
checks=$((checks + 1))
[ "$rc" != 0 ] || fail "a dead forward outside the Gitea lock fails the start" "$(cat "$shared/unlocked.out")"
check_contains "and names the recipe that heals it" "$(cat "$shared/unlocked.out")" "'just gitea-live' heals it under the lock"
checks=$((checks + 1))
if grep -E '(^| )(stop|down|rm|kill)( .*)? (knobas-)?gitea( |$)' "$shared/docker.log" >/dev/null; then
  fail "a start outside the Gitea lock never stops Gitea" "$(cat "$shared/docker.log")"
fi


# --- a remote target (testenv/live.env) --------------------------------------

# Each case sources live-lib.sh in its own subshell, with $1 as live.env in a
# scratch testenv/ (the library reads the file next to itself), and prints
# what it decided.
remote_lib=$scratch/remote/testenv
mkdir -p "$remote_lib"
cp "$here/live-lib.sh" "$remote_lib/"
# check_target <what> <live.env content> <script> <needle> [ok|refused]
check_target() {
  local what=$1 content=$2 script=$3 needle=$4 expected=${5:-ok} out rc=0
  checks=$((checks + 1))
  printf '%s\n' "$content" >"$remote_lib/live.env"
  out=$( (
    unset KNOBAS_LIVE_TARGET KNOBAS_LIVE_SSH KNOBAS_LIVE_ONE_JVM DOCKER_HOST
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$remote_lib/live-lib.sh" || exit 3
    eval "$script"
  ) 2>&1 ) || rc=$?
  if [ "$expected" = ok ] && [ "$rc" != 0 ]; then
    fail "$what" "exit $rc: $out"
  elif [ "$expected" = refused ] && [ "$rc" = 0 ]; then
    fail "$what: refused" "got: $out"
  fi
  case $out in *"$needle"*) ;; *) fail "$what: says '$needle'" "got: $out" ;; esac
}
remote_env=$'KNOBAS_LIVE_TARGET=remote\nKNOBAS_LIVE_SSH=knobas-wsl'
# shellcheck disable=SC2016  # expanded by the case, after it sources live-lib.sh
check_target "a remote target points docker at its engine" "$remote_env" \
  'echo "host=$DOCKER_HOST"' "host=ssh://knobas-wsl"
# shellcheck disable=SC2016  # expanded by the case, after it sources live-lib.sh
check_target "quotes, CRLF and export are read" $'export KNOBAS_LIVE_TARGET="remote"\r\nKNOBAS_LIVE_SSH=\'wsl box\'' \
  'echo "host=$DOCKER_HOST"' "host=ssh://wsl box"
check_target "no file line runs as code" $'KNOBAS_LIVE_TARGET=remote\nKNOBAS_LIVE_SSH=x\ntouch /tmp/x' \
  'echo sourced' "is not a KNOBAS_LIVE_*=value line" refused
check_target "remote without a machine is refused" "KNOBAS_LIVE_TARGET=remote" \
  'echo sourced' "names no machine" refused
check_target "an unknown target is refused" "KNOBAS_LIVE_TARGET=hetzner" \
  'echo sourced' "must be local or remote" refused
checks=$((checks + 1))
printf '%s\n' "$remote_env" >"$remote_lib/live.env"
# shellcheck disable=SC2030,SC2031  # its own subshell on purpose
out=$( (
  export KNOBAS_LIVE_TARGET=local
  unset DOCKER_HOST
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$remote_lib/live-lib.sh"
  echo "target=$LIVE_TARGET host=${DOCKER_HOST:-none}"
) 2>&1 )
[ "$out" = "target=local host=none" ] || fail "KNOBAS_LIVE_TARGET=local in the environment beats a remote live.env" "got: $out"
check_target "a remote target lets all three JVM products up at once" "$remote_env" \
  'STUB_RUNNING="knobas-teamcity knobas-jira" live_guard confluence && echo allowed' "allowed"
check_target "KNOBAS_LIVE_ONE_JVM=1 keeps the refusal on a remote target" "$remote_env"$'\nKNOBAS_LIVE_ONE_JVM=1' \
  'STUB_RUNNING="knobas-teamcity" live_guard confluence' "one JVM product at a time" refused
# shellcheck disable=SC2016  # expanded by the case, after it sources live-lib.sh
check_target "a remote target puts nothing back" "$remote_env" \
  'STUB_RUNNING="" ; echo "put_back=[$(live_to_put_back jira)]"' "put_back=[]"
check_target "a remote target is not refused as a stray context" "$remote_env" \
  'STUB_CONTEXT=knobas-jira live_guard gitea && echo allowed' "allowed"
# shellcheck disable=SC2016  # expanded by the case, after it sources live-lib.sh
check_target "no live.env is this machine" "" \
  'echo "target=$LIVE_TARGET put_back=[$(STUB_RUNNING="" live_to_put_back jira)]"' "target=local put_back=[jira]"
rm -f "$remote_lib/live.env"

# --- the 3-hour licence -----------------------------------------------------

# A docker whose `inspect -f {{.Created}}` answers $STUB_CREATED, and which
# logs every other call.
lic=$scratch/licence
mkdir -p "$lic/bin" "$lic/testenv"
cat >"$lic/bin/docker" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  inspect) [ -n "${STUB_CREATED:-}" ] || exit 1; echo "$STUB_CREATED"; exit 0 ;;
  ps) exit 0 ;;
esac
echo "docker $*" >>"$STUB_LOG"
STUB
chmod +x "$lic/bin/docker"
for s in seed-atlassian.sh seed-atlassian-content.sh; do
  # shellcheck disable=SC2016  # expanded by the fake seed
  printf '#!/usr/bin/env bash\necho "%s $*" >>"$STUB_LOG"\n[ ! -e "$STUB_SEED_FAILS" ] || { rm -f "$STUB_SEED_FAILS"; exit 1; }\n' "$s" >"$lic/testenv/$s"
  chmod +x "$lic/testenv/$s"
done
# check_licence <what> <age seconds|none> <seed fails once: 0|1> <needles...>
# and, after `--absent`, what the log must not hold.
check_licence() {
  local what=$1 age=$2 fails=$3 created="" out rc=0 n absent=""
  shift 3
  checks=$((checks + 1))
  : >"$lic/docker.log"
  rm -f "$lic/fails"
  [ "$fails" = 0 ] || : >"$lic/fails"
  [ "$age" = none ] || created=$(date -u -r $(($(date +%s) - age)) '+%Y-%m-%dT%H:%M:%S.123456789Z' 2>/dev/null ||
    date -u -d "@$(($(date +%s) - age))" '+%Y-%m-%dT%H:%M:%S.123456789Z')
    # shellcheck disable=SC2030,SC2031  # its own subshell on purpose
  out=$( (
    export PATH="$lic/bin:$PATH" STUB_LOG="$lic/docker.log" STUB_CREATED="$created" STUB_SEED_FAILS="$lic/fails"
    export JIRA_LICENSE_KEY=k CONFLUENCE_LICENSE_KEY=k
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=$lic/testenv
    _live_start jira
  ) 2>&1 ) || rc=$?
  out="$out"$'\n'"$(cat "$lic/docker.log")"
  for n in "$@"; do
    if [ "$n" = --absent ]; then absent=1; continue; fi
    if [ -n "$absent" ]; then
      case $out in *"$n"*) fail "$what: never '$n'" "$out" ;; esac
    else
      case $out in *"$n"*) ;; *) fail "$what: '$n'" "$out" ;; esac
    fi
  done
  echo "$rc" >"$lic/rc"
}
check_licence "a Jira with hours of licence left is re-seeded, not removed" 1800 0 \
  "min of its 3-hour licence left" "seed-atlassian.sh jira" --absent "rm --stop"
check_licence "a Jira with under an hour left is removed and set up from empty" 9000 0 \
  "removing it and setting it up from empty" "rm --stop --force jira jira-db" "seed-atlassian.sh jira"
check_licence "a Jira whose licence ran out is removed" 20000 0 \
  "about 0 min left" "rm --stop --force jira jira-db"
check_licence "no Jira container: nothing removed, seeded from empty" none 0 \
  "seed-atlassian.sh jira" --absent "rm --stop" "re-seeding"
check_licence "a seed failing on the instance that was there is retried once from empty" 1800 1 \
  "the seed failed against the instance that was already there" "rm --stop --force jira jira-db"
checks=$((checks + 1))
[ "$(cat "$lic/rc")" = 0 ] || fail "and the retry from empty passes"
check_licence "a seed failing from empty is not retried" none 1 \
  "seed-atlassian.sh jira" --absent "rm --stop"
checks=$((checks + 1))
[ "$(cat "$lic/rc")" != 0 ] || fail "and fails the start"


# --- the memory budget on a remote target ------------------------------------

# A docker answering `ps` from $STUB_RUNNING, `info` with $STUB_MEM bytes,
# `inspect` with a StartedAt from $STUB_STARTED ("<container>=<iso> ..."), and
# logging the rest.
mem=$scratch/mem
mkdir -p "$mem/bin" "$mem/testenv"
cat >"$mem/bin/docker" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  ps) for n in ${STUB_RUNNING:-}; do echo "$n"; done; exit 0 ;;
  info) echo "${STUB_MEM:-0}"; exit 0 ;;
  inspect)
    for kv in ${STUB_STARTED:-}; do [ "${kv%%=*}" != "${!#}" ] || { echo "${kv#*=}"; exit 0; }; done
    exit 1 ;;
esac
echo "docker $*" >>"$STUB_LOG"
STUB
chmod +x "$mem/bin/docker"
# check_room <what> <product> <running> <expect ok|refused> <needles...>
# (after --absent, what must not appear)
# shellcheck disable=SC2030,SC2031  # its own subshell on purpose
check_room() {
  local what=$1 product=$2 running=$3 expected=$4 out rc=0 n absent=""
  shift 4
  checks=$((checks + 1))
  : >"$mem/docker.log"
  out=$( (
    export PATH="$mem/bin:$PATH" STUB_LOG="$mem/docker.log" STUB_RUNNING="$running"
    export STUB_MEM=$((10 * 1024 * 1024 * 1024))
    export STUB_STARTED="knobas-teamcity=2026-11-01T08:00:00.1Z knobas-jira=2026-11-01T09:00:00.1Z knobas-confluence=2026-11-01T10:00:00.1Z"
    export KNOBAS_LIVE_TARGET=remote KNOBAS_LIVE_SSH=wsl-test KNOBAS_LIVE_LOCK="$mem/jvm.lock"
    unset KNOBAS_LIVE_MEM_BUDGET_MB
    # shellcheck source=testenv/live-lib.sh disable=SC1091
    . "$here/live-lib.sh"
    LIVE_TESTENV=$mem/testenv
    _live_make_room "$product"
  ) 2>&1 ) || rc=$?
  out="$out"$'\n'"$(cat "$mem/docker.log")"
  if [ "$expected" = ok ] && [ "$rc" != 0 ]; then fail "$what" "exit $rc: $out"; fi
  if [ "$expected" = refused ] && [ "$rc" = 0 ]; then fail "$what: refused" "$out"; fi
  for n in "$@"; do
    if [ "$n" = --absent ]; then absent=1; continue; fi
    if [ -n "$absent" ]; then
      case $out in *"$n"*) fail "$what: never '$n'" "$out" ;; esac
    else
      case $out in *"$n"*) ;; *) fail "$what: '$n'" "$out" ;; esac
    fi
  done
}
# 10 GB less the 1536 MB reserve is an 8704 MB budget; Gitea 512, TeamCity
# 3584, Jira 4096, Confluence 3072.
check_room "two JVM products fit beside Gitea" confluence "knobas-gitea knobas-jira" ok \
  --absent "stop"
check_room "a third stops the one started longest ago" jira "knobas-gitea knobas-teamcity knobas-confluence" ok \
  "stopping teamcity" "stop teamcity teamcity-agent" --absent "stop confluence"
check_room "one already up needs no room" jira "knobas-gitea knobas-teamcity knobas-jira knobas-confluence" ok \
  --absent "stop"
ln -s "999999 1 live-up-teamcity-kept /elsewhere" "$mem/jvm.lock"
check_room "a product a live-up keeps is never stopped" jira "knobas-gitea knobas-teamcity knobas-confluence" ok \
  "stopping confluence" --absent "stop teamcity"
rm -f "$mem/jvm.lock"
checks=$((checks + 1))
# shellcheck disable=SC2030,SC2031  # its own subshell on purpose
out=$( (
  export PATH="$mem/bin:$PATH" STUB_LOG="$mem/docker.log" STUB_RUNNING="knobas-gitea"
  export KNOBAS_LIVE_TARGET=remote KNOBAS_LIVE_SSH=wsl-test KNOBAS_LIVE_MEM_BUDGET_MB=3000
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  _live_make_room jira
) 2>&1 ) && fail "a product that cannot fit even alone is refused" "$out"
case $out in *"not starting it"*) ;; *) fail "and the refusal says why" "$out" ;; esac
checks=$((checks + 1))
# shellcheck disable=SC2030,SC2031  # its own subshell on purpose
out=$( (
  export PATH="$mem/bin:$PATH" STUB_LOG="$mem/docker.log" STUB_RUNNING="knobas-gitea knobas-teamcity knobas-jira"
  export KNOBAS_LIVE_TARGET=local
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  _live_make_room confluence && echo "no-op"
) 2>&1 )
[ "$out" = no-op ] || fail "the local target has no budget" "$out"

if [ "$failures" -gt 0 ]; then
  echo "live-test: $failures of $checks checks failed" >&2
  exit 1
fi
echo "live-test: all $checks checks passed"
