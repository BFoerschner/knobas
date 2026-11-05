#!/usr/bin/env bash
# seed-teamcity-builds.sh's delete decisions and assert_reports (#2480), run by
# `just witness-unit`. No server: `rest` and `gitea` are stand-ins answering what
# each check dictates and logging every call, so what deletes a build is checked
# without the shared TeamCity, where a delete cannot be undone. The script's own
# functions are lifted out of it, since it cannot be sourced: it runs on load,
# and here they run under bash, where the seed's shebang is `sh`.
# `./seed --teamcity` against the real server is the live witness of the shapes.

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
seed=$here/seed-teamcity-builds.sh

# Read by the functions lifted below, as the assignments lifted with them are.
# shellcheck disable=SC2034
FIXTURE=$here/../fixtures/tidewater/work.json
# shellcheck disable=SC2034
STATE=/dev/null

# A top-level `name() {` to its closing `}`, or the one line of a one-liner.
lift() {
  awk -v want=" $* " '
    !inside && match($0, /^[a-z_]+\(\) +\{/) {
      name = substr($0, 1, index($0, "(") - 1)
      if (index(want, " " name " ")) { print; inside = ($0 !~ /; \}( *#.*)?$/) }
      next
    }
    inside { print; if ($0 == "}") inside = 0 }
  ' "$seed"
}
eval "$(grep -E '^(SEEDED_[A-Z_]+|GITEA_INTERNAL_URL|GITEA_ORG|CHAIN_PROJECT|CHANGE_[A-Z]+)=' "$seed")"
eval "$(lift say die jqf expect expect_object seeded_folder_files seeded_root_files \
             failed_test_name failed_test_details fixture_log tc_escape step_script \
             project_of name_of repo_for_project vcs_root_id add_step ensure_build_type \
             assert_reports ensure_current_build ensure_change_witness)"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-seed-teamcity-builds-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
CALLS=$scratch/calls
OUT=$scratch/out

# Each answer is "<status> <body>". What a check leaves unset answers as a whole
# server would, so a check names only the read it breaks.
url=$GITEA_INTERNAL_URL/tidewater/payout-service.git
good_build=$(jq -nc --arg u "$url" '{testOccurrences:{failed:1},
  revisions:{revision:[{"vcs-root-instance":{properties:{property:[{name:"url",value:$u}]}}}]}}')
names() { jq -nc '{file: ($ARGS.positional | map({name: .}))}' --args "$@"; }
good_root=$(names reports sepa-chart.png sepa-report.txt sepa-statement.pdf)
good_folder=$(names sepa-chart.png sepa-report.txt sepa-statement.pdf)
good_failed=$(jq -nc --arg n "$(failed_test_name Payout_IntegrationTests)" '{testOccurrence:[{name:$n}]}')
good_agent='{"agent":{"name":"knobas-agent"}}'
good_state='{"state":"finished"}'
good_changes='{"change":[{"version":"abc123"}]}'
good_branch='{"commit":{"id":"abc123"}}'

answer() {  # answer "<status> <body>": REST_STATUS and REST_BODY, as `rest` sets them
  REST_STATUS=${1%% *}
  REST_BODY=${1#"$REST_STATUS"}; REST_BODY=${REST_BODY# }
}
rest() {
  echo "$1 $2" >>"$CALLS"
  case "$1 $2" in
    "DELETE "*) answer "${A_DELETE:-204 }" ;;
    *'/steps') answer '200 {"count":0}' ;;
    *'/artifacts/children/'*) answer "${A_FOLDER:-200 $good_folder}" ;;
    *'/artifacts?'*) answer "${A_ROOT:-200 $good_root}" ;;
    *'/testOccurrences?'*) answer "${A_FAILED:-200 $good_failed}" ;;
    *'fields=agent'*) answer "${A_AGENT:-200 $good_agent}" ;;
    *'fields=state'*) answer "${A_STATE:-200 $good_state}" ;;
    *'/app/rest/changes?'*) answer "${A_CHANGES:-200 $good_changes}" ;;
    *'/app/rest/builds?locator=buildType:'*) answer '200 {"build":[{"id":137},{"id":138}]}' ;;
    *) answer "${A_BUILD:-200 $good_build}" ;;
  esac
}
gitea() {
  echo "GITEA $1 $2" >>"$CALLS"
  # shellcheck disable=SC2034
  GITEA_STATUS=${A_GITEA_STATUS:-200}
  # shellcheck disable=SC2034
  GITEA_BODY=${A_GITEA_BODY-$good_branch}
}

failures=0 checks=0
fail() {
  failures=$((failures + 1))
  { printf 'seed-teamcity-builds-test: FAIL %s: %s\n' "$1" "$2"; sed 's/^/  /' "$OUT" "$CALLS"; } >&2
}

run() {  # run <shell words...>: `set -e`, as the script runs, in a subshell; leaves rc
  : >"$CALLS"
  set +e
  ( set -e; FOUND_ID=7; "$@"; echo "FOUND_ID=$FOUND_ID" ) >"$OUT" 2>&1
  rc=$?
  set -e
}

# check <what> <kept|deleted|refused> <text the output must hold> <build id> <FAILURE|SUCCESS>
check() {
  local what=$1 want=$2 text=$3; shift 3
  checks=$((checks + 1))
  run ensure_current_build "$@"
  local deleted=0; if grep -q '^DELETE ' "$CALLS"; then deleted=1; fi
  case "$want" in
    kept)
      if ! { [ "$rc" = 0 ] && [ "$deleted" = 0 ] && grep -q '^FOUND_ID=7$' "$OUT"; }; then
        fail "$what" "wanted the build left alone (rc $rc, deleted $deleted)"; return
      fi ;;
    deleted)
      if ! { [ "$rc" = 0 ] && [ "$deleted" = 1 ] && grep -q '^FOUND_ID=$' "$OUT"; }; then
        fail "$what" "wanted the build deleted (rc $rc, deleted $deleted)"; return
      fi ;;
    refused)
      if ! { [ "$rc" != 0 ] && [ "$deleted" = 0 ]; }; then
        fail "$what" "wanted a refusal and no delete (rc $rc, deleted $deleted)"; return
      fi ;;
  esac
  grep -qF -- "$text" "$OUT" || fail "$what" "the output lacks: $text"
}

# -- what the delete rests on: a whole answer that says the build is an older seed's.
check 'a build holding every file is left alone' kept '' 7 FAILURE
A_ROOT="200 $(names extra.bin reports sepa-chart.png sepa-report.txt sepa-statement.pdf)" \
  check 'a file a later seed added is no reason to run it again' kept '' 7 FAILURE
A_ROOT="200 $(names reports sepa-report.txt sepa-statement.pdf)" \
  check 'a missing file deletes' deleted 'it publishes no sepa-chart.png' 7 FAILURE
A_ROOT="200 $(names sepa-chart.png sepa-report.txt sepa-statement.pdf)" \
  check 'a missing folder deletes' deleted 'it publishes no reports' 7 FAILURE
A_BUILD="200 $(jq -nc --arg u "$url" '{revisions:{revision:[{"vcs-root-instance":{properties:{property:[{name:"url",value:$u}]}}}]}}')" \
  check 'a build reporting no failed test deletes' deleted 'it reports no failed test' 7 FAILURE
A_BUILD='200 {"revisions":{"revision":[{"vcs-root-instance":{"properties":{"property":[{"name":"url","value":"http://elsewhere/x.git"}]}}}]}}' \
  check 'a build from another address deletes' deleted 'it ran from another VCS root URL' 7 SUCCESS
A_ROOT='500 stub: an artifact read a passing build never reaches' \
  check 'a passed build reads no artifacts' kept '' 7 SUCCESS

# -- an answer that says nothing about the build deletes nothing.
A_ROOT='500 boom' check 'artifacts: HTTP 500' refused 'list the artifacts of build id 7: HTTP 500' 7 FAILURE
A_ROOT='000 ' check 'artifacts: no connection' refused 'HTTP 000' 7 FAILURE
A_ROOT='200 ' check 'artifacts: 200, empty body' refused 'not a JSON object' 7 FAILURE
A_ROOT='200 <html>Server is starting</html>' check 'artifacts: 200, not JSON' refused 'not a JSON object' 7 FAILURE
A_ROOT='200 {"fi' check 'artifacts: 200, cut off' refused 'not a JSON object' 7 FAILURE
A_ROOT='200 {}' check 'artifacts: 200, no list' refused 'lists no artifacts' 7 FAILURE
A_ROOT='200 {"file":[]}' check 'artifacts: 200, empty list' refused 'lists no artifacts' 7 FAILURE
A_BUILD='500 boom' check 'build: HTTP 500' refused 'read build id 7: HTTP 500' 7 FAILURE
A_BUILD='000 ' check 'build: no connection' refused 'HTTP 000' 7 SUCCESS
A_BUILD='200 ' check 'build: 200, empty body' refused 'not a JSON object' 7 FAILURE
A_BUILD='200 <html>Server is starting</html>' check 'build: 200, not JSON' refused 'not a JSON object' 7 SUCCESS
A_BUILD='200 {}' check 'build: 200, no revision' refused 'names no VCS root URL' 7 SUCCESS
A_BUILD='200 {"revisions":{"revision":[{"vcs-root-instance":{"properties":{"property":[{"name":"url","value":"http://elsewhere/x.git"}]}}}]}}' \
A_DELETE='500 boom' run ensure_current_build 7 SUCCESS
checks=$((checks + 1))
if [ "$rc" = 0 ] || ! grep -qF 'delete build id 7' "$OUT"; then fail 'a refused delete stops the seed' "rc $rc"; fi

# -- a fixture file missing from the checkout stops the seed before the step goes in.
checks=$((checks + 1))
SEEDED_BINARY_DIR=$here/teamcity-artifacts run ensure_build_type Payout_IntegrationTests
if [ "$rc" != 0 ] || ! grep -q '^POST .*/steps$' "$CALLS"; then fail 'the step goes in' "rc $rc"; fi
checks=$((checks + 1))
SEEDED_BINARY_DIR=$scratch/nowhere run ensure_build_type Payout_IntegrationTests
if [ "$rc" = 0 ] || grep -q '^POST .*/steps$' "$CALLS"; then fail 'a missing file adds no step' "rc $rc"; fi

# -- assert_reports: the comparisons the live run only saw pass.
reports() {  # reports <what> <ok|the text the refusal must hold>
  checks=$((checks + 1))
  run assert_reports 7 Payout_IntegrationTests
  if [ "$2" = ok ]; then
    [ "$rc" = 0 ] || fail "$1" "wanted it to pass (rc $rc)"
  elif [ "$rc" = 0 ]; then
    fail "$1" 'wanted it to die'
  else
    grep -qF -- "$2" "$OUT" || fail "$1" "the output lacks: $2"
  fi
}
reports 'the seed publishes exactly its files' ok
A_ROOT="200 $(names reports sepa-chart.png sepa-report.txt)" \
  reports 'a file missing at the root' 'wanted the files and the folder'
A_ROOT="200 $(names extra.bin reports sepa-chart.png sepa-report.txt sepa-statement.pdf)" \
  reports 'a file extra at the root' 'wanted the files and the folder'
A_ROOT="200 $(names sepa-chart.png sepa-report.txt sepa-statement.pdf)" \
  reports 'the folder missing at the root' 'wanted the files and the folder'
A_FOLDER="200 $(names sepa-report.txt sepa-statement.pdf)" \
  reports 'a file missing in the folder' 'in reports, wanted'
A_FOLDER="200 $(names extra.bin sepa-chart.png sepa-report.txt sepa-statement.pdf)" \
  reports 'a file extra in the folder' 'in reports, wanted'
A_FOLDER='404 no such folder' reports 'no folder at all' 'list the reports folder of build id 7: HTTP 404'

# -- the change witness: the pair stays unless an answer says it is not good.
RECORD='{"sha":"abc123","before_id":137,"after_id":138}'
jq() {  # the recorded pair, without a seed-state.json; every other call is jq's
  if [ "$*" = "-c .teamcity.changes // empty /dev/null" ]; then printf '%s\n' "$RECORD"; else command jq "$@"; fi
}
witness() {  # witness <what> <kept|remade|refused> [<text>]
  checks=$((checks + 1))
  # A remake stops at the first function not lifted here, past its deletes: enough.
  run ensure_change_witness
  local deleted=0; if grep -q '^DELETE ' "$CALLS"; then deleted=1; fi
  case "$2" in
    kept)
      if ! { [ "$rc" = 0 ] && [ "$deleted" = 0 ] && grep -qF 'the change witness is there' "$OUT"; }; then
        fail "$1" "wanted the pair kept (rc $rc, deleted $deleted)"
      fi ;;
    remade) [ "$deleted" = 1 ] || fail "$1" "wanted the pair remade (rc $rc)" ;;
    refused)
      if ! { [ "$rc" != 0 ] && [ "$deleted" = 0 ]; }; then
        fail "$1" "wanted a refusal and no delete (rc $rc, deleted $deleted)"
      fi ;;
  esac
  [ -z "${3:-}" ] || grep -qF -- "$3" "$OUT" || fail "$1" "the output lacks: $3"
}
witness 'the pair is there' kept
A_AGENT='200 {"agent":{"name":"another-agent"}}' witness 'the pair ran on another agent' remade
A_AGENT='404 gone' witness 'a build of the pair is gone' remade
A_STATE='404 gone' witness 'the first build is gone' remade
A_CHANGES='200 {"change":[]}' witness 'the build lists no change' remade
A_GITEA_STATUS=404 A_GITEA_BODY='{}' witness 'the branch is gone' remade
A_GITEA_BODY='{"commit":{"id":"elsewhere"}}' witness 'the branch moved' remade
A_AGENT='500 boom' witness 'agent: HTTP 500' refused 'HTTP 500'
A_AGENT='000 ' witness 'agent: no connection' refused 'HTTP 000'
A_AGENT='200 <html>' witness 'agent: 200, not JSON' refused 'not a JSON object'
A_AGENT='200 {}' witness 'agent: 200, names none' refused 'names no agent'
A_STATE='500 boom' witness 'build: HTTP 500' refused 'HTTP 500'
A_CHANGES='500 boom' witness 'changes: HTTP 500' refused 'HTTP 500'
A_CHANGES='200 ' witness 'changes: 200, empty body' refused 'not a JSON object'
A_CHANGES='200 {}' witness 'changes: 200, no list' refused 'no change list'
A_GITEA_STATUS=500 A_GITEA_BODY=boom witness 'branch: HTTP 500' refused 'HTTP 500'
A_GITEA_STATUS=000 A_GITEA_BODY='' witness 'branch: no connection' refused 'HTTP 000'
A_GITEA_BODY='<html>' witness 'branch: 200, not JSON' refused 'no commit'

if [ "$failures" != 0 ]; then
  echo "seed-teamcity-builds-test: $failures of $checks failed" >&2
  exit 1
fi
echo "seed-teamcity-builds-test: $checks passed"
