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
unset KNOBAS_TESTENV_HETZNER

# shellcheck source=testenv/live-lib.sh disable=SC1091
. "$here/live-lib.sh"

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

# --- never the Hetzner servers ----------------------------------------------

KNOBAS_TESTENV_HETZNER=1 \
  check_guard "a shell that sourced hetzner/env is refused" refused jira hetzner
STUB_CONTEXT=knobas-jira \
  check_guard "a docker context aimed at a server is refused" refused jira knobas-jira

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
KNOBAS_TESTENV_HETZNER=1 check_down_refused "a hetzner/env shell cannot take a product down either"
STUB_CONTEXT=knobas-jira check_down_refused "nor can a server's docker context"

# --- what a run puts back ---------------------------------------------------

# check_eq <what> <expected> <actual>
check_eq() {
  checks=$((checks + 1))
  [ "$2" = "$3" ] || fail "$1" "expected: $2" "actual:   $3"
}

check_eq "TeamCity's run needs Gitea, and Gitea first" \
  "gitea teamcity" "$(live_needs teamcity)"
check_eq "Jira needs nothing else" "jira" "$(live_needs jira)"

check_eq "from an empty engine a TeamCity run puts back both" \
  "teamcity gitea" "$(STUB_RUNNING="" live_to_put_back teamcity)"
check_eq "a Gitea that was up stays up after a TeamCity run" \
  "teamcity" "$(STUB_RUNNING="knobas-gitea" live_to_put_back teamcity)"
check_eq "a product that was up stays up after its run" \
  "" "$(STUB_RUNNING="knobas-gitea knobas-jira knobas-jira-db" live_to_put_back jira)"
check_eq "a Gitea run from an empty engine stops Gitea" \
  "gitea" "$(STUB_RUNNING="" live_to_put_back gitea)"

if [ "$failures" -gt 0 ]; then
  echo "live-test: $failures of $checks checks failed" >&2
  exit 1
fi
echo "live-test: all $checks checks passed"
